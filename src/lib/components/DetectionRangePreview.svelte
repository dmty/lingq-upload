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
    blockedReason = null,
    onConfirm,
    onRefine,
  }: {
    preview: DetectionPreview;
    chapters: ChapterMeta[];
    busy: boolean;
    blockedReason?: string | null;
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
  {#if (preview.atom_starts ?? []).length > 0}
    <p class="font-serif text-[10px] tracking-[0.18em] text-success uppercase">
      Heard each audio part
    </p>
    <ol class="mt-2 space-y-1" data-testid="detection-atom-starts">
      {#each preview.atom_starts ?? [] as start (start.track_index)}
        <li class="text-sm text-fg">
          Part {start.track_index + 1}
          <span class="text-fg-muted"> → </span>
          {chapterLabel(start.chapter_id)}
        </li>
      {/each}
    </ol>
    <p class="mt-2 text-xs text-fg-muted">
      {preview.align_source === "title" ? "Embedded titles" : "Transcription"}
      · {Math.round(preview.confidence * 100)}%
      · text between these openings is packed onto that part
    </p>
  {:else}
    <p class="font-serif text-[10px] tracking-[0.18em] text-success uppercase">
      Proposed EPUB span
    </p>
    <p class="mt-1 text-sm font-medium text-fg">
      {chapterLabel(preview.range.start_chapter_id)}
      <span class="text-fg-muted"> → </span>
      {chapterLabel(preview.range.end_chapter_id)}
    </p>
    <p class="mt-1 text-xs text-fg-muted">
      {preview.align_source === "title" ? "Embedded titles" : "Transcription"}
      · {Math.round(preview.confidence * 100)}%
      · every text chapter in this span shares the audio parts
    </p>
  {/if}

  {#if preview.transcript_head_preview}
    <p class="mt-2 font-serif text-sm italic text-fg">
      “{preview.transcript_head_preview}”
    </p>
  {/if}
  {#if preview.transcript_tail_preview}
    <p class="mt-1 font-serif text-sm italic text-fg">
      “{preview.transcript_tail_preview}”
    </p>
  {/if}

  <div class="mt-3" aria-live="polite" aria-atomic="true">
    {#if blockedReason}
      <p data-testid="detection-range-validation" class="text-sm text-error">
        {blockedReason}
      </p>
    {/if}
  </div>

  <div class="mt-3 flex gap-2">
    <button
      type="button"
      class="rounded-sm bg-accent px-3 py-1.5 text-sm font-medium text-canvas hover:bg-accent-hover disabled:opacity-50"
      disabled={busy || blockedReason !== null}
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
