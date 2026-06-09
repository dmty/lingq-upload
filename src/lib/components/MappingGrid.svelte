<script lang="ts">
  import { onMount } from "svelte";
  import MappingConnector from "./MappingConnector.svelte";
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
    canContinue,
    onOp,
    onConfirmPair,
    onContinue,
  }: Props = $props();

  let gridRef = $state<HTMLDivElement | null>(null);
  let chapterRowRefs: Record<string, HTMLElement | null> = $state({});
  let trackRowRefs: Record<string, HTMLElement | null> = $state({});
  // Bump to force recompute of connector geometry on layout changes.
  let layoutTick = $state(0);

  onMount(() => {
    if (!gridRef) return;
    const ro = new ResizeObserver(() => {
      layoutTick++;
    });
    ro.observe(gridRef);
    const onScroll = () => layoutTick++;
    window.addEventListener("scroll", onScroll, true);
    window.addEventListener("resize", onScroll);
    return () => {
      ro.disconnect();
      window.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("resize", onScroll);
    };
  });

  function chapterTitle(id: string): string {
    return chapters.find((c) => c.id === id)?.title ?? id;
  }

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

  function confidenceClass(c: number): string {
    if (c >= 0.8) return "border-l-4 border-l-success";
    if (c >= 0.6) return "border-l-4 border-l-warning";
    return "border-l-4 border-l-error";
  }

  function confidenceLabel(c: number): string {
    if (c >= 0.8) return "high";
    if (c >= 0.6) return "med";
    return "low";
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

  function formatDuration(sec: number | null): string {
    if (sec == null) return "";
    const total = Math.max(0, Math.round(sec));
    const m = Math.floor(total / 60);
    const s = total % 60;
    return `${m}:${s.toString().padStart(2, "0")}`;
  }

  function relativeFromMs(ms: number | null): string {
    if (!ms) return "never";
    const delta = Math.max(0, Date.now() - ms);
    if (delta < 5_000) return "just now";
    if (delta < 60_000) return `${Math.round(delta / 1000)}s ago`;
    if (delta < 3_600_000) return `${Math.round(delta / 60_000)}m ago`;
    return `${Math.round(delta / 3_600_000)}h ago`;
  }

  // Re-render the "just now → 30s ago" footer text passively (no animation).
  let footerTick = $state(0);
  $effect(() => {
    const t = setInterval(() => {
      footerTick++;
    }, 5_000);
    return () => clearInterval(t);
  });
  const savedLabel = $derived.by(() => {
    void footerTick;
    return relativeFromMs(lastSavedAt);
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
  <div class="relative grid grid-cols-[1fr_120px_1fr] gap-0">
    <!-- Left column: chapters -->
    <ul class="space-y-1" data-testid="mapping-chapter-col">
      {#each chapters as chapter (chapter.id)}
        {@const pair = pairFor(chapter.id)}
        {@const conf = pair?.confidence ?? 0}
        <li
          bind:this={chapterRowRefs[chapter.id]}
          class="flex items-center gap-2 rounded-sm bg-surface px-2 py-1.5 text-sm {pair
            ? confidenceClass(conf)
            : 'border-l-4 border-l-transparent'}"
          data-testid="mapping-chapter-row"
          data-chapter-id={chapter.id}
          ondragover={onChapterDragOver}
          ondrop={(ev) => onChapterDrop(ev, chapter.id)}
        >
          <span class="flex-1 truncate text-fg">{chapter.title}</span>
          {#if pair}
            <span
              class="rounded-sm bg-surface-sunken px-1.5 py-0.5 text-[10px] uppercase tracking-wide {conf >=
              0.8
                ? 'text-success'
                : conf >= 0.6
                  ? 'text-warning'
                  : 'text-error'}"
              data-testid="confidence-chip"
              data-confidence={conf}
              data-confidence-band={confidenceLabel(conf)}
            >
              {confidenceLabel(conf)}
            </span>
          {/if}
          <button
            type="button"
            class="rounded-sm border border-border bg-surface px-1.5 py-0.5 text-[10px] hover:bg-surface-sunken"
            data-testid="confirm-pair"
            data-chapter-id={chapter.id}
            onclick={() => onConfirmPair(chapter.id)}
          >
            Confirm
          </button>
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

    <div class="col-start-2"></div>

    <!-- Right column: tracks -->
    <ul class="space-y-1" data-testid="mapping-track-col">
      {#each tracks as track (track.id)}
        <li
          bind:this={trackRowRefs[track.id]}
          class="flex cursor-grab items-center gap-2 rounded-sm bg-surface px-2 py-1.5 text-sm active:cursor-grabbing"
          data-testid="mapping-track-row"
          data-track-id={track.id}
          draggable="true"
          ondragstart={(ev) => onTrackDragStart(ev, track.id)}
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
      All changes saved · {savedLabel}
    </span>
    <span class="group relative">
      <button
        type="button"
        onclick={onContinue}
        disabled={!canContinue}
        class="rounded-sm bg-accent px-3 py-1.5 text-sm font-medium text-white hover:bg-accent-hover disabled:bg-fg-subtle"
        data-testid="mapping-continue"
        aria-disabled={!canContinue}
        title={canContinue
          ? ""
          : "Confirm or swap the rows with low confidence to continue."}
      >
        Continue
      </button>
    </span>
  </footer>
</div>
