<script lang="ts">
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onDestroy } from "svelte";

  import {
    commands,
    type AppError,
    type ChapterCandidate,
    type ChapterId,
    type ChapterMeta,
    type DetectedRange,
    type DetectionAvailability,
    type DetectionPhase,
    type DetectionPreview,
    type DetectStartResult,
    type JobEvent,
    type NoTranscriptReason,
    type ProjectId,
  } from "$lib/ipc/bindings";
  import {
    appErrorActions,
    appErrorMessage,
    type RecoveryAction,
  } from "$lib/errors";
  import DetectionRangePreview from "$lib/components/DetectionRangePreview.svelte";
  import TranscribeConsentModal from "$lib/components/TranscribeConsentModal.svelte";
  import Button from "$lib/components/Button.svelte";

  let {
    projectId,
    chapters,
    skippedIds,
    availability,
    onAvailabilityChanged,
    onConfirmDetectedRange,
    onUseWholeBook,
  }: {
    projectId: ProjectId;
    chapters: ChapterMeta[];
    skippedIds: ChapterId[];
    availability: DetectionAvailability | null;
    onAvailabilityChanged: (next: DetectionAvailability) => void;
    onConfirmDetectedRange: (
      range: DetectedRange,
      preview: DetectionPreview,
    ) => Promise<void>;
    onUseWholeBook?: () => void;
  } = $props();

  let modalOpen = $state(false);
  let trigger = $state<HTMLElement | null>(null);
  let running = $state(false);
  let terminal = $state(false);
  let pct = $state(0);
  let statusText = $state<string | null>(null);
  let preview = $state<DetectionPreview | null>(null);
  let contentResult = $state<Exclude<
    DetectStartResult,
    { kind: "detected" }
  > | null>(null);
  let selectedHead = $state<ChapterId | null>(null);
  let selectedTail = $state<ChapterId | null>(null);
  let completedAt = $state<string | null>(null);
  let error = $state<{
    message: string;
    actions: RecoveryAction[];
  } | null>(null);
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
    selectedHead = null;
    selectedTail = null;
    completedAt = null;
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
    completedAt = new Date().toISOString();
    switch (result.kind) {
      case "detected":
        preview = result.preview;
        break;
      case "low_confidence":
        contentResult = result;
        // Seeded from real top candidates only — the final-chapter option is
        // never an implicit fallback. Identical top hits are intro+credits
        // colliding on the book title: leave the end unset so we don't
        // pretend one EPUB chapter spans the whole file.
        selectedHead = result.top_head[0]?.chapter_id ?? null;
        selectedTail =
          result.top_head[0]?.chapter_id &&
          result.top_head[0].chapter_id === result.top_tail[0]?.chapter_id
            ? null
            : (result.top_tail[0]?.chapter_id ?? null);
        break;
      case "no_transcript":
        contentResult = result;
        break;
      default:
        assertNever(result);
    }
  }

  function setError(cause: AppError) {
    error = {
      message: appErrorMessage(cause),
      actions: appErrorActions(cause),
    };
  }

  function contentOutcome(reason: NoTranscriptReason): {
    copy: string;
    retryable: boolean;
  } {
    switch (reason) {
      case "empty":
        return {
          copy: "No speech was recognized in the sampled audio.",
          retryable: true,
        };
      case "content_poor":
        return {
          copy: "The sample did not contain enough distinctive words.",
          retryable: true,
        };
      case "insufficient_audio":
        return {
          copy: "The resolved audio range is too short for two safe samples.",
          retryable: false,
        };
      default:
        return assertNever(reason);
    }
  }

  const lowConfidence = $derived(
    contentResult?.kind === "low_confidence" ? contentResult : null,
  );
  const chapterById = $derived(
    new Map(chapters.map((chapter) => [chapter.id, chapter])),
  );

  function candidateOption(candidate: ChapterCandidate) {
    const chapter = chapterById.get(candidate.chapter_id);
    if (!chapter) return null;
    return {
      id: chapter.id,
      label: `Chapter ${chapter.order + 1} · ${chapter.title} · ${Math.round(
        candidate.score * 100,
      )}% match`,
    };
  }

  function candidateOptions(list: ChapterCandidate[]) {
    return list.flatMap((candidate) => {
      const option = candidateOption(candidate);
      return option ? [option] : [];
    });
  }

  const headOptions = $derived(candidateOptions(lowConfidence?.top_head ?? []));
  const tailCandidateOptions = $derived(
    candidateOptions(lowConfidence?.top_tail ?? []),
  );
  // A server that never sampled the book's end still leaves the last chapter a
  // legitimate answer — offered explicitly and warned, never preselected. Only
  // eligible chapters can bound a range, so skipped trailing chapters are not
  // offered.
  const finalChapter = $derived(
    chapters.findLast((chapter) => !skippedIds.includes(chapter.id)) ?? null,
  );
  const tailOptions = $derived(
    finalChapter &&
      !lowConfidence?.top_tail.some(
        (candidate) => candidate.chapter_id === finalChapter.id,
      )
      ? [
          ...tailCandidateOptions,
          {
            id: finalChapter.id,
            label: `Chapter ${finalChapter.order + 1} · ${finalChapter.title} · final chapter fallback (not matched by the transcript)`,
          },
        ]
      : tailCandidateOptions,
  );
  // Any candidate the current chapter set can no longer resolve makes the whole
  // suggestion untrustworthy, so it is refused rather than quietly narrowed.
  const staleCandidates = $derived(
    lowConfidence !== null &&
      (headOptions.length !== lowConfidence.top_head.length ||
        tailCandidateOptions.length !== lowConfidence.top_tail.length ||
        headOptions.length === 0 ||
        tailCandidateOptions.length === 0),
  );

  function orderOf(id: ChapterId | null): number | null {
    if (!id) return null;
    return chapterById.get(id)?.order ?? null;
  }

  function scoreOf(list: ChapterCandidate[], id: ChapterId | null): number[] {
    const hit = list.find((candidate) => candidate.chapter_id === id);
    return hit ? [Math.min(1, Math.max(0, hit.score))] : [];
  }

  const eligibleCount = $derived(
    chapters.filter((chapter) => !skippedIds.includes(chapter.id)).length,
  );
  const partCount = $derived(Math.max(1, availability?.track_count ?? 1));
  const sameTopHit = $derived(
    Boolean(
      lowConfidence?.top_head[0] &&
        lowConfidence.top_tail[0] &&
        lowConfidence.top_head[0].chapter_id ===
          lowConfidence.top_tail[0].chapter_id &&
        eligibleCount > 1,
    ),
  );

  function rangeBlockedReason(
    startId: ChapterId | null,
    endId: ChapterId | null,
  ): string | null {
    const start = orderOf(startId);
    const end = orderOf(endId);
    if (start !== null && end !== null && start > end) {
      return "The end chapter comes before the start chapter. Choose an end chapter at or after it.";
    }
    if (startId !== null && endId !== null && startId === endId && eligibleCount > 1) {
      return "Start and end are the same chapter. That would drop the rest of the book onto one lesson. Pick a later end, or split all audio parts across the book.";
    }
    return null;
  }

  // Recomputed from current chapter metadata, not from the candidate orders the
  // server reported.
  const blockedReason = $derived(rangeBlockedReason(selectedHead, selectedTail));
  const detectedBlockedReason = $derived(
    preview
      ? rangeBlockedReason(
          preview.range.start_chapter_id,
          preview.range.end_chapter_id,
        )
      : null,
  );

  const candidatePreview = $derived.by<DetectionPreview | null>(() => {
    if (!lowConfidence || staleCandidates || !availability) return null;
    if (!selectedHead || !selectedTail) return null;
    const scores = [
      ...scoreOf(lowConfidence.top_head, selectedHead),
      ...scoreOf(lowConfidence.top_tail, selectedTail),
    ];
    return {
      provider_id: availability.active_provider.id,
      align_source: "transcript",
      range: { start_chapter_id: selectedHead, end_chapter_id: selectedTail },
      confidence: scores.length
        ? scores.reduce((sum, score) => sum + score, 0) / scores.length
        : 0,
      transcript_head_preview: lowConfidence.transcript_head_preview,
      transcript_tail_preview: lowConfidence.transcript_tail_preview,
      detected_at: completedAt ?? new Date().toISOString(),
      atom_starts: [],
    };
  });

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
          setError(ev.payload as unknown as AppError);
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

  // Exported so the route's gated auto mode runs this exact path.
  export async function startDetection() {
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
      setError(outcome.error);
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
        error = {
          message: cause instanceof Error ? cause.message : String(cause),
          actions: [],
        };
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
      <div class="mt-3" data-testid="detection-error-actions">
        <p class="text-sm text-error" role="alert">{error.message}</p>
        {#if !preview && !contentResult}
          {#if error.actions.includes("manual")}
            <p class="mt-1 text-xs text-fg-muted">
              Inspect or re-add the audio above, or choose a manual response
              below.
            </p>
          {/if}
          <div class="mt-2 flex flex-wrap gap-2">
            {#each error.actions as action (action)}
              {#if action === "settings" || action === "switch_provider"}
                <Button variant="secondary" href="/settings">
                  {action === "settings"
                    ? "Open transcription settings"
                    : "Switch transcription provider"}
                </Button>
              {:else if action === "retry"}
                <Button variant="secondary" onclick={() => void startDetection()}>
                  Try detection again
                </Button>
              {/if}
            {/each}
            <Button variant="secondary" onclick={clearOutcome}>
              Refine
            </Button>
          </div>
        {/if}
      </div>
    {/if}

    {#if preview}
      <DetectionRangePreview
        {preview}
        {chapters}
        busy={confirming}
        blockedReason={detectedBlockedReason}
        onConfirm={confirmDetectedRange}
        onRefine={clearOutcome}
      />
    {:else if lowConfidence}
      <div
        data-testid="detection-cue-sheet"
        class="mt-3 overflow-hidden rounded-md border border-border bg-surface"
      >
        <div class="border-b border-border bg-surface-sunken px-3 py-2">
          <p class="text-sm font-medium text-fg">Detection needs refinement</p>
          <p class="mt-0.5 text-xs text-fg-muted">
            Listened to the <span class="text-fg">opening of each audio part</span>.
            Interior parts are matched to the chapter they contain, not only
            the first and last files.
          </p>
        </div>

        <div class="px-3 pt-3">
          <div class="flex items-end gap-1" aria-hidden="true">
            {#each Array.from({ length: partCount }) as _, index (index)}
              <span
                class="h-2 min-w-0 flex-1 rounded-sm bg-accent"
              ></span>
            {/each}
          </div>
          <div class="mt-1 flex justify-between text-[10px] tracking-[0.18em] text-fg-subtle uppercase">
            <span>Part 1</span>
            {#if partCount > 1}
              <span>Part {partCount}</span>
            {/if}
          </div>
        </div>

        <div class="mt-3 grid gap-2 px-3 sm:grid-cols-2">
          <figure class="rounded-sm border border-accent/30 bg-accent-soft/40 px-3 py-2">
            <figcaption
              class="text-[10px] tracking-[0.18em] text-accent uppercase"
            >
              Heard at start
            </figcaption>
            <blockquote
              class="mt-1 font-serif text-sm leading-snug text-fg italic"
            >
              {lowConfidence.transcript_head_preview?.trim() ||
                "No start transcript kept."}
            </blockquote>
          </figure>
          <figure class="rounded-sm border border-warning/30 bg-warning-soft/40 px-3 py-2">
            <figcaption
              class="text-[10px] tracking-[0.18em] text-warning uppercase"
            >
              Heard at end
            </figcaption>
            <blockquote
              class="mt-1 font-serif text-sm leading-snug text-fg italic"
            >
              {lowConfidence.transcript_tail_preview?.trim() ||
                "No end transcript kept."}
            </blockquote>
          </figure>
        </div>

        {#if staleCandidates}
          <div class="px-3 py-3">
            <p class="text-xs text-fg-muted">
              These suggestions no longer match this book. Re-run detection or
              choose a manual response below.
            </p>
            <div class="mt-3 flex flex-wrap gap-2">
              <Button variant="secondary" onclick={() => void startDetection()}>
                Try detection again
              </Button>
              <Button variant="secondary" onclick={clearOutcome}>
                Refine
              </Button>
            </div>
          </div>
        {:else}
          {#if sameTopHit}
            <p
              class="mx-3 mt-3 rounded-sm border border-warning/40 bg-warning-soft px-3 py-2 text-xs text-fg"
              data-testid="detection-title-collision"
            >
              Same EPUB chapter scored 100% at both ends — usually the book
              title in the intro and credits, not a story span. Do not pick it
              for both.
            </p>
          {/if}

          {#if onUseWholeBook && partCount > 1}
            <div class="px-3 pt-3">
              <Button
                data-testid="detection-use-whole-book"
                size="lg"
                class="w-full"
                onclick={onUseWholeBook}
              >
                Split all {partCount} audio parts across the book
              </Button>
              <p class="mt-1 text-xs text-fg-muted">
                Uses each M4B chapter’s duration. Skips this start/end guess.
              </p>
            </div>
          {/if}

          {#if sameTopHit}
            <details class="px-3 py-3">
              <summary
                class="cursor-pointer text-xs font-medium text-fg-muted hover:text-fg"
              >
                Or trim title page / credits first
              </summary>
              <p class="mt-2 text-xs text-fg-muted">
                Start = first story chapter. End = last story chapter. Text
                between those two is then packed onto the {partCount} audio
                {partCount === 1 ? "part" : "parts"} — this does not map interiors.
              </p>
              <fieldset class="mt-3">
                <legend class="text-xs font-medium text-fg-muted">
                  Start chapter
                </legend>
                <p class="mt-0.5 text-[11px] text-fg-subtle">
                  First EPUB chapter that is actual story.
                </p>
                {#each headOptions as option (option.id)}
                  <label
                    class="mt-1 flex min-h-[24px] items-center gap-2 text-sm text-fg"
                  >
                    <input
                      type="radio"
                      name="detection-head"
                      value={option.id}
                      bind:group={selectedHead}
                      class="h-3.5 w-3.5"
                    />
                    <span>{option.label}</span>
                  </label>
                {/each}
              </fieldset>
              <fieldset class="mt-3">
                <legend class="text-xs font-medium text-fg-muted">
                  End chapter
                </legend>
                <p class="mt-0.5 text-[11px] text-fg-subtle">
                  Last EPUB chapter this audiobook covers. Must be after start.
                </p>
                {#each tailOptions as option (option.id)}
                  <label
                    class="mt-1 flex min-h-[24px] items-center gap-2 text-sm text-fg"
                  >
                    <input
                      type="radio"
                      name="detection-tail"
                      value={option.id}
                      bind:group={selectedTail}
                      class="h-3.5 w-3.5"
                    />
                    <span>{option.label}</span>
                  </label>
                {/each}
              </fieldset>
            </details>
          {:else}
            <p class="px-3 pt-3 text-xs text-fg-muted">
              Optional trim: start is the first story chapter (skip title page),
              end is the last story chapter (skip credits). Everything between is
              then packed onto the {partCount} audio
              {partCount === 1 ? "part" : "parts"}.
            </p>
            <fieldset class="mt-3 px-3">
              <legend class="text-xs font-medium text-fg-muted">
                Start chapter
              </legend>
              <p class="mt-0.5 text-[11px] text-fg-subtle">
                First EPUB chapter that is actual story.
              </p>
              {#each headOptions as option (option.id)}
                <label
                  class="mt-1 flex min-h-[24px] items-center gap-2 text-sm text-fg"
                >
                  <input
                    type="radio"
                    name="detection-head"
                    value={option.id}
                    bind:group={selectedHead}
                    class="h-3.5 w-3.5"
                  />
                  <span>{option.label}</span>
                </label>
              {/each}
            </fieldset>
            <fieldset class="mt-3 px-3 pb-3">
              <legend class="text-xs font-medium text-fg-muted">
                End chapter
              </legend>
              <p class="mt-0.5 text-[11px] text-fg-subtle">
                Last EPUB chapter this audiobook covers. Must be after start.
              </p>
              {#each tailOptions as option (option.id)}
                <label
                  class="mt-1 flex min-h-[24px] items-center gap-2 text-sm text-fg"
                >
                  <input
                    type="radio"
                    name="detection-tail"
                    value={option.id}
                    bind:group={selectedTail}
                    class="h-3.5 w-3.5"
                  />
                  <span>{option.label}</span>
                </label>
              {/each}
            </fieldset>
          {/if}
        {/if}
      </div>
      {#if candidatePreview}
        <DetectionRangePreview
          preview={candidatePreview}
          {chapters}
          busy={confirming}
          {blockedReason}
          onConfirm={confirmDetectedRange}
          onRefine={clearOutcome}
        />
      {/if}
    {:else if contentResult?.kind === "no_transcript"}
      {@const outcome = contentOutcome(contentResult.reason)}
      <div
        data-testid="detection-content-outcome"
        class="mt-3 rounded-md border border-border bg-surface p-3"
      >
        <p class="text-sm font-medium text-fg">Detection needs refinement</p>
        <p class="mt-1 text-xs text-fg-muted">{outcome.copy}</p>
        <p class="mt-1 text-xs text-fg-muted">
          {outcome.retryable
            ? "Try again, or choose a manual response below."
            : "Use a longer audio range, or choose a manual response below."}
        </p>
        <div class="mt-3 flex flex-wrap gap-2">
          {#if outcome.retryable}
            <Button variant="secondary" onclick={() => void startDetection()}>
              Try detection again
            </Button>
          {/if}
          <Button variant="secondary" onclick={clearOutcome}>
            Refine
          </Button>
        </div>
      </div>
    {/if}

    <div class="mt-3">
      {#if running}
        <Button variant="secondary" onclick={cancelDetection}>
          Cancel detection
        </Button>
      {:else if !preview && !contentResult && !error && availability.key_present}
        <Button onclick={requestDetection}>
          Detect audio's text range
        </Button>
      {:else if !preview && !contentResult && !error}
        <p class="text-xs text-fg-muted">
          Add an API key for {availability.active_provider.label} to use this optional
          assist.
        </p>
        <Button variant="secondary" class="mt-2" href="/settings">
          Open transcription settings
        </Button>
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
