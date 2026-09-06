<script lang="ts">
  import { onMount } from "svelte";
  import { page } from "$app/state";
  import { navigationHistory } from "$lib/stores/navigation.svelte";
  import { libraryUrl } from "$lib/library-url";
  import LibraryList from "$lib/components/LibraryList.svelte";
  import Spinner from "$lib/components/Spinner.svelte";
  import { library } from "$lib/stores/library.svelte";
  import { course } from "$lib/stores/course.svelte";
  import { libraryBanner } from "$lib/stores/library-banner.svelte";
  import { appErrorMessage } from "$lib/errors";
  import { joinKey } from "$lib/identity";
  import { primaryActionFor } from "$lib/library-actions";
  import {
    commands,
    type LibraryEntry,
    type ProjectId,
  } from "$lib/ipc/bindings";
  import Button from "$lib/components/Button.svelte";
  import Alert from "$lib/components/Alert.svelte";
  import { openSettings } from "$lib/settings-window";

  onMount(() => {
    library.load();
    void checkLingqKey();
    const onFocus = () => void checkLingqKey();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  });

  let lingqKeyMissing = $state(false);
  let focusIndex = $state<number | null>(null);
  let confirmRequestId = $state<string | null>(null);

  async function checkLingqKey() {
    const r = await commands.cmdLoadLingqKey();
    if (r.status === "ok") lingqKeyMissing = r.data === null;
  }

  function nfc(s: string): string {
    return s.normalize("NFC").toLowerCase();
  }

  const selectedLanguage = $derived(
    page.url.searchParams.get("language") ?? "",
  );
  const urlSearch = $derived(page.url.searchParams.get("q") ?? "");

  // A real navigation, not shallow routing: $app/navigation's replaceState
  // moves the address bar without updating page.url, and the sidebar reads
  // its language and query from there. Replacing rather than pushing keeps a
  // search worth one Back instead of one per keystroke.
  function replaceLibraryUrl(language: string, query: string) {
    void navigationHistory.replace(libraryUrl(language, query));
  }

  const search = $derived(urlSearch);

  const entries = $derived(library.index?.entries ?? []);

  $effect(() => {
    for (const e of entries) {
      if (e.lingq_collection_id != null && !e.cover_path) {
        void course.ensure(e.language, e.lingq_collection_id);
      }
    }
  });

  const STATUS_ORDER: Record<NonNullable<LibraryEntry["status"]>, number> = {
    running: 0,
    needs_match: 1,
    paused: 2,
    done: 3,
    failed: 4,
    idle: 5,
  };

  function sortEntries(list: LibraryEntry[]): LibraryEntry[] {
    return [...list].sort((a, b) => {
      const sa = STATUS_ORDER[a.status ?? "idle"];
      const sb = STATUS_ORDER[b.status ?? "idle"];
      if (sa !== sb) return sa - sb;
      if ((a.status ?? "idle") === "done") {
        const ta = a.last_activity_at
          ? Date.parse(a.last_activity_at)
          : -Infinity;
        const tb = b.last_activity_at
          ? Date.parse(b.last_activity_at)
          : -Infinity;
        if (ta !== tb) return tb - ta;
      }
      return a.title.localeCompare(b.title);
    });
  }

  const sorted = $derived(sortEntries(entries));

  const filtered = $derived.by(() => {
    const q = nfc(search.trim());
    return sorted.filter((e) => {
      if (selectedLanguage && e.language !== selectedLanguage) return false;
      if (q) {
        const hay = `${nfc(e.title)} ${nfc((e.authors ?? []).join(" "))}`;
        if (!hay.includes(q)) return false;
      }
      return true;
    });
  });

  const runningCount = $derived(
    entries.filter((e) => (e.status ?? "idle") === "running").length,
  );

  // Clears just the query — the language is the sidebar's, and nothing here
  // reaches over to reset it.
  function clearSearchText() {
    replaceLibraryUrl(selectedLanguage, "");
  }

  // A focusIndex that survives re-filtering targets the wrong row.
  $effect(() => {
    urlSearch;
    selectedLanguage;
    focusIndex = null;
  });

  // A language nothing is written in — a stale link, a deletion, a refresh
  // that dropped the last book — falls back to All. Only once the library is
  // readable: while it is loading or unreadable the represented languages
  // aren't known, and the requested destination has to survive.
  $effect(() => {
    if (library.status !== "ready" || !selectedLanguage) return;
    if (entries.some((e) => e.language === selectedLanguage)) return;
    replaceLibraryUrl("", urlSearch);
  });

  function isFormField(el: EventTarget | null): boolean {
    if (!(el instanceof HTMLElement)) return false;
    const tag = el.tagName;
    return (
      tag === "INPUT" ||
      tag === "TEXTAREA" ||
      tag === "SELECT" ||
      el.isContentEditable
    );
  }

  function handleTrashed(id: ProjectId) {
    const removedKey = joinKey(id);
    const list = filtered;
    const removedIdx = list.findIndex((e) => joinKey(e.id) === removedKey);
    library.removeById(id);
    const nextLen = list.length - 1;
    if (focusIndex == null || removedIdx === -1) return;
    if (nextLen === 0) {
      focusIndex = null;
    } else if (removedIdx === focusIndex) {
      focusIndex = Math.min(removedIdx, nextLen - 1);
    } else if (removedIdx < focusIndex) {
      focusIndex = focusIndex - 1;
    }
  }

  $effect(() => {
    function onKeydown(e: KeyboardEvent) {
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      if (isFormField(e.target)) return;

      const list = filtered;
      if (list.length === 0) return;

      if (e.key === "ArrowDown") {
        e.preventDefault();
        focusIndex =
          focusIndex == null ? 0 : Math.min(focusIndex + 1, list.length - 1);
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        focusIndex = focusIndex == null ? 0 : Math.max(focusIndex - 1, 0);
        return;
      }
      if (e.key === "Enter" && focusIndex != null) {
        e.preventDefault();
        const entry = list[focusIndex];
        if (entry) primaryActionFor(entry).run();
        return;
      }
      if ((e.key === "Backspace" || e.key === "Delete") && focusIndex != null) {
        e.preventDefault();
        const entry = list[focusIndex];
        if (entry) confirmRequestId = joinKey(entry.id);
      }
    }
    window.addEventListener("keydown", onKeydown);
    return () => window.removeEventListener("keydown", onKeydown);
  });
