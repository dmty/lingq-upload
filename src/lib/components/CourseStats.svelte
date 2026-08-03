<script lang="ts">
  import type { CourseView, LessonStat } from "$lib/ipc/bindings";

  let { view = null }: { view?: CourseView | null } = $props();

  const lessons = $derived(
    view?.collection.lessons_count ?? view?.lessons.length ?? null,
  );

  function sum(pick: (l: LessonStat) => number | null) {
    if (!view) return null;
    let total = 0;
    let seen = false;
    for (const l of view.lessons) {
      const n = pick(l);
      if (n != null) {
        total += n;
        seen = true;
      }
    }
    return seen ? total : null;
  }

  const words = $derived(sum((l) => l.word_count));
  const uniqueWords = $derived(sum((l) => l.unique_word_count));
  const newWords = $derived(view?.collection.new_words_count ?? sum((l) => l.new_words_count));
  const audio = $derived(view?.collection.duration ?? sum((l) => l.duration));

  function count(n: number | null): string {
    return n == null ? "—" : n.toLocaleString();
  }

  function hoursMinutes(seconds: number | null): string {
    if (seconds == null) return "—";
    const total = Math.round(seconds / 60);
    const h = Math.floor(total / 60);
    const m = total % 60;
    return h > 0 ? `${h}h ${m}m` : `${m}m`;
  }

  const cells = $derived([
    { id: "stat-lessons", label: "lessons", value: count(lessons) },
    { id: "stat-words", label: "words", value: count(words) },
    { id: "stat-unique-words", label: "unique words", value: count(uniqueWords) },
    { id: "stat-new-words", label: "new words", value: count(newWords) },
    { id: "stat-audio", label: "audio", value: hoursMinutes(audio) },
  ]);

  const progress = $derived.by(() => {
    if (!view || view.lessons.length === 0) return null;
    let weighted = 0;
    let weight = 0;
    let unweighted = 0;
    let counted = 0;
    let read = 0;
    for (const l of view.lessons) {
      const pct = l.percent_completed;
      if (pct == null) continue;
      counted += 1;
      unweighted += pct;
      if ((l.word_count ?? 0) > 0) {
        weighted += pct * (l.word_count as number);
        weight += l.word_count as number;
      }
      if (pct >= 100) read += 1;
    }
    if (counted === 0) return null;
    // Weight by word count so a 3,000-word chapter at 100% does not count the
    // same as a 200-word one; fall back to a flat mean when word counts are
    // missing.
    const percent = weight > 0 ? weighted / weight : unweighted / counted;
    return { percent: Math.round(percent), read, total: view.lessons.length };
  });

  $effect(() => {
    const declared = view?.collection.lessons_count;
    const fetched = view?.lessons.length;
    if (declared != null && fetched != null && declared !== fetched) {
      console.warn(
        `course: LingQ reports ${declared} lessons but the list returned ${fetched}`,
      );
    }
  });
</script>

<div data-testid="course-stat-band" class="grid grid-cols-5 gap-4">
  {#each cells as cell (cell.id)}
    <div data-testid={cell.id}>
      {#if view}
        <div class="text-2xl tabular-nums">{cell.value}</div>
      {:else}
        <div class="h-8 w-16 animate-pulse rounded-sm bg-surface-sunken"></div>
      {/if}
      <div class="text-xs uppercase tracking-wide text-fg-subtle">{cell.label}</div>
    </div>
  {/each}
</div>
{#if progress}
  <div data-testid="course-progress" class="mt-3 flex items-center gap-3">
    <div class="h-1 flex-1 rounded-sm bg-surface-sunken">
      <div class="h-1 rounded-sm bg-accent" style="width: {progress.percent}%"></div>
    </div>
    <span class="text-xs tabular-nums text-fg-muted">
      {progress.percent}% · {progress.read} of {progress.total} read
    </span>
  </div>
{/if}
