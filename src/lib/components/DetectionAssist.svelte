<script lang="ts">
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onDestroy } from "svelte";

  import {
    commands,
    type AppError,
    type ChapterCandidate,
    type ChapterMeta,
    type DetectedRange,
    type DetectionAvailability,
    type DetectionPhase,
    type DetectionPreview,
    type DetectStartResult,
    type JobEvent,
    type ProjectId,
  } from "$lib/ipc/bindings";
  import { appErrorMessage } from "$lib/errors";
  import DetectionRangePreview from "$lib/components/DetectionRangePreview.svelte";
  import TranscribeConsentModal from "$lib/components/TranscribeConsentModal.svelte";

  let {
    projectId,
    chapters,
    availability,
    onAvailabilityChanged,
    onConfirmDetectedRange,
  }: {
    projectId: ProjectId;
    chapters: ChapterMeta[];
    availability: DetectionAvailability | null;
    onAvailabilityChanged: (next: DetectionAvailability) => void;
    onConfirmDetectedRange: (
      range: DetectedRange,
      preview: DetectionPreview,
    ) => Promise<void>;
  } = $props();

  let modalOpen = $state(false);
  let trigger = $state<HTMLButtonElement | null>(null);
  let running = $state(false);
  let terminal = $state(false);
  let pct = $state(0);
  let statusText = $state<string | null>(null);
  let preview = $state<DetectionPreview | null>(null);
  let contentResult = $state<Exclude<
    DetectStartResult,
    { kind: "detected" }
  > | null>(null);
  let candidates = $state<ChapterCandidate[]>([]);
  let error = $state<string | null>(null);
  let confirming = $state(false);
  let activeJobId: string | null = null;
  let activeToken = 0;
  let unlisten: UnlistenFn | null = null;
  let projectScope = "";
  let destroyed = false;

  $effect(() => {
    const nextScope = projectId.content_hash;
    if (projectScope === "") {
      projectScope = nextScope;
      return;
    }
    if (nextScope === projectScope) return;
    projectScope = nextScope;
    modalOpen = false;
    invalidateRun();
    clearOutcome();
  });

  onDestroy(() => {
    destroyed = true;
    invalidateRun();
  });

  function assertNever(value: never): never {
    throw new Error(`Unhandled IPC variant: ${JSON.stringify(value)}`);
  }

  function detectionPhaseLabel(phase: DetectionPhase): string {
    switch (phase) {
      case "title_check":
        return "Checking embedded titles";
      case "sample_head":
        return "Preparing start sample";
      case "transcribe_head":
        return "Transcribing start sample";
      case "align_head":
        return "Matching start chapter";
      case "sample_tail":
        return "Preparing end sample";
      case "transcribe_tail":
        return "Transcribing end sample";
      case "align_tail":
        return "Matching end chapter";
      default:
        return assertNever(phase);
    }
  }

  function clearOutcome() {
    preview = null;
    contentResult = null;
    candidates = [];
    error = null;
    confirming = false;
    terminal = false;
    pct = 0;
    statusText = null;
  }

  function stopListening() {
    const stop = unlisten;
    unlisten = null;
    if (stop) void stop();
  }

  function invalidateRun() {
    activeToken += 1;
    activeJobId = null;
    running = false;
    stopListening();
  }

  function finish(status: string) {
    terminal = true;
    running = false;
    activeJobId = null;
    statusText = status;
    stopListening();
  }

  function applyResult(result: DetectStartResult) {
    switch (result.kind) {
      case "detected":
        preview = result.preview;
        break;
      case "low_confidence":
        contentResult = result;
        candidates = [...result.top_head, ...result.top_tail];
        break;
      case "no_transcript":
        contentResult = result;
        break;
      default:
        assertNever(result);
    }
  }

  function handleJobEvent(ev: JobEvent, jobId: string, scope: string) {
    if (
      ev.job_id !== jobId ||
      activeJobId !== jobId ||
      projectId.content_hash !== scope ||
      terminal
    ) {
      return;
    }
    switch (ev.kind) {
      case "Started":
        statusText = "Detecting text range";
        break;
      case "DetectionProgress":
        pct = Math.max(pct, ev.pct);
        statusText = detectionPhaseLabel(ev.phase);
        break;
      case "Result":
        if (ev.ok) {
          applyResult(ev.payload as unknown as DetectStartResult);
          finish("Detection complete");
        } else {
          error = appErrorMessage(ev.payload as unknown as AppError);
          finish("Detection failed");
        }
        break;
      case "Cancelled":
        finish("Detection cancelled");
        break;
      case "StageChanged":
      case "Progress":
      case "Log":
      case "ChapterDone":
      case "NeedsMatch":
        break;
      default:
        assertNever(ev);
    }
  }

  async function startDetection() {
    if (running) return;
    clearOutcome();
    const scope = projectId.content_hash;
    const detectionProjectId = projectId;
    const jobId = crypto.randomUUID();
    const token = ++activeToken;
    activeJobId = jobId;
    running = true;
    statusText = "Starting detection";

    const stop = await listen<JobEvent>("job", (event) => {
      handleJobEvent(event.payload, jobId, scope);
    });
    if (
      destroyed ||
      activeToken !== token ||
      activeJobId !== jobId ||
      projectId.content_hash !== scope
    ) {
      await stop();
      return;
    }
    unlisten = stop;

    // Paths that return before the backend emitter exists (cached evidence,
    // setup failures, transport faults) never emit a terminal event, so the
    // returned Result is the only terminal signal. `terminal` keeps whichever
    // arrives first the only one rendered.
    const outcome = await commands
      .cmdDetectStartOffset(detectionProjectId, jobId)
      .catch((cause: unknown) => ({
        status: "error" as const,
        error: {
          kind: "Other" as const,
          message: cause instanceof Error ? cause.message : String(cause),
        },
      }));
    if (destroyed || activeToken !== token || terminal) return;
    if (outcome.status === "ok") {
      applyResult(outcome.data);
      finish("Detection complete");
    } else {
      error = appErrorMessage(outcome.error);
      finish("Detection failed");
    }
  }

  function requestDetection(event: MouseEvent) {
    trigger = event.currentTarget as HTMLButtonElement;
    if (!availability?.consent_matches) modalOpen = true;
    else void startDetection();
  }

  async function acceptConsent() {
    if (!availability) return;
    const consentProjectId = projectId;
    const accepted = await commands.cmdAcceptTranscribeConsent(
      consentProjectId,
      availability.active_provider.id,
    );
    if (accepted.status === "error") {
      throw new Error(appErrorMessage(accepted.error));
    }
    if (destroyed || projectId !== consentProjectId) return;
    const refreshed = await commands.cmdDetectionAvailability(consentProjectId);
    if (refreshed.status === "error") {
      throw new Error(appErrorMessage(refreshed.error));
    }
    if (destroyed || projectId !== consentProjectId) return;
    onAvailabilityChanged(refreshed.data);
    modalOpen = false;
    await startDetection();
  }

  function cancelDetection() {
    if (activeJobId) void commands.cmdCancelJob(activeJobId);
  }

  async function confirmDetectedRange(
    range: DetectedRange,
    evidence: DetectionPreview,
  ) {
    confirming = true;
    error = null;
    try {
      await onConfirmDetectedRange(range, evidence);
    } catch (cause) {
      if (!destroyed) {
        error = cause instanceof Error ? cause.message : String(cause);
        confirming = false;
      }
    }
  }
