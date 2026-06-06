<script lang="ts">
  import type { MismatchResponse } from "$lib/ipc/bindings";

  let {
    response,
    selected,
    onSelect,
  }: {
    response: MismatchResponse;
    selected: boolean;
    onSelect: () => void;
  } = $props();

  const COPY: Record<MismatchResponse, { title: string; body: string }> = {
    pair_accept: {
      title: "Pair by order",
      body: "Match chapters 1↔1, 2↔2, …; extras become standalone lessons.",
    },
    pair_drop: {
      title: "Pair, drop extras",
      body: "Match in order; discard the unmatched tail to keep counts aligned.",
    },
    single_lesson: {
      title: "Single lesson",
      body: "Concatenate all chapters into one lesson.",
    },
    cancel: {
      title: "Cancel",
      body: "Stop the job and return to Library.",
    },
  };

  const isDegraded = $derived(response === "single_lesson");
</script>

<button
  type="button"
  onclick={onSelect}
  class="flex w-full flex-col items-start gap-1 rounded-md border bg-surface px-4 py-3 text-left transition-colors duration-120 {selected
    ? 'border-accent ring-1 ring-accent shadow-card'
    : 'border-border hover:border-border-strong'}"
>
  <span class="flex items-center gap-2">
    <span class="text-sm font-medium text-fg">{COPY[response].title}</span>
    {#if isDegraded}
      <span
        class="rounded-sm bg-warning/10 px-2 py-0.5 text-[11px] font-medium text-warning"
      >
        audio not chapter-aligned
      </span>
    {/if}
  </span>
  <span class="text-xs text-fg-muted">{COPY[response].body}</span>
</button>
