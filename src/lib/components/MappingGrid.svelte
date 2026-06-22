<script lang="ts">
  import MismatchDiffInspector from "./MismatchDiffInspector.svelte";
  import ParkingLot from "./ParkingLot.svelte";
  import type {
    BucketMeta,
    ChapterMeta,
    MappingOp,
    MappingState,
  } from "$lib/ipc/bindings";
  import { mapping } from "$lib/stores/mapping.svelte";

  /**
   * Banded bucket list. Included chapters (non-skipped) are numbered 1..N and
   * grouped into contiguous bands by their paired track_id. Each band shows
   * its audio metadata (title + duration) when available via the buckets prop.
   * Replaces the two-column connector grid.
   */

  type Props = {
    chapters: ChapterMeta[];
    mappingState: MappingState | null;
    buckets: BucketMeta[];
    skippedIds: string[];
    lastSavedAt: number | null;
    saving: boolean;
    canContinue: boolean;
    onOp: (op: MappingOp) => void;
    onConfirmPair: (chapterId: string) => void;
    onRemove: (chapterId: string) => void;
    onUndoRemove: () => void;
    onContinue: () => void;
  };

  const {
    chapters,
    mappingState,
    buckets,
    skippedIds,
    lastSavedAt,
    saving,
    canContinue,
    onOp,
    onConfirmPair,
    onRemove,
    onUndoRemove,
    onContinue,
  }: Props = $props();

  type ConfidenceBand = {
    borderClass: string;
    label: "high" | "med" | "low";
    textClass: string;
  };

  function confidenceBand(c: number): ConfidenceBand {
    if (c >= 0.8) {
      return {
        borderClass: "border-l-4 border-l-success",
        label: "high",
        textClass: "text-success",
      };
    }
    if (c >= 0.6) {
      return {
        borderClass: "border-l-4 border-l-warning",
        label: "med",
        textClass: "text-warning",
      };
    }
    return {
      borderClass: "border-l-4 border-l-error",
      label: "low",
      textClass: "text-error",
    };
  }

  // Included chapters in order, each tagged with its pair, a 1-based number.
  const rows = $derived.by(() => {
    const skipped = new Set(skippedIds);
    const pairByCh = new Map(
      mappingState?.pairs.map((p) => [p.chapter_id, p]) ?? [],
    );
    let n = 0;
    return chapters
      .filter((c) => !skipped.has(c.id))
      .map((c) => ({ chapter: c, pair: pairByCh.get(c.id) ?? null, number: ++n }));
  });

  type BandRow = { chapter: ChapterMeta; pair: (typeof rows)[number]["pair"]; number: number };
  type Band = { trackId: string | null; meta: BucketMeta | null; rows: BandRow[] };

  // Contiguous bands by track_id, joined to bucket metadata.
  const bands = $derived.by(() => {
    const metaByTrack = new Map(buckets.map((b) => [b.trackId, b]));
    const out: Band[] = [];
    for (const r of rows) {
      const tid = r.pair?.track_id ?? null;
      const last = out[out.length - 1];
      if (last && last.trackId === tid) {
        last.rows.push(r);
      } else {
        out.push({ trackId: tid, meta: tid ? (metaByTrack.get(tid) ?? null) : null, rows: [r] });
      }
    }
    return out;
  });

  const removedChapters = $derived.by(() => {
    const skipped = new Set(skippedIds);
    return chapters.filter((c) => skipped.has(c.id));
  });

  function fmtDur(sec: number): string {
    const t = Math.max(0, Math.round(sec));
    const m = Math.floor(t / 60);
    const s = t % 60;
    return `${m}:${s.toString().padStart(2, "0")}`;
  }

  function relativeFromMs(ms: number): string {
    const delta = Math.max(0, Date.now() - ms);
    if (delta < 5_000) return "just now";
    if (delta < 60_000) return `${Math.round(delta / 1000)}s ago`;
    if (delta < 3_600_000) return `${Math.round(delta / 60_000)}m ago`;
    return `${Math.round(delta / 3_600_000)}h ago`;
  }

  let footerTick = $state(0);
  $effect(() => {
    if (lastSavedAt == null) return;
    const t = setInterval(() => {
      footerTick++;
    }, 5_000);
    return () => clearInterval(t);
  });
  const savedLabel = $derived.by(() => {
    void footerTick;
    return lastSavedAt != null ? relativeFromMs(lastSavedAt) : "";
  });

  const unpairedChapterIds = $derived(
    (mappingState?.pairs ?? [])
      .filter((p) => !p.track_id)
      .map((p) => p.chapter_id),
  );
  const chapterTitleById = $derived(
    Object.fromEntries(chapters.map((c) => [c.id, c.title])),
  );

  // Per-row diff inspector reveal. Hover gated on 250ms debounce; focus immediate.
  let revealedPairId = $state<string | null>(null);
  let hoverPairId: string | null = null;
  let hoverTimer: ReturnType<typeof setTimeout> | null = null;
  const HOVER_DEBOUNCE_MS = 250;
  let chapterRowRefs: Record<string, HTMLElement | null> = $state({});

  function clearHoverTimer() {
    if (hoverTimer != null) {
      clearTimeout(hoverTimer);
      hoverTimer = null;
    }
  }

  function onChapterPointerEnter(chapterId: string) {
    hoverPairId = chapterId;
    clearHoverTimer();
    hoverTimer = setTimeout(() => {
      hoverTimer = null;
      if (hoverPairId === chapterId) revealedPairId = chapterId;
    }, HOVER_DEBOUNCE_MS);
  }

  function onChapterPointerLeave(chapterId: string) {
    if (hoverPairId === chapterId) hoverPairId = null;
    clearHoverTimer();
    if (revealedPairId === chapterId) revealedPairId = null;
  }

  function onChapterFocus(chapterId: string) {
    revealedPairId = chapterId;
  }

  function onChapterBlur(ev: FocusEvent, chapterId: string) {
    if (revealedPairId !== chapterId) return;
    const next = ev.relatedTarget;
    const row = chapterRowRefs[chapterId];
    if (row && next instanceof Node && row.contains(next)) return;
    revealedPairId = null;
  }

  function onChapterKeydown(ev: KeyboardEvent, chapterId: string) {
    if (ev.key === "Escape") {
      revealedPairId = null;
      return;
    }
    if (ev.key === "Enter" || ev.key === " ") {
      ev.preventDefault();
      const pair = mappingState?.pairs.find((p) => p.chapter_id === chapterId);
      if (!pair?.track_id) return;
      revealedPairId = revealedPairId === chapterId ? null : chapterId;
    }
  }

  function trackLabelFor(trackId: string | null | undefined): string {
    if (!trackId) return "";
    const b = buckets.find((b) => b.trackId === trackId);
    return b?.atomTitle ?? trackId;
  }

  function median(xs: number[]): number {
    const v = xs.filter((x) => x > 0).sort((a, b) => a - b);
    if (!v.length) return 0;
    const m = Math.floor(v.length / 2);
    return v.length % 2 ? v[m] : (v[m - 1] + v[m]) / 2;
  }

  const driftMedian = $derived(median(buckets.map((b) => b.charsPerSec)));

  function isDrift(cps: number): boolean {
    return driftMedian > 0 && cps > 0 && Math.abs(cps - driftMedian) / driftMedian > 0.3;
  }