</script>

<section class="col-wide">
  <h1 class="sr-only">Library</h1>
  {#if runningCount > 0}
    <p class="mb-3 flex items-center gap-1.5 text-xs text-fg-muted">
      <Spinner size="sm" aria-hidden="true" />
      {runningCount} uploading
    </p>
  {/if}

  {#if lingqKeyMissing && !libraryBanner.dismissed}
    <Alert
      variant="warning"
      aria-live="polite"
      class="mb-3 flex items-center justify-between gap-3 px-3 py-2"
    >
      <span> Add your LingQ API key in Settings to start uploading. </span>
      <span class="flex items-center gap-3 text-xs">
        <Button
          variant="secondary"
          size="sm"
          onclick={() => void openSettings()}
        >
          Open Settings
        </Button>
        <button
          type="button"
          class="text-fg-muted hover:text-fg"
          onclick={() => (libraryBanner.dismissed = true)}
        >
          Dismiss
        </button>
      </span>
    </Alert>
  {/if}

  {#if library.status === "loading"}
    <p class="text-sm text-fg-muted">Loading library…</p>
  {:else if library.status === "error"}
    <Alert body class="px-4 py-4">
      <p class="font-medium">Library is unreadable</p>
      <p class="mt-1 text-fg-muted">{appErrorMessage(library.error!)}</p>
      <details class="mt-2 text-xs text-fg-muted">
        <summary class="cursor-pointer">Show details</summary>
        <pre
          class="mt-2 overflow-auto rounded-sm bg-surface-sunken p-2">{JSON.stringify(
            library.error,
            null,
            2,
          )}</pre>
      </details>
      <Button class="mt-3" onclick={() => library.load()}>Retry</Button>
    </Alert>
  {:else if entries.length === 0}
    <div
      class="mx-auto mt-10 max-w-sm rounded-sm border border-border bg-surface p-8 text-center"
    >
      <div class="text-3xl">📚</div>
      <p class="mt-3 text-base font-medium text-fg">Your shelf is empty.</p>
      <p class="mt-2 text-sm text-fg-muted">
        Add a book, its audio, or both to get started.
      </p>
      <Button href="/add" class="mt-4">Add a book…</Button>
    </div>
  {:else}
    {#if filtered.length === 0}
      <div class="rounded-sm border border-border bg-surface p-4 text-sm">
        <p class="font-medium text-fg">No matches.</p>
        <p class="mt-1 text-xs text-fg-muted">
          Searched: title and author. Try a partial match or a different filter.
        </p>
        <button
          type="button"
          class="mt-2 text-xs font-medium text-accent hover:underline"
          onclick={clearSearchText}
        >
          Clear search
        </button>
      </div>
    {:else}
      <LibraryList
        entries={filtered}
        {focusIndex}
        onfocuschange={(i) => (focusIndex = i)}
        ontrash={handleTrashed}
        {confirmRequestId}
        onconfirmhandled={() => (confirmRequestId = null)}
      />
    {/if}
  {/if}
</section>
