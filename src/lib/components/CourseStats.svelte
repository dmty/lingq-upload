<script lang="ts">
  import type { CourseView, LessonStat } from "$lib/ipc/bindings";
  import { formatAge, formatCount } from "$lib/format";
  import Button from "$lib/components/Button.svelte";
  import Spinner from "$lib/components/Spinner.svelte";

  let {
    view = null,
    fetchedAt = null,
    revalidating = false,
    refreshFailed = false,
    onrefresh,
  }: {
    view?: CourseView | null;
    fetchedAt?: number | null;
    revalidating?: boolean;
    refreshFailed?: boolean;
    onrefresh?: () => void;
  } = $props();

  // The label is the only signal that a mount skipped its fetch, so it has to
  // age while the screen sits open — a derived-once value would read "just
  // now" for the full fifteen minutes.
  let now = $state(Date.now());
  $effect(() => {
    const id = setInterval(() => (now = Date.now()), 30_000);
    return () => clearInterval(id);
  });

  const freshness = $derived(
    fetchedAt == null ? null : formatAge(new Date(fetchedAt).toISOString(), now),
  );

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

  function hoursMinutes(seconds: number | null): string {
    if (seconds == null) return "—";
    const total = Math.round(seconds / 60);
    const h = Math.floor(total / 60);
    const m = total % 60;
    return h > 0 ? `${h}h ${m}m` : `${m}m`;
  }

  const cells = $derived([
    { id: "stat-lessons", label: "lessons", value: formatCount(lessons) },
    { id: "stat-words", label: "words", value: formatCount(words) },
    { id: "stat-unique-words", label: "unique words", value: formatCount(uniqueWords) },
    { id: "stat-new-words", label: "new words", value: formatCount(newWords) },
    { id: "stat-audio", label: "audio", value: hoursMinutes(audio) },
  ]);

  const progress = $derived.by(() => {
    if (!view || view.lessons.length === 0) return null;
    const knownWordCounts = view.lessons
      .filter((l) => l.percent_completed != null && (l.word_count ?? 0) > 0)
      .map((l) => l.word_count as number);
    // Lessons missing a word count still need a weight, or they'd be
    // dropped from the percentage while still counting toward "read". Give
    // them the mean word count of their siblings — with no siblings that
    // have one, every weight is 1 and this collapses to a flat mean.
    const fallbackWeight =
      knownWordCounts.length > 0
        ? knownWordCounts.reduce((a, b) => a + b, 0) / knownWordCounts.length
        : 1;

    let weighted = 0;
    let weight = 0;
    let counted = 0;
    let read = 0;
    for (const l of view.lessons) {
      const pct = l.percent_completed;
      if (pct == null) continue;
      counted += 1;
      const w = (l.word_count ?? 0) > 0 ? (l.word_count as number) : fallbackWeight;
      weighted += pct * w;
      weight += w;
      if (pct >= 100) read += 1;
    }
    if (counted === 0) return null;
    return { percent: Math.round(weighted / weight), read, total: view.lessons.length };
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

<div class="mb-2 flex items-center justify-end gap-2 text-xs text-fg-muted">
  {#if revalidating}
    <span data-testid="course-revalidating" class="flex items-center gap-1">
      <Spinner size="sm" tone="muted" /> Refreshing
    </span>
  {:else}
    {#if refreshFailed}
      <span data-testid="course-refresh-failed" class="text-warning">Couldn't refresh</span>
    {/if}
    {#if freshness}
      <span data-testid="course-freshness">updated {freshness}</span>
    {/if}
  {/if}
  <Button variant="secondary" size="sm" data-testid="course-refresh" onclick={() => onrefresh?.()}>
    Refresh
  </Button>
</div>
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
