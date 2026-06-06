<script lang="ts">
  import { onMount } from "svelte";
  import LibraryList from "$lib/components/LibraryList.svelte";
  import { library } from "$lib/stores/library.svelte";
  import { appErrorMessage } from "$lib/errors";

  onMount(() => {
    library.load();
  });

  let search = $state("");
  let languageFilter = $state("");

  function nfc(s: string): string {
    return s.normalize("NFC").toLowerCase();
  }

  const entries = $derived(library.index?.entries ?? []);
  const languages = $derived(
    [...new Set(entries.map((e) => e.language))].sort(),
  );
  const filtered = $derived.by(() => {
    const q = nfc(search.trim());
    return entries.filter((e) => {
      if (languageFilter && e.language !== languageFilter) return false;
      if (q && !nfc(e.title).includes(q)) return false;
      return true;
    });
  });
</script>

<section class="mx-auto max-w-3xl pt-6">
  <header class="mb-4 flex items-center justify-between">
    <h1 class="text-lg font-semibold text-fg">Library</h1>
    <a
      href="/add"
      class="rounded-sm bg-accent px-3 py-1.5 text-sm font-medium text-white hover:bg-accent-hover transition-colors duration-120"
    >
      + Add
    </a>
  </header>

  {#if library.status === "loading"}
    <p class="text-sm text-fg-muted">Loading library…</p>
  {:else if library.status === "error"}
    <div
      class="rounded-sm border border-error-soft bg-error-soft/30 p-4 text-sm text-fg"
    >
      <p class="font-medium">Library is unreadable</p>
      <p class="mt-1 text-fg-muted">{appErrorMessage(library.error!)}</p>
      <button
        type="button"
        class="mt-3 rounded-sm bg-accent px-3 py-1.5 text-sm font-medium text-white"
        onclick={() => library.load()}
      >
        Retry
      </button>
    </div>
  {:else if entries.length === 0}
    <p
      class="rounded-sm border border-border bg-surface p-6 text-sm text-fg-muted"
    >
      Connect a Calibre library or a Libation folder in
      <a href="/add" class="text-accent hover:underline">Add Project</a>.
    </p>
  {:else}
    <div class="mb-3 flex gap-2">
      <input
        type="search"
        placeholder="Search titles…"
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
      <p class="text-sm text-fg-muted">No matches.</p>
    {:else}
      <LibraryList entries={filtered} />
    {/if}
  {/if}
</section>
