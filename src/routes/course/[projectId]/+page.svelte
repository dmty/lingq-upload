<script lang="ts">
  import { page } from "$app/state";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { library } from "$lib/stores/library.svelte";
  import { course } from "$lib/stores/course.svelte";
  import { joinKey } from "$lib/identity";
  import { lingqCollectionUrl } from "$lib/lingq";
  import CoverThumb from "$lib/components/CoverThumb.svelte";
  import CourseStats from "$lib/components/CourseStats.svelte";
  import LessonStatRow from "$lib/components/LessonStatRow.svelte";
  import Button from "$lib/components/Button.svelte";
  import Alert from "$lib/components/Alert.svelte";
  import { appErrorMessage, isLingqNotFound, isMissingApiKey } from "$lib/errors";

  const projectKey = $derived(page.params.projectId ?? "");

  const entry = $derived(
    library.index?.entries.find((e) => joinKey(e.id) === projectKey) ?? null,
  );

  const collectionId = $derived(entry?.lingq_collection_id ?? null);
  const authorLine = $derived((entry?.authors ?? []).join(" · "));

  const cached = $derived(
    entry && collectionId != null ? course.entry(entry.language, collectionId) : null,
  );

  function refresh(force = false) {
    if (entry && collectionId != null) void course.ensure(entry.language, collectionId, force);
  }

  $effect(() => {
    if (library.status === "idle") void library.load();
  });

  $effect(() => {
    refresh();
  });

  const alert = $derived.by(() => {
    const err = cached?.error;
    if (!err || cached?.view != null) return null;
    if (isMissingApiKey(err)) {
      return {
        message: "Add your LingQ API key in Settings to see course stats.",
        settings: true,
        retry: false,
      };
    }
    if (isLingqNotFound(err)) {
      return { message: "This course is no longer on LingQ.", settings: false, retry: false };
    }
    if (err.kind === "Lingq" && err.message.kind === "Transport") {
      return { message: "Couldn't reach LingQ.", settings: false, retry: true };
    }
    return { message: appErrorMessage(err), settings: false, retry: true };
  });
</script>

{#if entry == null && (library.status === "idle" || library.status === "loading")}
  <p data-testid="course-loading" class="text-sm text-fg-muted">Loading your library…</p>
{:else if entry == null && library.status === "error"}
  <p data-testid="course-library-error">
    {appErrorMessage(library.error!)}
  </p>
{:else if entry == null}
  <p data-testid="course-not-found">
    That course isn't in your library. <a href="/library">Back to Library</a>
  </p>
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

  {#if collectionId == null}
    <p data-testid="course-not-uploaded" class="mt-6 text-sm text-fg-muted">
      This course hasn't been uploaded to LingQ yet.
    </p>
  {:else if alert}
    <div class="flex items-center gap-3">
      <Alert variant="warning" data-testid="course-alert" class="flex-1">
        {alert.message}
        {#if alert.settings}
          <a href="/settings" class="ml-1 font-medium text-accent underline">Settings</a>
        {/if}
      </Alert>
      {#if alert.retry}
        <Button variant="secondary" size="sm" data-testid="course-refresh" onclick={() => refresh(true)}>
          Retry
        </Button>
      {/if}
    </div>
  {:else}
    <CourseStats
      view={cached?.view ?? null}
      fetchedAt={cached?.fetchedAt ?? null}
      revalidating={cached?.revalidating ?? false}
      refreshFailed={cached?.error != null}
      onrefresh={() => refresh(true)}
    />
  {/if}

  {#if cached?.view && cached.view.lessons.length > 0}
    <section data-testid="course-lessons" class="mt-6">
      <div
        class="grid grid-cols-[2.5rem_1fr_5rem_4rem_4rem_5rem] items-center gap-3 text-xs uppercase text-fg-subtle"
      >
        <span></span>
        <span></span>
        <span class="text-right">Words</span>
        <span class="text-right">New</span>
        <span class="text-right">Time</span>
        <span class="text-right">Progress</span>
      </div>
      {#each cached.view.lessons as lesson, i (lesson.id)}
        <LessonStatRow index={i} {lesson} />
      {/each}
    </section>
  {/if}
{/if}
