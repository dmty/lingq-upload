<script lang="ts">
  import MappingConnector from "./MappingConnector.svelte";
  import MismatchDiffInspector from "./MismatchDiffInspector.svelte";
  import ParkingLot from "./ParkingLot.svelte";
  import type {
    ChapterMeta,
    MappingOp,
    MappingState,
  } from "$lib/ipc/bindings";

  /**
   * Two-column mapping editor. Left column = chapters; right column = tracks.
   * Connectors are rendered as SVG lines whose endpoints are derived from the
   * row bounding rects. Drag-and-drop uses native HTML5 events — track rows
   * are the only drag sources; chapter rows and the parking lot are drop
   * targets. The score gate ("Continue") is wired off the store's
   * `gateContinue()` predicate.
   */

  type TrackRow = {
    id: string;
    filename: string;
    durationSec: number | null;
  };

  type Props = {
    chapters: ChapterMeta[];
    tracks: TrackRow[];
    mappingState: MappingState | null;
    lastSavedAt: number | null;
    saving: boolean;
    canContinue: boolean;
    onOp: (op: MappingOp) => void;
    onConfirmPair: (chapterId: string) => void;
    onContinue: () => void;
  };

  const {
    chapters,
    tracks,
    mappingState,
    lastSavedAt,
    saving,
    canContinue,
    onOp,
    onConfirmPair,
    onContinue,
  }: Props = $props();

  let gridRef = $state<HTMLDivElement | null>(null);
  let chapterRowRefs: Record<string, HTMLElement | null> = $state({});
  let trackRowRefs: Record<string, HTMLElement | null> = $state({});
  // Bump to force recompute of connector geometry on layout changes.
  // rAF-coalesced: capture-phase scroll and MutationObserver bursts collapse
  // into one recompute per frame.
  let layoutTick = $state(0);
  let layoutRaf: number | null = null;
  function bumpLayout() {
    if (layoutRaf != null) return;
    layoutRaf = requestAnimationFrame(() => {
      layoutRaf = null;
      layoutTick++;
    });
  }

  $effect(() => {
    if (!gridRef) return;
    const mo = new MutationObserver(bumpLayout);
    mo.observe(gridRef, {
      childList: true,
      subtree: true,
    });
    window.addEventListener("scroll", bumpLayout, true);
    window.addEventListener("resize", bumpLayout);
    return () => {
      mo.disconnect();
      window.removeEventListener("scroll", bumpLayout, true);
      window.removeEventListener("resize", bumpLayout);
      if (layoutRaf != null) {
        cancelAnimationFrame(layoutRaf);
        layoutRaf = null;
      }
    };
  });

  type Connector = {
    chapterId: string;
    x1: number;
    y1: number;
    x2: number;
    y2: number;
    confidence: number;
  };

  function computeConnectors(): Connector[] {
    if (!mappingState || !gridRef) return [];
    void layoutTick;
    const gridRect = gridRef.getBoundingClientRect();
    const out: Connector[] = [];
    for (const pair of mappingState.pairs) {
      if (!pair.track_id) continue;
      const left = chapterRowRefs[pair.chapter_id];
      const right = trackRowRefs[pair.track_id];
      if (!left || !right) continue;
      const lr = left.getBoundingClientRect();
      const rr = right.getBoundingClientRect();
      out.push({
        chapterId: pair.chapter_id,
        x1: lr.right - gridRect.left,
        y1: lr.top + lr.height / 2 - gridRect.top,
        x2: rr.left - gridRect.left,
        y2: rr.top + rr.height / 2 - gridRect.top,
        confidence: pair.confidence,
      });
    }
    return out;
  }

  const connectors = $derived(computeConnectors());

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

  function pairFor(chapterId: string) {
    return mappingState?.pairs.find((p) => p.chapter_id === chapterId);
  }

  function onChapterDragOver(ev: DragEvent) {
    ev.preventDefault();
  }

  function onChapterDrop(ev: DragEvent, chapterId: string) {
    ev.preventDefault();
    const tid = ev.dataTransfer?.getData("application/x-track-id");
    if (!tid) return;
    onOp({ kind: "swap", chapter_id: chapterId, track_id: tid });
  }

  function onTrackDragStart(ev: DragEvent, trackId: string) {
    if (!ev.dataTransfer) return;
    ev.dataTransfer.setData("application/x-track-id", trackId);
    ev.dataTransfer.effectAllowed = "move";
  }

  // Keyboard path mirroring drag-and-drop: Enter/Space on a track selects
  // it, Enter/Space on a chapter assigns it (Swap), P parks the selected
  // track, Escape cancels.
  let selectedTrackId = $state<string | null>(null);

  function onTrackKeydown(ev: KeyboardEvent, trackId: string) {
    if (ev.key === "Escape") {
      selectedTrackId = null;
      return;
    }
    if (ev.key === "Enter" || ev.key === " ") {
      ev.preventDefault();
      selectedTrackId = selectedTrackId === trackId ? null : trackId;
      return;
    }
    if ((ev.key === "p" || ev.key === "P") && selectedTrackId === trackId) {
      ev.preventDefault();
      selectedTrackId = null;
      onOp({ kind: "park", track_id: trackId });
    }
  }

  function onChapterKeydown(ev: KeyboardEvent, chapterId: string) {
    if (ev.key === "Escape") {
      selectedTrackId = null;
      revealedPairId = null;
      return;
    }
    if (ev.key === "Enter" || ev.key === " ") {
      ev.preventDefault();
      if (selectedTrackId != null) {
        const tid = selectedTrackId;
        selectedTrackId = null;
        onOp({ kind: "swap", chapter_id: chapterId, track_id: tid });
        return;
      }
      // No track selected — Enter/Space toggles the per-row diff inspector
      // for paired rows.
      const pair = pairFor(chapterId);
      if (!pair?.track_id) return;
      revealedPairId = revealedPairId === chapterId ? null : chapterId;
    }
  }

  // Per-row diff inspector reveal. Only one row open at a time. Hover is
  // gated on a 250ms debounce so transient pointer drift doesn't fire; focus
  // (keyboard tab) reveals immediately. Enter/Space on a focused chapter row
  // also toggles. Escape collapses.
  let revealedPairId = $state<string | null>(null);
  let hoverPairId: string | null = null;
  let hoverTimer: ReturnType<typeof setTimeout> | null = null;
  const HOVER_DEBOUNCE_MS = 250;

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

  function trackLabelFor(trackId: string | null | undefined): string {
    if (!trackId) return "";
    return tracks.find((t) => t.id === trackId)?.filename ?? trackId;
  }

  function formatDuration(sec: number | null): string {
    if (sec == null) return "";
    const total = Math.max(0, Math.round(sec));
    const m = Math.floor(total / 60);
    const s = total % 60;
    return `${m}:${s.toString().padStart(2, "0")}`;
  }

  function relativeFromMs(ms: number): string {
    const delta = Math.max(0, Date.now() - ms);
    if (delta < 5_000) return "just now";
    if (delta < 60_000) return `${Math.round(delta / 1000)}s ago`;
    if (delta < 3_600_000) return `${Math.round(delta / 60_000)}m ago`;
    return `${Math.round(delta / 3_600_000)}h ago`;
  }

  // Re-render the "just now → 30s ago" footer text passively (no animation).
  // Interval runs only while there's a save timestamp to display.
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
</script>

