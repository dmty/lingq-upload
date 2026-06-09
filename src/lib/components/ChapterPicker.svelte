<script lang="ts">
  import { untrack } from "svelte";

  type Kind = "body" | "front_matter" | "back_matter";

  type Row = {
    id: string;
    order: number;
    title: string;
    kind: Kind;
  };

  type Props = {
    projectId: string;
    chapters: Row[];
    skippedIds: string[];
    onChange: (skippedIds: string[]) => void;
  };

  const { projectId, chapters, skippedIds, onChange }: Props = $props();

  // Per-row checked state. `true` = include, `false` = skip.
  let checked = $state<Record<string, boolean>>({});
  let skipFrontChip = $state(false);
  // Snapshot of front-matter row state at the moment the chip went on.
  // Restored when the chip flips back off so manual edits survive.
  let preChipFront = $state<Record<string, boolean> | null>(null);
  let lastClicked = $state<string | null>(null);

  // Seed checked map from props once per chapter-set identity. The
  // `skippedIds` prop is the *initial* selection — we deliberately do not
  // re-seed when it changes downstream so user edits aren't overwritten by
  // the round-trip echo from our own onChange flush.
  let lastChaptersRef: Row[] | null = null;
  $effect(() => {
    if (chapters === lastChaptersRef) return;
    lastChaptersRef = chapters;
    untrack(() => {
      const seed: Record<string, boolean> = {};
      const skipSet = new Set(skippedIds);
      for (const c of chapters) {
        seed[c.id] = !skipSet.has(c.id);
      }
      checked = seed;
    });
  });

  function rowsSorted(): Row[] {
    return [...chapters].sort((a, b) => a.order - b.order);
  }

  function currentSkipped(): string[] {
    return chapters.filter((c) => !checked[c.id]).map((c) => c.id);
  }

  let flushTimer: ReturnType<typeof setTimeout> | null = null;
  function scheduleFlush() {
    if (flushTimer != null) clearTimeout(flushTimer);
    flushTimer = setTimeout(() => {
      onChange(currentSkipped());
      flushTimer = null;
    }, 300);
  }

  function toggleRow(id: string, shiftKey: boolean, newState: boolean) {
    const sorted = rowsSorted();
    const targetIdx = sorted.findIndex((r) => r.id === id);
    if (targetIdx < 0) return;

    if (shiftKey && lastClicked != null) {
      const anchorIdx = sorted.findIndex((r) => r.id === lastClicked);
      if (anchorIdx >= 0) {
        const [lo, hi] = anchorIdx < targetIdx
          ? [anchorIdx, targetIdx]
          : [targetIdx, anchorIdx];
        const next = { ...checked };
        for (let i = lo; i <= hi; i++) {
          next[sorted[i].id] = newState;
        }
        checked = next;
        lastClicked = id;
        scheduleFlush();
        return;
      }
    }

    checked = { ...checked, [id]: newState };
    lastClicked = id;
    scheduleFlush();
  }

  let lastShiftKey = $state(false);

  function toggleSkipFrontChip() {
    if (!skipFrontChip) {
      // Going on. Snapshot front-matter rows, then uncheck them.
      const snap: Record<string, boolean> = {};
      const next = { ...checked };
      for (const c of chapters) {
        if (c.kind === "front_matter") {
          snap[c.id] = checked[c.id] !== false;
          next[c.id] = false;
        }
      }
      preChipFront = snap;
      checked = next;
      skipFrontChip = true;
    } else {
      // Going off. Restore the snapshot to undo the chip's effect only,
      // leaving any manual edits to non-front-matter rows untouched.
      if (preChipFront != null) {
        const next = { ...checked };
        for (const [id, was] of Object.entries(preChipFront)) {
          next[id] = was;
        }
        checked = next;
      }
      preChipFront = null;
      skipFrontChip = false;
    }
    scheduleFlush();
  }

  function kindLabel(k: Kind): string {
    if (k === "front_matter") return "front";
    if (k === "back_matter") return "back";
    return "body";
  }

  const sortedChapters = $derived(rowsSorted());
  const isVirtualised = $derived(chapters.length > 100);
</script>

<aside
  class="flex h-full flex-col gap-3 border-r border-border bg-surface p-3"
  data-testid="chapter-picker"
  data-project-id={projectId}
>
  <header class="flex items-center justify-between gap-2">
    <h2 class="text-sm font-semibold text-fg">Chapters</h2>
    <button
      type="button"
      class="rounded-full border px-2 py-0.5 text-xs font-medium {skipFrontChip
        ? 'border-accent bg-accent/10 text-accent'
        : 'border-border bg-surface text-fg-muted hover:bg-surface-sunken'}"
      data-testid="skip-front-chip"
      aria-pressed={skipFrontChip}
      onclick={toggleSkipFrontChip}
    >
      Skip front-matter
    </button>
  </header>

  {#if isVirtualised}
    <!-- Virtualisation not wired: no virtual-list dep is currently a project dependency. Falls back to a plain scrollable list; expected to handle 500-row fixture without lag on modern hardware. -->
    <p class="text-[10px] text-fg-muted">{chapters.length} chapters</p>
  {/if}

  <ul
    class="flex-1 overflow-y-auto"
    data-testid="chapter-list"
  >
    {#each sortedChapters as row (row.id)}
      <li class="flex items-center gap-2 py-1">
        <input
          type="checkbox"
          class="h-4 w-4"
          checked={checked[row.id] !== false}
          data-testid="chapter-row"
          data-chapter-id={row.id}
          onmousedown={(ev) => {
            lastShiftKey = ev.shiftKey;
          }}
          onkeydown={(ev) => {
            if (ev.key === " " || ev.key === "Enter") {
              lastShiftKey = ev.shiftKey;
            }
          }}
          onchange={(ev) => {
            const target = ev.currentTarget as HTMLInputElement;
            toggleRow(row.id, lastShiftKey, target.checked);
            lastShiftKey = false;
          }}
        />
        <span class="flex-1 truncate text-sm text-fg">{row.title}</span>
        <span
          class="rounded-sm bg-surface-sunken px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-fg-muted"
        >
          {kindLabel(row.kind)}
        </span>
      </li>
    {/each}
  </ul>
</aside>
