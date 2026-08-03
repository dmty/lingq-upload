<script lang="ts">
  import type { CourseView } from "$lib/ipc/bindings";

  let { view = null }: { view?: CourseView | null } = $props();

  const lessons = $derived(
    view?.collection.lessons_count ?? view?.lessons.length ?? null,
  );

  function sum(pick: (l: NonNullable<typeof view>["lessons"][number]) => number | null) {
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
    const h = Math.floor(seconds / 3600);
    const m = Math.round((seconds % 3600) / 60);
    return h > 0 ? `${h}h ${m}m` : `${m}m`;
  }

  const cells = $derived([
    { id: "stat-lessons", label: "lessons", value: count(lessons) },
    { id: "stat-words", label: "words", value: count(words) },
    { id: "stat-unique-words", label: "unique words", value: count(uniqueWords) },
    { id: "stat-new-words", label: "new words", value: count(newWords) },
    { id: "stat-audio", label: "audio", value: hoursMinutes(audio) },
  ]);

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