<div
  bind:this={gridRef}
  class="relative flex w-full flex-col gap-3"
  data-testid="mapping-grid"
>
  <p class="sr-only" id="mapping-kbd-help">
    Keyboard: focus a track and press Enter or Space to select it, then focus
    a chapter and press Enter to assign the track there. Press P on a
    selected track to park it. Press Escape to cancel the selection.
  </p>

  <div class="relative grid grid-cols-[1fr_120px_1fr] gap-0">
    <!-- Left column: chapters -->
    <ul
      class="space-y-1"
      data-testid="mapping-chapter-col"
      role="listbox"
      aria-label="Chapters"
      aria-describedby="mapping-kbd-help"
    >
      {#each chapters as chapter (chapter.id)}
        {@const pair = pairFor(chapter.id)}
        {@const displayConf = pair?.original_confidence ?? pair?.confidence ?? 0}
        {@const band = confidenceBand(displayConf)}
        {@const touched = pair?.touched ?? false}
        {@const showInspector = revealedPairId === chapter.id && !!pair?.track_id}
        <li
          bind:this={chapterRowRefs[chapter.id]}
          class="flex flex-col gap-1 rounded-sm bg-surface px-2 py-1.5 text-sm {pair
            ? band.borderClass
            : 'border-l-4 border-l-transparent'}"
          data-testid="mapping-chapter-row"
          data-chapter-id={chapter.id}
          role="option"
          aria-selected={false}
          aria-label="Chapter {chapter.title}. With a track selected, press Enter to assign it here."
          tabindex={0}
          ondragover={onChapterDragOver}
          ondrop={(ev) => onChapterDrop(ev, chapter.id)}
          onkeydown={(ev) => onChapterKeydown(ev, chapter.id)}
          onpointerenter={() => onChapterPointerEnter(chapter.id)}
          onpointerleave={() => onChapterPointerLeave(chapter.id)}
          onfocus={() => onChapterFocus(chapter.id)}
          onblur={(ev) => onChapterBlur(ev, chapter.id)}
        >
          <div class="flex items-center gap-2">
            <span class="flex-1 truncate text-fg">{chapter.title}</span>
            {#if pair}
              <span
                class="rounded-sm bg-surface-sunken px-1.5 py-0.5 text-[10px] uppercase tracking-wide {band.textClass}"
                data-testid="confidence-chip"
                data-confidence={displayConf}
                data-confidence-band={band.label}
              >
                {band.label}
              </span>
              {#if touched}
                <span
                  class="rounded-sm border border-border px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-fg-muted"
                  data-testid="manual-badge"
                >
                  Manual
                </span>
              {/if}
            {/if}
            <button
              type="button"
              class="rounded-sm border border-border bg-surface px-1.5 py-0.5 text-[10px] hover:bg-surface-sunken disabled:opacity-50"
              data-testid="confirm-pair"
              data-chapter-id={chapter.id}
              disabled={!pair?.track_id}
              onclick={() => onConfirmPair(chapter.id)}
            >
              Confirm
            </button>
          </div>
          {#if showInspector}
            <MismatchDiffInspector
              chapterTitle={chapter.title}
              trackLabel={trackLabelFor(pair?.track_id)}
            />
          {/if}
        </li>
      {/each}
    </ul>

    <!-- Middle column: SVG connector layer -->
    <svg
      class="pointer-events-none absolute inset-0 h-full w-full"
      data-testid="mapping-connector-layer"
      aria-hidden="true"
    >
      {#each connectors as conn (conn.chapterId)}
        <MappingConnector
          x1={conn.x1}
          y1={conn.y1}
          x2={conn.x2}
          y2={conn.y2}
          confidence={conn.confidence}
        />
      {/each}
    </svg>

    <!-- Right column: tracks -->
    <ul
      class="col-start-3 space-y-1"
      data-testid="mapping-track-col"
      role="listbox"
      aria-label="Audio tracks"
      aria-describedby="mapping-kbd-help"
    >
      {#each tracks as track (track.id)}
        <li
          bind:this={trackRowRefs[track.id]}
          class="flex cursor-grab items-center gap-2 rounded-sm bg-surface px-2 py-1.5 text-sm active:cursor-grabbing {selectedTrackId ===
          track.id
            ? 'ring-2 ring-accent'
            : ''}"
          data-testid="mapping-track-row"
          data-track-id={track.id}
          role="option"
          aria-selected={selectedTrackId === track.id}
          aria-label="Track {track.filename}. Press Enter to select, P to park."
          tabindex={0}
          draggable="true"
          ondragstart={(ev) => onTrackDragStart(ev, track.id)}
          onkeydown={(ev) => onTrackKeydown(ev, track.id)}
        >
          <span class="flex-1 truncate text-fg">{track.filename}</span>
          {#if track.durationSec != null}
            <span class="text-[10px] text-fg-muted tabular-nums">
              {formatDuration(track.durationSec)}
            </span>
          {/if}
        </li>
      {/each}
    </ul>
  </div>

  <ParkingLot
    parked={mappingState?.parking_lot ?? []}
    unpairedChapterIds={unpairedChapterIds}
    chapterTitleById={chapterTitleById}
    onPark={(tid) => onOp({ kind: "park", track_id: tid })}
    onUnpark={(tid, cid) =>
      onOp({ kind: "unpark", track_id: tid, chapter_id: cid })}
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