</script>

<div data-testid="mapping-grid" class="flex w-full flex-col gap-2">
  {#each bands as band, i (`band-${i}-${band.trackId ?? "unpaired"}`)}
    <section data-testid="mapping-bucket-band" class="overflow-hidden rounded-md border border-border bg-surface">
      {#if band.meta}
        <header
          data-testid="bucket-band-meta"
          class="flex items-center gap-2 border-b border-border bg-surface-sunken px-3 py-1.5 text-xs"
        >
          <span class="font-medium text-fg">
            {band.meta.atomTitle ?? "Audio"} · {fmtDur(band.meta.atomDurationSec)}
          </span>
          {#if isDrift(band.meta.charsPerSec)}
            <span
              data-testid="bucket-drift"
              title="chars/sec deviates >±30% from the median — the narrator may have added or skipped material here."
              class="rounded-sm bg-warning/10 px-1.5 py-0.5 text-[10px] font-semibold text-warning"
            >drift</span>
          {/if}
        </header>
      {/if}
      <ul role="listbox" aria-label="Chapter rows">
        {#each band.rows as row (row.chapter.id)}
          {@const pair = row.pair}
          {@const displayConf = pair?.original_confidence ?? pair?.confidence ?? 0}
          {@const confBand = pair ? confidenceBand(displayConf) : null}
          {@const touched = pair?.touched ?? false}
          {@const showInspector = revealedPairId === row.chapter.id && !!pair?.track_id}
          {@const isSingleton = band.rows.length === 1}
          <li
            role="option"
            aria-selected={showInspector}
            bind:this={chapterRowRefs[row.chapter.id]}
            data-testid="mapping-chapter-row"
            data-chapter-id={row.chapter.id}
            class="flex flex-col gap-1 px-3 py-1.5 text-sm {pair && confBand
              ? confBand.borderClass
              : 'border-l-4 border-l-transparent'}"
            class:selected={mapping.selectedChapterId === row.chapter.id}
            tabindex={0}
            onclick={() => mapping.selectChapter(row.chapter.id)}
            onkeydown={(ev) => onChapterKeydown(ev, row.chapter.id)}
            onpointerenter={() => onChapterPointerEnter(row.chapter.id)}
            onpointerleave={() => onChapterPointerLeave(row.chapter.id)}
            onfocus={() => onChapterFocus(row.chapter.id)}
            onblur={(ev) => onChapterBlur(ev, row.chapter.id)}
          >
            <div class="flex items-center gap-2">
              <span
                data-testid="chapter-number"
                class="w-6 shrink-0 text-right text-xs tabular-nums text-fg-muted"
              >{row.number}</span>
              <span class="flex-1 truncate text-fg">{row.chapter.title}</span>
              {#if pair && isSingleton && confBand}
                <span
                  class="rounded-sm bg-surface-sunken px-1.5 py-0.5 text-[10px] uppercase tracking-wide {confBand.textClass}"
                  data-testid="confidence-chip"
                  data-confidence={displayConf}
                  data-confidence-band={confBand.label}
                >
                  {confBand.label}
                </span>
              {/if}
              {#if pair?.track_id && displayConf < 0.6 && !touched}
                <button
                  type="button"
                  class="rounded-sm border border-border bg-surface px-1.5 py-0.5 text-[10px] hover:bg-surface-sunken"
                  data-testid="confirm-pair"
                  data-chapter-id={row.chapter.id}
                  onclick={() => onConfirmPair(row.chapter.id)}
                >
                  Confirm
                </button>
              {/if}
              <button
                type="button"
                data-testid="chapter-remove"
                aria-label={`Remove ${row.chapter.title}`}
                class="text-fg-subtle hover:text-error"
                onclick={() => onRemove(row.chapter.id)}
              >×</button>
            </div>
            {#if showInspector}
              <MismatchDiffInspector
                chapterTitle={row.chapter.title}
                trackLabel={trackLabelFor(pair?.track_id)}
              />
            {/if}
          </li>
        {/each}
      </ul>
    </section>
  {/each}

  {#if removedChapters.length > 0}
    <div
      data-testid="removed-strip"
      class="rounded-md border border-dashed border-border-strong px-3 py-2 text-xs text-fg-muted"
    >
      Removed ({removedChapters.length}): {removedChapters.map((c) => c.title).join(" · ")}
      <button
        type="button"
        data-testid="removed-undo"
        class="ml-2 text-accent"
        onclick={() => onUndoRemove()}
      >undo</button>
    </div>
  {/if}

  <ParkingLot
    parked={mappingState?.parking_lot ?? []}
    unpairedChapterIds={unpairedChapterIds}
    chapterTitleById={chapterTitleById}
    onPark={(tid) => onOp({ kind: "park", track_id: tid })}
    onUnpark={(tid, cid) => onOp({ kind: "unpark", track_id: tid, chapter_id: cid })}
  />

  <footer
    class="flex items-center justify-between border-t border-border pt-2 text-xs text-fg-muted"
    data-testid="mapping-footer"
  >
    <span data-testid="mapping-saved-label">
      {#if saving}
        Saving…
      {:else if lastSavedAt != null}
        All changes saved · {savedLabel}
      {/if}
    </span>
    <span
      class="group relative"
      title={canContinue
        ? undefined
        : "Confirm or swap the rows with low confidence to continue."}
    >
      <button
        type="button"
        onclick={onContinue}
        disabled={!canContinue}
        class="rounded-sm bg-accent px-3 py-1.5 text-sm font-medium text-white hover:bg-accent-hover disabled:bg-fg-subtle"
        data-testid="mapping-continue"
        aria-disabled={!canContinue}
      >
        Continue
      </button>
    </span>
  </footer>
</div>

<style>
  li.selected {
    background-color: var(--color-accent-soft);
  }
</style>
