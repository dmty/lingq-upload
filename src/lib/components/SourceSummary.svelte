<script lang="ts">
  /**
   * What this project is actually made of, stated on the match screen before
   * the grid. The audio file used to be visible only after opening the
   * "Add more audio" panel, so an audio-only project looked empty.
   */
  import type { BucketMeta } from "$lib/ipc/bindings";
  import { basename } from "$lib/paths";
  import { formatDuration } from "$lib/format";

  let {
    chapterCount,
    buckets,
    audioPaths,
    hasText,
  }: {
    chapterCount: number;
    buckets: BucketMeta[];
    audioPaths: string[];
    hasText: boolean;
  } = $props();

  // Bucket paths are authoritative once a mapping exists: one m4b expands into
  // several tracks, so the file list and the track count are different facts.
  const files = $derived.by(() => {
    const fromBuckets = buckets
      .map((b) => b.audioPath)
      .filter((p): p is string => !!p);
    const source = fromBuckets.length > 0 ? fromBuckets : audioPaths;
    return [...new Set(source.map(basename))];
  });

  const totalSec = $derived(
    buckets.reduce((acc, b) => acc + (b.atomDurationSec ?? 0), 0),
  );

  const fileLabel = $derived(
    files.length === 0
      ? null
      : files.length === 1
        ? files[0]
        : `${files.length} files`,
  );

  const parts = $derived.by(() => {
    const out: string[] = [];
    if (hasText) {
      out.push(`${chapterCount} ${chapterCount === 1 ? "chapter" : "chapters"}`);
      out.push(
        buckets.length === 0
          ? "no audio"
          : `${buckets.length} ${buckets.length === 1 ? "track" : "tracks"}`,
      );
    } else {
      out.push(
        `${buckets.length} ${buckets.length === 1 ? "chapter" : "chapters"} in audio`,
      );
    }
    if (totalSec > 0) out.push(formatDuration(totalSec));
    return out;
  });
</script>

<div data-testid="source-summary" class="mt-0.5 space-y-1">
  <p class="flex items-center gap-1.5 text-xs text-fg-muted">
    {#if fileLabel}
      <span
        aria-hidden="true"
        class="shrink-0 rounded-sm bg-surface-sunken px-1 text-[10px] leading-4 text-fg-subtle"
        >♪</span
      >
      <span class="truncate text-fg" title={files.join(", ")}>{fileLabel}</span>
      <span class="text-fg-subtle">·</span>
    {/if}
    <span class="tabular whitespace-nowrap">{parts.join(" · ")}</span>
  </p>

  {#if !hasText && buckets.length > 0}
    <p
      data-testid="transcription-notice"
      class="rounded-sm border border-accent-soft bg-accent-soft/40 px-2 py-1 text-xs text-fg"
    >
      No text in this project — LingQ transcribes the audio on upload, which
      uses your Premium transcription quota.
    </p>
  {/if}
</div>
