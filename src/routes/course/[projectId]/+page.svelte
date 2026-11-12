<script lang="ts">
  import { page } from "$app/state";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { library } from "$lib/stores/library.svelte";
  import { course } from "$lib/stores/course.svelte";
  import { joinKey } from "$lib/identity";
  import { lingqCollectionUrl } from "$lib/lingq";
  import { toolbarTitle } from "$lib/stores/toolbar.svelte";
  import CoverThumb from "$lib/components/CoverThumb.svelte";
  import CourseStats from "$lib/components/CourseStats.svelte";
  import LessonStatRow from "$lib/components/LessonStatRow.svelte";
  import Button from "$lib/components/Button.svelte";
  import ExternalLinkIcon from "$lib/components/ExternalLinkIcon.svelte";
  import Alert from "$lib/components/Alert.svelte";
  import {
    appErrorMessage,
    isLingqNotFound,
    isMissingApiKey,
  } from "$lib/errors";
  import { openSettings } from "$lib/settings-window";

  const projectKey = $derived(page.params.projectId ?? "");

  const entry = $derived(
    library.index?.entries.find((e) => joinKey(e.id) === projectKey) ?? null,
  );

  const collectionId = $derived(entry?.lingq_collection_id ?? null);
  const authorLine = $derived((entry?.authors ?? []).join(" · "));

  const languageDisplay = new Intl.DisplayNames(["en"], { type: "language" });
  const languageName = (code: string) => {
    try {
      return languageDisplay.of(code) ?? code;
    } catch {
      return code;
    }
  };

  const cached = $derived(
    entry && collectionId != null
      ? course.entry(entry.language, collectionId)
      : null,
  );

  // A column that is zero on every row carries no information; LingQ reports
  // no new words once a course has been read through.
  const showNewWords = $derived(
    (cached?.view?.lessons ?? []).some((l) => (l.new_words_count ?? 0) > 0),
  );

  function refresh(force = false) {
    if (entry && collectionId != null)
      void course.ensure(entry.language, collectionId, force);
  }

  $effect(() => {
    if (library.status === "idle") void library.load();
  });

  $effect(() => {
    refresh();
  });

  const toolbarLabel = $derived(
    entry == null
      ? library.status === "idle" || library.status === "loading"
        ? "Loading…"
        : "Course unavailable"
      : entry.title,
  );

  $effect(() => {
    toolbarTitle.set(page.url.pathname, toolbarLabel);
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
      return {
        message: "This course is no longer on LingQ.",
        settings: false,
        retry: false,
      };
    }
    if (err.kind === "Lingq" && err.message.kind === "Transport") {
      return { message: "Couldn't reach LingQ.", settings: false, retry: true };
    }
    return { message: appErrorMessage(err), settings: false, retry: true };
  });
</script>

{#if entry == null && (library.status === "idle" || library.status === "loading")}
  <p data-testid="course-loading" class="text-sm text-fg-muted">
    Loading your library…
  </p>
{:else if entry == null && library.status === "error"}
  <p data-testid="course-library-error">
    {appErrorMessage(library.error!)} <a href="/library">Back to Library</a>
  </p>
{:else if entry == null}
  <p data-testid="course-not-found">
    That course isn't in your library. <a href="/library">Back to Library</a>
  </p>
{:else}
  <h1 class="sr-only">{entry.title}</h1>
  <header data-testid="course-header" class="flex items-center gap-4">
    <CoverThumb
      coverPath={entry.cover_path ?? null}
      imageUrl={cached?.view?.collection.image_url ?? null}
      title={entry.title}
    />
    <div data-testid="course-identity" class="flex-1">
      {#if authorLine}<p class="text-fg-muted">{authorLine}</p>{/if}
      <p class="text-xs text-fg-subtle">
        {languageName(entry.language)}{cached?.view?.collection.level
          ? ` · ${cached.view.collection.level}`
          : ""}{collectionId != null ? ` · LingQ course ${collectionId}` : ""}
      </p>
    </div>
    {#if collectionId != null}
      <Button
        variant="secondary"
        data-testid="open-in-lingq"
        onclick={() =>
          void openUrl(lingqCollectionUrl(entry.language, collectionId))}
      >
        <ExternalLinkIcon />
        Open in LingQ
      </Button>
    {/if}
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
          <Button
            variant="secondary"
            size="sm"
            class="ml-2"
            onclick={() => void openSettings()}
          >
            Settings
          </Button>
        {/if}
      </Alert>
      {#if alert.retry}
        <Button
          variant="secondary"
          size="sm"
          data-testid="course-refresh"
          onclick={() => refresh(true)}
        >
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
        class="grid items-center gap-3 pb-1 text-xs text-fg-subtle"
        style="grid-template-columns: 2.5rem 1fr 5rem {showNewWords
          ? '4rem '
          : ''}4rem 5rem 1.5rem"
      >
        <span></span>
        <span></span>
        <span class="text-right">Words</span>
        {#if showNewWords}<span class="text-right">New</span>{/if}
        <span class="text-right">Time</span>
        <span class="text-right">Progress</span>
        <span></span>
      </div>
      {#each cached.view.lessons as lesson, i (lesson.id)}
        <LessonStatRow
          index={i}
          {lesson}
          language={entry.language}
          {showNewWords}
        />
      {/each}
    </section>
  {/if}
{/if}
