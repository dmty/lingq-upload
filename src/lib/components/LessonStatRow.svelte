<script lang="ts">
  import type { LessonStat } from "$lib/ipc/bindings";
  import { formatClock, formatCount } from "$lib/format";

  let { index, lesson }: { index: number; lesson: LessonStat } = $props();

  const percent = $derived(
    lesson.percent_completed == null ? null : Math.round(lesson.percent_completed),
  );

  const clock = (seconds: number | null) =>
    seconds == null ? "—" : formatClock(seconds);
</script>

<div
  data-testid="lesson-row"
  class="grid grid-cols-[2.5rem_1fr_5rem_4rem_4rem_5rem] items-center gap-3 py-2"
>
  <span class="tabular-nums text-fg-subtle">{index + 1}</span>
  <span class="truncate">{lesson.title}</span>
  <span class="text-right tabular-nums text-fg-muted">{formatCount(lesson.word_count)}</span>
  <span class="text-right tabular-nums text-fg-muted">{formatCount(lesson.new_words_count)}</span>
  <span class="text-right tabular-nums text-fg-muted">{clock(lesson.duration)}</span>
  <span class="flex items-center gap-2">
    <span class="h-1 flex-1 rounded-sm bg-surface-sunken">
      <span class="block h-1 rounded-sm bg-accent" style="width: {percent ?? 0}%"></span>
    </span>
    <span class="w-9 text-right text-xs tabular-nums text-fg-muted">
      {percent == null ? "—" : `${percent}%`}
    </span>
  </span>
</div>
