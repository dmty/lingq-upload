<script lang="ts">
  import { page } from "$app/state";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { library } from "$lib/stores/library.svelte";
  import { course } from "$lib/stores/course.svelte";
  import { joinKey } from "$lib/identity";
  import { lingqCollectionUrl } from "$lib/lingq";
  import CoverThumb from "$lib/components/CoverThumb.svelte";
  import CourseStats from "$lib/components/CourseStats.svelte";
  import Button from "$lib/components/Button.svelte";

  const projectKey = $derived(page.params.projectId ?? "");

  const entry = $derived(
    library.index?.entries.find((e) => joinKey(e.id) === projectKey) ?? null,
  );

  const collectionId = $derived(entry?.lingq_collection_id ?? null);
  const authorLine = $derived((entry?.authors ?? []).join(" · "));

  const cached = $derived(
    entry && collectionId != null ? course.entry(entry.language, collectionId) : null,
  );

  $effect(() => {
    if (library.status === "idle") void library.load();
  });

  $effect(() => {
    if (entry && collectionId != null) {
      void course.ensure(entry.language, collectionId);
    }
  });
</script>

{#if entry == null}
  <p>That course isn't in your library. <a href="/library">Back to Library</a></p>
{:else}
  <header data-testid="course-header" class="flex items-start gap-4">
    <CoverThumb coverPath={entry.cover_path ?? null} title={entry.title} />
    <div class="flex-1">
      <h1>{entry.title}</h1>
      {#if authorLine}<p class="text-fg-muted">{authorLine}</p>{/if}
      <p class="text-sm text-fg-subtle">
        {entry.language}{cached?.view?.collection.level
          ? ` · ${cached.view.collection.level}`
          : ""}
      </p>
    </div>
    <span class="flex items-center gap-3">
      {#if collectionId != null}
        <Button
          size="sm"
          data-testid="open-in-lingq"
          onclick={() => void openUrl(lingqCollectionUrl(entry.language, collectionId))}
        >
          Open in LingQ ↗
        </Button>
      {/if}
      <a href="/library" class="text-fg-muted hover:text-fg">Back to Library</a>
    </span>
  </header>

  <CourseStats view={cached?.view ?? null} />
{/if}
