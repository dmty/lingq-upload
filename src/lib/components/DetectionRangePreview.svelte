<script lang="ts">
  import type {
    ChapterMeta,
    DetectedRange,
    DetectionPreview,
  } from "$lib/ipc/bindings";

  let {
    preview,
    chapters,
    busy,
    onConfirm,
    onRefine,
  }: {
    preview: DetectionPreview;
    chapters: ChapterMeta[];
    busy: boolean;
    onConfirm: (
      range: DetectedRange,
      preview: DetectionPreview,
    ) => Promise<void>;
    onRefine: () => void;
  } = $props();

  function chapterLabel(id: string): string {
    const chapter = chapters.find((candidate) => candidate.id === id);
    return chapter ? `Chapter ${chapter.order + 1} · ${chapter.title}` : id;
  }
</script>

<div
  data-testid="detection-range-preview"
  class="mt-3 rounded-md border border-success/40 bg-success-soft/30 p-3"
>
  <p class="text-sm font-medium text-fg">Detected text range</p>
  <dl class="mt-2 grid gap-1 text-sm text-fg">
    <div>
      <dt class="inline text-fg-muted">Start:</dt>
      <dd class="inline">{chapterLabel(preview.range.start_chapter_id)}</dd>
    </div>
    <div>
      <dt class="inline text-fg-muted">End:</dt>
      <dd class="inline">{chapterLabel(preview.range.end_chapter_id)}</dd>
    </div>
    <div>
      <dt class="inline text-fg-muted">Source:</dt>
      <dd class="inline">
        {preview.align_source === "title" ? "Embedded titles" : "Transcription"}
      </dd>
    </div>
    <div>
      <dt class="inline text-fg-muted">Confidence:</dt>
      <dd class="inline">{Math.round(preview.confidence * 100)}%</dd>
    </div>
  </dl>

  {#if preview.transcript_head_preview}
    <details class="mt-2 text-sm text-fg">
      <summary class="cursor-pointer text-fg-muted">Start transcript</summary>
      <p class="mt-1 whitespace-pre-wrap">{preview.transcript_head_preview}</p>
    </details>
  {/if}
  {#if preview.transcript_tail_preview}
    <details class="mt-2 text-sm text-fg">
      <summary class="cursor-pointer text-fg-muted">End transcript</summary>
      <p class="mt-1 whitespace-pre-wrap">{preview.transcript_tail_preview}</p>
    </details>
  {/if}

  <div class="mt-3 flex gap-2">
    <button
      type="button"
      class="rounded-sm bg-accent px-3 py-1.5 text-sm font-medium text-canvas hover:bg-accent-hover disabled:opacity-50"
      disabled={busy}
      onclick={() => onConfirm(preview.range, preview)}
    >
      {busy ? "Confirming…" : "Confirm detected range"}
    </button>
    <button
      type="button"
      class="rounded-sm border border-border bg-surface px-3 py-1.5 text-sm font-medium text-fg hover:bg-surface-sunken disabled:opacity-50"
      disabled={busy}
      onclick={onRefine}
    >
      Refine
    </button>
  </div>
</div>
