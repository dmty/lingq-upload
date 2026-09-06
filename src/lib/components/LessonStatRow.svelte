<script lang="ts">
  import type { LessonStat } from "$lib/ipc/bindings";
  import { formatClock, formatCount, normalizeLessonTitle } from "$lib/format";

  let {
    index,
    lesson,
    showNewWords = true,
  }: { index: number; lesson: LessonStat; showNewWords?: boolean } = $props();

  const percent = $derived(
    lesson.percent_completed == null
      ? null
      : Math.round(lesson.percent_completed),
  );

  const title = $derived(normalizeLessonTitle(lesson.title));

  const clock = (seconds: number | null) =>
    seconds == null ? "—" : formatClock(seconds);
</script>

<div
  data-testid="lesson-row"
  class="grid items-center gap-3 border-t border-border py-1.5"
  style="grid-template-columns: 2.5rem 1fr 5rem {showNewWords
    ? '4rem '
    : ''}4rem 5rem"
>
  <span class="tabular-nums text-fg-subtle">{index + 1}</span>
  <span class="truncate" title={lesson.title}>{title}</span>
  <span class="text-right tabular-nums text-fg-muted"
    >{formatCount(lesson.word_count)}</span
  >
  {#if showNewWords}
    <span class="text-right tabular-nums text-fg-muted"
      >{formatCount(lesson.new_words_count)}</span
    >
  {/if}
  <span class="text-right tabular-nums text-fg-muted"
    >{clock(lesson.duration)}</span
  >
  <!-- A finished lesson is a checkmark, not a full bar plus "100%": the
       number is the same on every read row and says nothing. -->
  <span class="flex items-center justify-end gap-2">
    {#if percent == null}
      <span class="text-xs text-fg-subtle">—</span>
    {:else if percent >= 100}
      <svg
        width="13"
        height="13"
        viewBox="0 0 20 20"
        fill="none"
        stroke="currentColor"
        stroke-width="2.4"
        stroke-linecap="round"
        stroke-linejoin="round"
        class="text-fg-muted"
        aria-label="Read"
      >
        <path d="M4 10.5l4 4 8-9" />
      </svg>
    {:else}
      <span class="h-1 flex-1 rounded-sm bg-surface-sunken">
        <span class="block h-1 rounded-sm bg-accent" style="width: {percent}%"
        ></span>
      </span>
    {/if}
  </span>
</div>
