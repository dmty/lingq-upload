<script lang="ts">
  import type { MismatchCondition } from "$lib/ipc/bindings";

  let {
    title,
    chapters,
    tracks,
    condition,
  }: {
    title: string;
    chapters: number;
    tracks: number;
    condition: MismatchCondition;
  } = $props();

  const explanation = $derived.by(() => {
    switch (condition) {
      case "one_to_many":
        return `1 text chapter but ${tracks} audio tracks.`;
      case "many_to_one":
        return `${chapters} text chapters but only 1 audio file.`;
      case "many_to_few":
        return `${tracks} audio chapters found in the file, but the text has ${chapters} chapters.`;
      case "count_off":
        return `${chapters} text chapters vs ${tracks} audio tracks — close, but not equal.`;
      case "unalignable":
        return `${chapters} text chapters vs ${tracks} audio tracks — too far apart to safely auto-pair.`;
      case "unknown":
        return "Unknown condition from a newer build — cannot proceed.";
    }
  });
</script>

<div class="rounded-md border border-border bg-surface p-4">
  <p class="text-sm text-fg-muted">Chapter counts don't match.</p>
  <p class="mt-1 text-base">
    <em class="font-medium">{title}</em>
  </p>
  <p class="mt-2 text-sm text-fg">{explanation}</p>
</div>
