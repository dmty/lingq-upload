<script lang="ts">
  import { mapping } from "$lib/stores/mapping.svelte";

  let id = $derived(mapping.selectedChapterId);
  let title = $derived(
    id ? (mapping.chapters.find((c) => c.id === id)?.title ?? "") : "",
  );
  let body = $derived(id ? mapping.chapterTextFor(id) : null);
  let audio = $derived(mapping.selectedBucketAudio());
  let el: HTMLAudioElement | undefined = $state();
  let moves = $derived(id ? mapping.adjacentTracksFor(id) : []);
  let menuOpen = $state(false);

  function onPlay() {
    if (el && audio) el.currentTime = audio.start;
  }
  function onTimeUpdate() {
    if (el && audio && el.currentTime >= audio.end) el.pause();
  }
</script>

{#if id}
  <aside data-testid="chapter-inspector" class="inspector">
    <header class="inspector__title">{title}</header>
    {#if audio}
      <audio
        bind:this={el}
        data-testid="inspector-audio"
        data-window-start={audio.start}
        data-window-end={audio.end}
        src={audio.src}
        controls
        onplay={onPlay}
        ontimeupdate={onTimeUpdate}
      ></audio>
    {/if}
    <div data-testid="inspector-text" class="inspector__text">
      {#if body === null}
        <span class="inspector__loading">Loading…</span>
      {:else}
        {body}
      {/if}
    </div>
    <div class="inspector__actions">
      {#if moves.length}
        <button data-testid="inspector-move" onclick={() => (menuOpen = !menuOpen)}>Move to ▾</button>
        {#if menuOpen}
          <ul class="inspector__menu">
            {#each moves as mv}
              <li>
                <button
                  data-testid="inspector-move-option"
                  onclick={() => { mapping.moveChapter(id!, mv.trackId); menuOpen = false; }}
                >{mv.label}</button>
              </li>
            {/each}
          </ul>
        {/if}
      {/if}
      <button data-testid="inspector-remove" onclick={() => mapping.removeChapter(id!)}>Remove chapter</button>
    </div>
  </aside>
{/if}

<style>
  .inspector {
    width: 340px;
    border-left: 1px solid var(--color-border);
    padding: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    overflow: hidden;
  }
  .inspector__title {
    font-weight: 600;
  }
  .inspector__text {
    overflow-y: auto;
    white-space: pre-wrap;
    mask-image: linear-gradient(to bottom, black 85%, transparent);
  }
  .inspector__actions {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    position: relative;
  }
  .inspector__menu {
    list-style: none;
    margin: 0;
    padding: 0;
    position: absolute;
    top: 100%;
    left: 0;
    background: var(--color-surface, #fff);
    border: 1px solid var(--color-border);
    border-radius: 4px;
    z-index: 10;
    min-width: 120px;
  }
  .inspector__menu li button {
    width: 100%;
    text-align: left;
    padding: 0.4rem 0.75rem;
    background: none;
    border: none;
    cursor: pointer;
  }
</style>
