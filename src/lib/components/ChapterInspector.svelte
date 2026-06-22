<script lang="ts">
  import { mapping } from "$lib/stores/mapping.svelte";

  let id = $derived(mapping.selectedChapterId);
  let title = $derived(
    id ? (mapping.chapters.find((c) => c.id === id)?.title ?? "") : "",
  );
  let body = $derived(id ? mapping.chapterTextFor(id) : null);
</script>

{#if id}
  <aside data-testid="chapter-inspector" class="inspector">
    <header class="inspector__title">{title}</header>
    <div data-testid="inspector-text" class="inspector__text">
      {#if body === null}
        <span class="inspector__loading">Loading…</span>
      {:else}
        {body}
      {/if}
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
</style>
