<script lang="ts">
  import { diffStrings } from "$lib/utils/string-diff";

  /**
   * Per-row diagnostic showing the character-level diff between a chapter
   * title and the audio track label. Deletions (in title only) tint the
   * left line; additions (in label only) tint the right. Equal substrings
   * render unchanged so the eye can lock onto the shared anchor first.
   *
   * AD-025: no banner, no toast, no diagnostic copy — the diff itself is
   * the signal. No animations or transitions.
   */
  type Props = {
    chapterTitle: string;
    trackLabel: string;
  };
  const { chapterTitle, trackLabel }: Props = $props();

  const segments = $derived(diffStrings(chapterTitle, trackLabel));
</script>

<div
  class="rounded-sm border border-border bg-surface-sunken px-2 py-1.5 text-xs leading-relaxed text-fg"
  data-testid="mismatch-diff-inspector"
  role="group"
  aria-label="Chapter title and track label comparison"
>
  <div class="flex items-start gap-2" data-testid="mismatch-diff-title-row">
    <span class="w-10 shrink-0 text-[10px] uppercase tracking-wide text-fg-muted">
      title
    </span>
    <span class="flex-1 break-words font-mono">
      {#each segments.a as seg, i (i)}
        {#if seg.kind === "del"}
          <span
            class="bg-error/10 text-error"
            data-testid="mismatch-diff-del"
          >{seg.text}</span>
        {:else}
          <span>{seg.text}</span>
        {/if}
      {/each}
    </span>
  </div>
  <div class="mt-0.5 flex items-start gap-2" data-testid="mismatch-diff-track-row">
    <span class="w-10 shrink-0 text-[10px] uppercase tracking-wide text-fg-muted">
      track
    </span>
    <span class="flex-1 break-words font-mono">
      {#each segments.b as seg, i (i)}
        {#if seg.kind === "add"}
          <span
            class="bg-success/10 text-success"
            data-testid="mismatch-diff-add"
          >{seg.text}</span>
        {:else}
          <span>{seg.text}</span>
        {/if}
      {/each}
    </span>
  </div>
</div>
