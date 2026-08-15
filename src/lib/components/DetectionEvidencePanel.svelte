<script lang="ts">
  import { tick } from "svelte";

  import type {
    ChapterMeta,
    DetectionEvidence,
    TranscribeProviderId,
  } from "$lib/ipc/bindings";

  let {
    evidence,
    chapters,
    canReset,
    resetting,
    onReset,
  }: {
    evidence: DetectionEvidence;
    chapters: ChapterMeta[];
    canReset: boolean;
    resetting: boolean;
    onReset: () => Promise<void>;
  } = $props();

  const PROVIDER_LABELS: Record<TranscribeProviderId, string> = {
    groq: "Groq",
    open_ai: "OpenAI",
  };

  let confirming = $state(false);
  let error = $state<string | null>(null);
  let trigger = $state<HTMLButtonElement | null>(null);

  // A boundary the current chapter set can no longer resolve is named by its
  // stable ID rather than guessed at — the page blocks Continue on the same
  // condition, leaving reset as the way out.
  function chapterLabel(id: string): string {
    const chapter = chapters.find((candidate) => candidate.id === id);
    return chapter
      ? `Chapter ${chapter.order + 1} · ${chapter.title}`
      : `Chapter unavailable · ${id}`;
  }

  const detectedAt = $derived(new Date(evidence.detected_at).toLocaleString());

  async function restoreFocus() {
    confirming = false;
    await tick();
    trigger?.focus();
  }

  async function confirmReset() {
    error = null;
    try {
      await onReset();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
      await restoreFocus();
    }
  }
</script>

<section
  data-testid="detection-evidence-panel"
  class="rounded-md border border-success/40 bg-success-soft/30 p-4"
  aria-labelledby="detection-evidence-title"
>
  <h2 id="detection-evidence-title" class="text-sm font-medium text-fg">
    Confirmed text range
  </h2>
  <dl class="mt-2 grid gap-1 text-sm text-fg">
    <div>
      <dt class="inline text-fg-muted">Start:</dt>
      <dd class="inline">{chapterLabel(evidence.range.start_chapter_id)}</dd>
    </div>
    <div>
      <dt class="inline text-fg-muted">End:</dt>
      <dd class="inline">{chapterLabel(evidence.range.end_chapter_id)}</dd>
    </div>
    <div>
      <dt class="inline text-fg-muted">Source:</dt>
      <dd class="inline">
        {evidence.align_source === "title"
          ? "Embedded titles"
          : "Transcription"}
      </dd>
    </div>
    <div>
      <dt class="inline text-fg-muted">Confidence:</dt>
      <dd class="inline">{Math.round(evidence.confidence * 100)}%</dd>
    </div>
    <div>
      <dt class="inline text-fg-muted">Transcription:</dt>
      <dd class="inline">
        {evidence.provider_id
          ? PROVIDER_LABELS[evidence.provider_id]
          : "No provider upload"}
      </dd>
    </div>
    <div>
      <dt class="inline text-fg-muted">Detected:</dt>
      <dd class="inline">
        <time datetime={evidence.detected_at}>{detectedAt}</time>
      </dd>
    </div>
  </dl>

  {#if evidence.transcript_head_preview}
    <details class="mt-2 text-sm text-fg">
      <summary class="cursor-pointer text-fg-muted">Start transcript</summary>
      <p class="mt-1 whitespace-pre-wrap">{evidence.transcript_head_preview}</p>
    </details>
  {/if}
  {#if evidence.transcript_tail_preview}
    <details class="mt-2 text-sm text-fg">
      <summary class="cursor-pointer text-fg-muted">End transcript</summary>
      <p class="mt-1 whitespace-pre-wrap">{evidence.transcript_tail_preview}</p>
    </details>
  {/if}

  {#if error}
    <p class="mt-3 text-sm text-error" role="alert">{error}</p>
  {/if}

  <div class="mt-3">
    {#if !canReset}
      <p class="text-xs text-fg-muted">
        Uploads have started for this project, so the detected range can no
        longer be reset.
      </p>
    {:else if confirming}
      <div class="rounded-md border border-border bg-surface p-3">
        <p class="text-sm text-fg">
          Resetting clears this range and the mapping below, and returns to
          mismatch resolution. Nothing is transcribed and no keys or consent
          change.
        </p>
        <div class="mt-3 flex gap-2">
          <button
            type="button"
            class="rounded-sm bg-accent px-3 py-1.5 text-sm font-medium text-accent-fg hover:bg-accent-hover disabled:opacity-50"
            disabled={resetting}
            onclick={confirmReset}
          >
            {resetting ? "Resetting…" : "Confirm reset"}
          </button>
          <button
            type="button"
            class="rounded-sm border border-border bg-surface px-3 py-1.5 text-sm font-medium text-fg hover:bg-surface-sunken disabled:opacity-50"
            disabled={resetting}
            onclick={restoreFocus}
          >
            Keep detected range
          </button>
        </div>
      </div>
    {:else}
      <button
        bind:this={trigger}
        type="button"
        class="rounded-sm border border-border bg-surface px-3 py-1.5 text-sm font-medium text-fg hover:bg-surface-sunken"
        onclick={() => {
          error = null;
          confirming = true;
        }}
      >
        Reset detected range
      </button>
    {/if}
  </div>
</section>
