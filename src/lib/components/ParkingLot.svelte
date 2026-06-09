<script lang="ts">
  /**
   * Drop target + visible list of parked tracks. Dragging a track row onto
   * the lot emits `Park`; clicking a parked track focuses a chapter picker
   * (a small inline list of unpaired chapter ids) which on selection emits
   * `Unpark`.
   */
  type Props = {
    parked: string[];
    unpairedChapterIds: string[];
    chapterTitleById: Record<string, string>;
    onPark: (trackId: string) => void;
    onUnpark: (trackId: string, chapterId: string) => void;
  };

  const {
    parked,
    unpairedChapterIds,
    chapterTitleById,
    onPark,
    onUnpark,
  }: Props = $props();

  let dragOver = $state(false);
  let activePick = $state<string | null>(null);

  function handleDragOver(ev: DragEvent) {
    ev.preventDefault();
    dragOver = true;
  }

  function handleDragLeave() {
    dragOver = false;
  }

  function handleDrop(ev: DragEvent) {
    ev.preventDefault();
    dragOver = false;
    const tid = ev.dataTransfer?.getData("application/x-track-id");
    if (tid) onPark(tid);
  }
</script>

<section
  class="rounded-md border border-dashed p-3 transition-colors {dragOver
    ? 'border-accent bg-accent/5'
    : 'border-border bg-surface'}"
  data-testid="parking-lot"
  aria-label="Parking lot"
  ondragover={handleDragOver}
  ondragleave={handleDragLeave}
  ondrop={handleDrop}
>
  <header class="mb-2 flex items-center justify-between text-xs text-fg-muted">
    <span class="font-medium">Parking lot</span>
    <span data-testid="parking-lot-count">{parked.length}</span>
  </header>

  {#if parked.length === 0}
    <p class="text-xs text-fg-muted">Drop tracks here to exclude them from upload.</p>
  {:else}
    <ul class="space-y-1">
      {#each parked as track (track)}
        <li
          class="rounded-sm border border-border bg-surface-sunken px-2 py-1 text-xs text-fg"
          data-testid="parked-track"
          data-track-id={track}
        >
          <div class="flex items-center justify-between gap-2">
            <span class="truncate">{track}</span>
            <button
              type="button"
              class="text-[10px] uppercase tracking-wide text-fg-muted hover:text-fg"
              data-testid="parked-track-restore"
              onclick={() => (activePick = activePick === track ? null : track)}
            >
              Restore
            </button>
          </div>
          {#if activePick === track}
            <div class="mt-1 flex flex-wrap gap-1" data-testid="parked-track-chapters">
              {#if unpairedChapterIds.length === 0}
                <span class="text-[10px] text-fg-muted">No unpaired chapters.</span>
              {/if}
              {#each unpairedChapterIds as cid (cid)}
                <button
                  type="button"
                  class="rounded-sm border border-border bg-surface px-1.5 py-0.5 text-[10px] hover:bg-surface-sunken"
                  onclick={() => {
                    activePick = null;
                    onUnpark(track, cid);
                  }}
                >
                  {chapterTitleById[cid] ?? cid}
                </button>
              {/each}
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>
