<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { commands, type Candidate } from "$lib/ipc/bindings";
  import { appErrorMessage } from "$lib/errors";

  type Source = "calibre" | "libation";

  let {
    source,
    selectedCandidate = $bindable<Candidate | null>(null),
  }: { source: Source; selectedCandidate?: Candidate | null } = $props();

  let rootPath = $state("");
  let candidates = $state<Candidate[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let search = $state("");

  const SOURCE_HINT: Record<Source, string> = {
    calibre:
      "Calibre layout: <root>/<Author>/<Book Title (id)>/metadata.opf plus an .epub.",
    libation:
      "Libation layout: <root>/<Author>/<Book Title [ASIN]>/ with an audio file.",
  };

  // Switching source must reset everything in here — the parent only clears
  // its own pickedCandidate/conflict; stale rootPath/candidates from the
  // previous source would otherwise persist.
  $effect(() => {
    source;
    rootPath = "";
    candidates = [];
    search = "";
    error = null;
    loading = false;
  });

  const normalisedTitles = $derived(
    candidates.map((c) => c.title.normalize("NFC").toLowerCase()),
  );

  const filtered = $derived.by(() => {
    const q = search.normalize("NFC").toLowerCase().trim();
    if (!q) return candidates;
    return candidates.filter((_, i) => normalisedTitles[i].includes(q));
  });

  async function chooseFolder() {
    const sel = await open({ directory: true, multiple: false });
    if (typeof sel !== "string") return;
    rootPath = sel;
    await runScan();
  }

  async function runScan() {
    if (!rootPath) return;
    loading = true;
    error = null;
    selectedCandidate = null;
    candidates = [];
    const res = await commands.cmdIngestScan(source, rootPath);
    loading = false;
    if (res.status === "error") {
      error = appErrorMessage(res.error);
      return;
    }
    candidates = res.data;
  }

  function select(c: Candidate) {
    selectedCandidate = c;
  }

  function clearSelection() {
    selectedCandidate = null;
  }
</script>

<div class="space-y-3">
  <div class="flex items-center gap-2">
    <button
      type="button"
      onclick={chooseFolder}
      disabled={loading}
      class="rounded-sm bg-accent px-3 py-1.5 text-sm font-medium text-white hover:bg-accent-hover disabled:bg-fg-subtle"
    >
      Choose folder
    </button>
    {#if rootPath}
      <span class="truncate text-xs text-fg-muted" title={rootPath}>
        {rootPath}
      </span>
    {:else}
      <span class="text-xs text-fg-subtle">
        Pick the {source === "calibre" ? "Calibre" : "Libation"} library root.
      </span>
    {/if}
  </div>

  {#if loading}
    <div
      class="flex items-center gap-2 rounded-sm border border-border bg-surface-sunken p-3 text-sm text-fg-muted"
    >
      <span
        class="inline-block h-3 w-3 animate-spin rounded-full border-2 border-accent border-t-transparent"
        aria-hidden="true"
      ></span>
      Scanning {rootPath}…
    </div>
  {/if}

  {#if error}
    <p
      class="rounded-sm border border-error-soft bg-error-soft/30 px-3 py-2 text-sm text-fg"
    >
      {error}
    </p>
  {/if}

  {#if !loading && !error && rootPath && candidates.length === 0}
    <div
      class="space-y-1 rounded-sm border border-border bg-surface-sunken p-3 text-sm text-fg-muted"
    >
      <p>No books found under {rootPath}. Check the folder layout.</p>
      <p class="text-xs text-fg-subtle">{SOURCE_HINT[source]}</p>
    </div>
  {/if}

  {#if candidates.length > 0}
    <div class="space-y-2">
      <input
        type="search"
        bind:value={search}
        placeholder="Filter by title…"
        class="w-full rounded-sm border border-border bg-surface px-3 py-1.5 text-sm"
      />

      {#if selectedCandidate}
        <div class="flex items-center justify-between">
          <span class="text-xs text-fg-muted">
            Selected: {selectedCandidate.title}
          </span>
          <button
            type="button"
            onclick={clearSelection}
            class="rounded-sm px-2 py-1 text-xs text-fg-muted hover:bg-surface-sunken hover:text-fg"
          >
            Clear selection
          </button>
        </div>
      {/if}

      <ul
        class="max-h-72 divide-y divide-border overflow-y-auto rounded-sm border border-border bg-surface"
      >
        {#each filtered as c (JSON.stringify([c.text_source, c.audio_source]))}
          {@const isSelected = selectedCandidate === c}
          <li>
            <button
              type="button"
              onclick={() => select(c)}
              class="flex w-full flex-col items-start gap-0.5 px-3 py-2 text-left transition-colors duration-120 {isSelected
                ? 'bg-accent-soft'
                : 'hover:bg-surface-sunken'}"
            >
              <span class="text-sm font-medium text-fg">{c.title}</span>
              <span class="text-xs text-fg-muted">
                {c.authors.length ? c.authors.join(", ") : "Unknown author"}
                {#if c.language}
                  — {c.language}
                {/if}
              </span>
            </button>
          </li>
        {/each}
        {#if filtered.length === 0}
          <li class="px-3 py-2 text-xs text-fg-subtle">No matches.</li>
        {/if}
      </ul>
    </div>
  {/if}
</div>