</script>

{#if availability?.eligible}
  <section
    data-testid="detection-assist"
    class="rounded-md border border-accent-soft bg-accent-soft/30 p-4"
    aria-labelledby="detection-assist-title"
  >
    <h2 id="detection-assist-title" class="text-sm font-medium text-fg">
      Detect audio's text range
    </h2>
    <p class="mt-1 text-xs text-fg-muted">
      Optionally compare the audio boundaries with this book before choosing a
      manual response.
    </p>
    {#if running || statusText}
      <div class="mt-3" role="status" aria-live="polite" aria-atomic="true">
        <p class="text-sm text-fg">{statusText}</p>
        {#if running}
          <div class="mt-2 flex items-center gap-2">
            <progress
              class="h-2 flex-1"
              max="1"
              value={pct}
              aria-label="Detection progress"
            ></progress>
            <span class="text-xs tabular text-fg-muted"
              >{Math.round(pct * 100)}%</span
            >
          </div>
        {/if}
      </div>
    {/if}

    {#if error}
      <p class="mt-3 text-sm text-error" role="alert">{error}</p>
    {/if}

    {#if preview}
      <DetectionRangePreview
        {preview}
        {chapters}
        busy={confirming}
        onConfirm={confirmDetectedRange}
        onRefine={clearOutcome}
      />
    {:else if contentResult}
      <div class="mt-3 rounded-md border border-border bg-surface p-3">
        <p class="text-sm font-medium text-fg">Detection needs refinement</p>
        {#if contentResult.kind === "low_confidence"}
          <p class="mt-1 text-xs text-fg-muted">
            The samples matched more than one possible boundary.
          </p>
          {#if candidates.length > 0}
            <ul class="mt-2 space-y-1 text-sm text-fg">
              {#each candidates as candidate, index (`${candidate.chapter_id}:${index}`)}
                <li>{candidate.title}</li>
              {/each}
            </ul>
          {/if}
        {:else}
          <p class="mt-1 text-xs text-fg-muted">
            {contentResult.reason === "insufficient_audio"
              ? "The audio is too short to sample both boundaries."
              : contentResult.reason === "empty"
                ? "No speech was detected in the samples."
                : "The samples did not contain enough spoken text."}
          </p>
        {/if}
        <button
          type="button"
          class="mt-3 rounded-sm border border-border bg-surface px-3 py-1.5 text-sm font-medium text-fg hover:bg-surface-sunken"
          onclick={clearOutcome}
        >
          Refine
        </button>
      </div>
    {:else if error}
      <button
        type="button"
        class="mt-3 rounded-sm border border-border bg-surface px-3 py-1.5 text-sm font-medium text-fg hover:bg-surface-sunken"
        onclick={clearOutcome}
      >
        Refine
      </button>
    {/if}

    <div class="mt-3">
      {#if running}
        <button
          type="button"
          class="rounded-sm border border-border bg-surface px-3 py-1.5 text-sm font-medium text-fg hover:bg-surface-sunken"
          onclick={cancelDetection}
        >
          Cancel detection
        </button>
      {:else if !preview && !contentResult && !error && availability.key_present}
        <button
          bind:this={trigger}
          type="button"
          class="rounded-sm bg-accent px-3 py-1.5 text-sm font-medium text-canvas hover:bg-accent-hover"
          onclick={requestDetection}
        >
          Detect audio's text range
        </button>
      {:else if !preview && !contentResult && !error}
        <p class="text-xs text-fg-muted">
          Add an API key for {availability.active_provider.label} to use this optional
          assist.
        </p>
        <a
          class="mt-2 inline-flex rounded-sm border border-border bg-surface px-3 py-1.5 text-sm font-medium text-fg no-underline hover:bg-surface-sunken hover:no-underline"
          href="/settings"
        >
          Open transcription settings
        </a>
      {/if}
    </div>
  </section>

  <TranscribeConsentModal
    provider={availability.active_provider}
    open={modalOpen}
    onAccept={acceptConsent}
    onCancel={() => (modalOpen = false)}
    returnFocusTo={trigger}
  />
{/if}
