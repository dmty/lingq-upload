<script lang="ts">
  import { onMount } from "svelte";
  import LibraryList from "$lib/components/LibraryList.svelte";
  import { library } from "$lib/stores/library.svelte";
  import { libraryBanner } from "$lib/stores/library-banner.svelte";
  import { appErrorMessage } from "$lib/errors";
  import { commands, type LibraryEntry } from "$lib/ipc/bindings";

  onMount(() => {
    library.load();
    void checkLingqKey();
    const onFocus = () => void checkLingqKey();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  });

  let search = $state("");
  let languageFilter = $state("");
  let lingqKeyMissing = $state(false);

  async function checkLingqKey() {
    const r = await commands.cmdLoadLingqKey();
    if (r.status === "ok") lingqKeyMissing = r.data === null;
  }

  function nfc(s: string): string {
    return s.normalize("NFC").toLowerCase();
  }

  const entries = $derived(library.index?.entries ?? []);
  const languages = $derived(
    [...new Set(entries.map((e) => e.language))].sort(),
  );

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
      if (languageFilter && e.language !== languageFilter) return false;
      if (q) {
        const hay = `${nfc(e.title)} ${nfc((e.authors ?? []).join(" "))}`;
        if (!hay.includes(q)) return false;
      }
      return true;
    });
  });

  const totalCount = $derived(entries.length);
  const runningCount = $derived(
    entries.filter((e) => (e.status ?? "idle") === "running").length,
  );

  function clearSearch() {
    search = "";
    languageFilter = "";
  }
</script>

<section class="mx-auto max-w-3xl pt-6">
  <header class="mb-4 flex items-center justify-between">
    <div>
      <h1 class="text-lg font-semibold text-fg">Library</h1>
      {#if totalCount > 0}
        <p class="mt-0.5 flex items-center gap-1 text-xs text-fg-muted">
          <span>
            {totalCount} books{runningCount > 0
              ? ` · ${runningCount} in progress`
              : ""}
          </span>
          {#if runningCount > 0}
            <span class="inline-block animate-spin text-accent">⟳</span>
          {/if}
        </p>
      {/if}
    </div>
    <a
      href="/add"
      class="rounded-sm bg-accent px-3 py-1.5 text-sm font-medium text-white transition-colors duration-120 hover:bg-accent-hover"
    >
      + Add
    </a>
  </header>

  {#if lingqKeyMissing && !libraryBanner.dismissed}
    <div
      role="status"
      aria-live="polite"
      class="mb-3 flex items-center justify-between gap-3 rounded-sm border-l-2 border-warning bg-warning/10 px-3 py-2 text-sm text-fg"
    >
      <span> Add your LingQ API key in Settings to start uploading. </span>
      <span class="flex items-center gap-3 text-xs">
        <a href="/settings" class="font-medium text-accent hover:underline">
          Open Settings
        </a>
        <button
          type="button"
          class="text-fg-muted hover:text-fg"
          onclick={() => (libraryBanner.dismissed = true)}
        >
          Dismiss
        </button>
      </span>
    </div>
  {/if}

  {#if library.status === "loading"}
    <p class="text-sm text-fg-muted">Loading library…</p>
  {:else if library.status === "error"}
    <div
      class="rounded-sm border border-error-soft bg-error-soft/30 p-4 text-sm text-fg"
    >
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
      <button
        type="button"
        class="mt-3 rounded-sm bg-accent px-3 py-1.5 text-sm font-medium text-white"
        onclick={() => library.load()}
      >
        Retry
      </button>
    </div>
  {:else if entries.length === 0}
    <div
      class="mx-auto mt-10 max-w-sm rounded-sm border border-border bg-surface p-8 text-center"
    >
      <div class="text-3xl">📚</div>
      <p class="mt-3 text-base font-medium text-fg">Your shelf is empty.</p>
      <p class="mt-2 text-sm text-fg-muted">
        Point me at your Calibre library or a Libation folder and I'll show you
        what's there.
      </p>
      <a
        href="/add"
        class="mt-4 inline-block rounded-sm bg-accent px-3 py-1.5 text-sm font-medium text-white hover:bg-accent-hover"
      >
        + Add a book
      </a>
    </div>
  {:else}
    <div class="mb-3 flex gap-2">
      <input
        type="search"
        placeholder="Search titles or authors…"
        bind:value={search}
        class="flex-1 rounded-sm border border-border bg-surface px-3 py-1.5 text-sm text-fg placeholder:text-fg-muted"
      />
      <select
        bind:value={languageFilter}
        class="rounded-sm border border-border bg-surface px-3 py-1.5 text-sm text-fg"
      >
        <option value="">All languages</option>
        {#each languages as lang (lang)}
          <option value={lang}>{lang}</option>
        {/each}
      </select>
    </div>

    {#if filtered.length === 0}
      <div class="rounded-sm border border-border bg-surface p-4 text-sm">
        <p class="font-medium text-fg">No matches.</p>
        <p class="mt-1 text-xs text-fg-muted">
          Searched: title and author. Try a partial match or a different filter.
        </p>
        <button
          type="button"
          class="mt-2 text-xs font-medium text-accent hover:underline"
          onclick={clearSearch}
        >
          Clear search
        </button>
      </div>
    {:else}
      <LibraryList entries={filtered} />
    {/if}
  {/if}
</section>
