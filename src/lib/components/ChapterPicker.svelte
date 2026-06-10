<script lang="ts">
  import { untrack } from "svelte";

  /**
   * Selective chapter picker.
   *
   * Maintains per-row `checked` state, debounces flushes to the parent
   * (500 ms, matching the mapping auto-save contract), and supports
   * shift-click range toggle plus a non-destructive
   * "skip front-matter" chip. Virtualisation kicks in above 100 rows —
   * currently a no-op fallback to a plain scroll list; the 500-row fixture
   * is expected to render without lag on modern hardware.
   *
   * AD-025: a flush that fails silently reverts in the store; the parent
   * propagates revert via `revertEpoch` so we can re-align row state without
   * re-seeding on first mount.
   */

  type Kind = "body" | "front_matter" | "back_matter";

  type Row = {
    id: string;
    order: number;
    title: string;
    kind: Kind;
  };

  type Props = {
    chapters: Row[];
    skippedIds: string[];
    /** Bumps when the store reverts an optimistic write. */
    revertEpoch?: number;
    onChange: (skippedIds: string[]) => void;
    /** Flush the pending debounced edit immediately. */
    onFlush?: () => void | Promise<void>;
  };

  const {
    chapters,
    skippedIds,
    revertEpoch = 0,
    onChange,
    onFlush,
  }: Props = $props();

  // Per-row checked state. `true` = include, `false` = skip.
  let checked = $state<Record<string, boolean>>({});
  let skipFrontChip = $state(false);
  let preChipFront = $state<Record<string, boolean> | null>(null);
  let lastClicked = $state<string | null>(null);

  // Seed once per chapter-set identity. We deliberately do not re-seed when
  // `skippedIds` changes downstream so user edits aren't overwritten by the
  // round-trip echo from our own onChange flush.
  // Toggles made since the last flush. Replayed on top of a revert re-align
  // so a pending (not-yet-flushed) edit isn't lost when an older write fails.
  let pendingEdits: Record<string, boolean> = {};

  let lastChaptersRef: Row[] | null = null;
  $effect(() => {
    if (chapters === lastChaptersRef) return;
    lastChaptersRef = chapters;
    untrack(() => {
      pendingEdits = {};
      const seed: Record<string, boolean> = {};
      const skipSet = new Set(skippedIds);
      for (const c of chapters) {
        seed[c.id] = !skipSet.has(c.id);
      }
      checked = seed;
    });
  });

  // Re-align row state from skippedIds when the store reverts an optimistic
  // write. Triggered by revertEpoch ticking; first mount is skipped because
  // the chapters-seed effect above already populated checked.
  let lastRevertEpoch: number | null = null;
  $effect(() => {
    const epoch = revertEpoch;
    if (lastRevertEpoch === null) {
      lastRevertEpoch = epoch;
      return;
    }
    if (epoch === lastRevertEpoch) return;
    lastRevertEpoch = epoch;
    untrack(() => {
      const skipSet = new Set(skippedIds);
      const next: Record<string, boolean> = {};
      for (const c of chapters) {
        next[c.id] = !skipSet.has(c.id);
      }
      // Replay edits still waiting on the debounce timer — only the failed
      // (already-flushed) write reverts, not the user's newest toggles.
      for (const [id, v] of Object.entries(pendingEdits)) {
        next[id] = v;
      }
      checked = next;
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
      flushTimer = null;
      pendingEdits = {};
      onChange(currentSkipped());
    }, 500);
  }

  function flushNow() {
    if (flushTimer != null) {
      clearTimeout(flushTimer);
      flushTimer = null;
      pendingEdits = {};
      onChange(currentSkipped());
    }
  }

  // Flush pending edit on unmount so navigate-away within the debounce
  // window doesn't drop the user's selection.
  $effect(() => {
    return () => {
      flushNow();
      void onFlush?.();
    };
  });

  function toggleRow(id: string, shiftKey: boolean, newState: boolean) {
    const sorted = rowsSorted();
    const targetIdx = sorted.findIndex((r) => r.id === id);
    if (targetIdx < 0) return;

    // Shift before any prior click: treat the clicked row as the anchor
    // (Finder convention — no range op, single toggle).
    if (shiftKey && lastClicked != null) {
      const anchorIdx = sorted.findIndex((r) => r.id === lastClicked);
      if (anchorIdx >= 0) {
        const [lo, hi] = anchorIdx < targetIdx
          ? [anchorIdx, targetIdx]
          : [targetIdx, anchorIdx];
        const next = { ...checked };
        for (let i = lo; i <= hi; i++) {
          next[sorted[i].id] = newState;
          pendingEdits[sorted[i].id] = newState;
        }
        checked = next;
        lastClicked = id;
        scheduleFlush();
        return;
      }
    }

    checked = { ...checked, [id]: newState };
    pendingEdits[id] = newState;
    lastClicked = id;
    scheduleFlush();
  }

  let lastShiftKey = $state(false);

  function toggleSkipFrontChip() {
    if (!skipFrontChip) {
      const snap: Record<string, boolean> = {};
      const next = { ...checked };
      for (const c of chapters) {
        if (c.kind === "front_matter") {
          snap[c.id] = checked[c.id] !== false;
          next[c.id] = false;
          pendingEdits[c.id] = false;
        }
      }
      preChipFront = snap;
      checked = next;
      skipFrontChip = true;
    } else {
      if (preChipFront != null) {
        const next = { ...checked };
        for (const [id, was] of Object.entries(preChipFront)) {
          next[id] = was;
          pendingEdits[id] = was;
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
