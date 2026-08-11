<script lang="ts">
  import { onMount } from "svelte";
  import {
    commands,
    type AppTranscriptionPreferences,
    type BackendChoice,
    type DevBackendInfo,
    type ProviderInfo,
    type TranscribeProviderId,
    type TrashEntry,
  } from "$lib/ipc/bindings";
  import { appErrorMessage } from "$lib/errors";
  import { formatRelative } from "$lib/format";
  import {
    clearSavedLanguage,
    languagesStore,
  } from "$lib/stores/languages.svelte";
  import Button from "$lib/components/Button.svelte";
  import Alert from "$lib/components/Alert.svelte";

  let key = $state("");
  let savedTail = $state<string | null>(null); // last 4 chars of stored key
  let busy = $state(false);
  let error = $state<string | null>(null);
  let justSaved = $state(false);

  let trashEntries = $state<TrashEntry[]>([]);
  let trashError = $state<string | null>(null);
  let trashLoaded = $state(false);
  let purgeConfirmId = $state<string | null>(null);
  let purgeTimer: ReturnType<typeof setTimeout> | null = null;
  let rowBusyId = $state<string | null>(null);
  let rowErrors = $state<Record<string, string>>({});

  let clearArmed = $state(false);
  let clearTimer: ReturnType<typeof setTimeout> | null = null;

  let devBackend = $state<DevBackendInfo | null>(null);
  let devBackendBusy = $state(false);
  let devBackendError = $state<string | null>(null);

  let transcribeProviders = $state<ProviderInfo[]>([]);
  let transcriptionPreferences = $state<AppTranscriptionPreferences>({
    provider_id: "groq",
    auto_detect_start: false,
  });
  let transcriptionKeys = $state<Record<TranscribeProviderId, string>>({
    groq: "",
    open_ai: "",
  });
  let transcriptionLoaded = $state(false);
  let transcriptionBusy = $state(false);
  let transcriptionKeyBusy = $state<TranscribeProviderId | null>(null);
  let transcriptionError = $state<string | null>(null);
  let transcriptionClearArmed = $state<TranscribeProviderId | null>(null);
  let transcriptionClearTimer: ReturnType<typeof setTimeout> | null = null;

  async function loadTranscriptionSettings() {
    const [providersResult, preferencesResult] = await Promise.all([
      commands.cmdListTranscribeProviders(),
      commands.cmdGetTranscriptionPreferences(),
    ]);
    if (providersResult.status === "ok") {
      transcribeProviders = providersResult.data;
    } else {
      transcriptionError = appErrorMessage(providersResult.error);
    }
    if (preferencesResult.status === "ok") {
      transcriptionPreferences = preferencesResult.data;
    } else if (!transcriptionError) {
      transcriptionError = appErrorMessage(preferencesResult.error);
    }
    transcriptionLoaded = true;
  }

  async function saveTranscriptionPreferences(
    next: AppTranscriptionPreferences,
  ) {
    if (transcriptionBusy) return;
    const previous = { ...transcriptionPreferences };
    transcriptionPreferences = next;
    transcriptionBusy = true;
    transcriptionError = null;
    const result = await commands.cmdSetTranscriptionPreferences(next);
    if (result.status === "error") {
      transcriptionPreferences = previous;
      transcriptionError = appErrorMessage(result.error);
    }
    transcriptionBusy = false;
  }

  function setTranscriptionProvider(provider_id: TranscribeProviderId) {
    cancelTranscriptionClear();
    return saveTranscriptionPreferences({
      ...transcriptionPreferences,
      provider_id,
    });
  }

  function setAutoDetect(auto_detect_start: boolean) {
    return saveTranscriptionPreferences({
      ...transcriptionPreferences,
      auto_detect_start,
    });
  }

  async function refreshTranscriptionKey(provider: TranscribeProviderId) {
    const result = await commands.cmdTranscribeKeyPresent(provider);
    if (result.status === "ok") {
      transcribeProviders = transcribeProviders.map((item) =>
        item.id === provider ? { ...item, key_present: result.data } : item,
      );
    } else {
      transcriptionError = appErrorMessage(result.error);
    }
  }

  async function saveTranscriptionKey(provider: TranscribeProviderId) {
    const value = transcriptionKeys[provider].trim();
    if (!value) {
      transcriptionError = "Enter a key first.";
      return;
    }
    transcriptionKeyBusy = provider;
    transcriptionError = null;
    const result = await commands.cmdSaveTranscribeKey(provider, value);
    if (result.status === "ok") {
      transcriptionKeys[provider] = "";
      await refreshTranscriptionKey(provider);
    } else {
      transcriptionError = appErrorMessage(result.error);
    }
    transcriptionKeyBusy = null;
  }

  function cancelTranscriptionClear() {
    if (transcriptionClearTimer != null) {
      clearTimeout(transcriptionClearTimer);
    }
    transcriptionClearTimer = null;
    transcriptionClearArmed = null;
  }

  function requestTranscriptionClear(provider: TranscribeProviderId) {
    if (transcriptionClearArmed === provider) {
      cancelTranscriptionClear();
      void clearTranscriptionKey(provider);
      return;
    }
    cancelTranscriptionClear();
    transcriptionClearArmed = provider;
    transcriptionClearTimer = setTimeout(cancelTranscriptionClear, 5000);
  }

  async function clearTranscriptionKey(provider: TranscribeProviderId) {
    transcriptionKeyBusy = provider;
    transcriptionError = null;
    const result = await commands.cmdClearTranscribeKey(provider);
    if (result.status === "ok") {
      transcriptionKeys[provider] = "";
      await refreshTranscriptionKey(provider);
    } else {
      transcriptionError = appErrorMessage(result.error);
    }
    transcriptionKeyBusy = null;
  }

  async function loadDevBackend() {
    const res = await commands.cmdGetDevBackend();
    if (res.status === "ok") {
      devBackend = res.data;
      devBackendError = null;
    } else {
      devBackendError = appErrorMessage(res.error);
    }
  }

  async function setDevBackend(choice: BackendChoice) {
    if (devBackendBusy) return;
    devBackendBusy = true;
    devBackendError = null;
    const res = await commands.cmdSetDevBackend(choice);
    if (res.status === "ok") {
      await loadDevBackend();
    } else {
      devBackendError = appErrorMessage(res.error);
    }
    devBackendBusy = false;
  }

  async function loadTrash() {
    const res = await commands.cmdListTrash();
    if (res.status === "ok") {
      trashEntries = res.data;
      trashError = null;
    } else {
      trashError = appErrorMessage(res.error);
    }
    trashLoaded = true;
  }

  function setRowError(id: string, msg: string | null) {
    if (msg == null) {
      const { [id]: _drop, ...rest } = rowErrors;
      rowErrors = rest;
    } else {
      rowErrors = { ...rowErrors, [id]: msg };
    }
  }

  async function restore(entry: TrashEntry) {
    if (rowBusyId) return;
    rowBusyId = entry.trash_id;
    setRowError(entry.trash_id, null);
    const res = await commands.cmdRestoreProject(entry.trash_id);
    rowBusyId = null;
    if (res.status === "ok") {
      trashEntries = trashEntries.filter((t) => t.trash_id !== entry.trash_id);
    } else {
      setRowError(entry.trash_id, appErrorMessage(res.error));
    }
  }

  function startPurgeConfirm(entry: TrashEntry) {
    if (purgeTimer != null) clearTimeout(purgeTimer);
    purgeConfirmId = entry.trash_id;
    setRowError(entry.trash_id, null);
    purgeTimer = setTimeout(() => {
      purgeConfirmId = null;
      purgeTimer = null;
    }, 5000);
  }

  function cancelPurge() {
    if (purgeTimer != null) clearTimeout(purgeTimer);
    purgeTimer = null;
    purgeConfirmId = null;
  }

  async function purge(entry: TrashEntry) {
    if (rowBusyId) return;
    rowBusyId = entry.trash_id;
    setRowError(entry.trash_id, null);
    const res = await commands.cmdPurgeProject(entry.trash_id);
    rowBusyId = null;
    if (res.status === "ok") {
      cancelPurge();
      trashEntries = trashEntries.filter((t) => t.trash_id !== entry.trash_id);
    } else {
      setRowError(entry.trash_id, appErrorMessage(res.error));
    }
  }

  async function refresh() {
    error = null;
    const res = await commands.cmdLoadLingqKey();
    if (res.status === "ok") {
      savedTail = res.data ? res.data.slice(-4) : null;
    } else {
      savedTail = null;
      error = appErrorMessage(res.error);
    }
  }

  async function save() {
    if (!key.trim()) {
      error = "Enter a key first.";
      return;
    }
    busy = true;
    error = null;
    const res = await commands.cmdSaveLingqKey(key);
    if (res.status === "ok") {
      key = "";
      languagesStore.invalidate();
      clearSavedLanguage();
      void languagesStore.ensureLoaded();
      await refresh();
      justSaved = true;
      setTimeout(() => (justSaved = false), 2500);
    } else {
      error = appErrorMessage(res.error);
    }
    busy = false;
  }

  function requestClear() {
    if (!clearArmed) {
      clearArmed = true;
      clearTimer = setTimeout(() => (clearArmed = false), 5000);
      return;
    }
    if (clearTimer != null) clearTimeout(clearTimer);
    clearTimer = null;
    clearArmed = false;
    void clear();
  }

  async function clear() {
    busy = true;
    error = null;
    const res = await commands.cmdClearLingqKey();
    if (res.status === "ok") {
      languagesStore.invalidate();
      clearSavedLanguage();
      await refresh();
    } else {
      error = appErrorMessage(res.error);
    }
    busy = false;
  }

  async function pasteFromClipboard() {
    try {
      const text = await navigator.clipboard.readText();
      if (text) key = text.trim();
    } catch {
      error = "Clipboard unavailable — paste the key manually.";
    }
  }

  onMount(() => {
    void refresh();
    void loadTrash();
    void loadDevBackend();
    void loadTranscriptionSettings();
  });
</script>

<section class="col-form pt-12">
  <h1 class="text-xl font-semibold text-fg">Settings</h1>
  <p class="mt-2 text-base text-fg-muted">
    Where this app keeps your keys and preferences.
  </p>

  <div class="mt-8 rounded-md border border-border bg-surface p-6 shadow-card">
    <h2 class="text-md font-semibold text-fg">LingQ API key</h2>
    <p class="mt-1 text-sm text-fg-subtle">
      Find it at <a
        href="https://www.lingq.com/accounts/apikey/"
        target="_blank"
        rel="noopener noreferrer">lingq.com/accounts/apikey/</a
      > — we store it in your OS keychain.
    </p>

    <label class="mt-4 block">
      <span
        class="block text-xs font-medium tracking-[0.04em] text-fg-muted uppercase"
      >
        Key
      </span>
      <div class="relative mt-1.5">
        <input
          type="password"
          autocomplete="off"
          spellcheck="false"
          bind:value={key}
          disabled={busy}
          placeholder="Paste your LingQ API key"
          class="h-10 w-full rounded-sm border border-border bg-surface px-3 pr-16 text-base text-fg outline-none transition-[box-shadow,border-color] duration-180 ease-snappy placeholder:text-fg-subtle focus:border-accent focus:shadow-[0_0_0_3px_var(--color-accent-soft)]"
        />
        <button
          type="button"
          onclick={pasteFromClipboard}
          disabled={busy}
          class="absolute top-1/2 right-1.5 -translate-y-1/2 rounded-sm px-2.5 py-1 text-xs font-medium text-fg-muted transition-colors duration-120 hover:bg-surface-sunken hover:text-fg disabled:opacity-50"
        >
          Paste
        </button>
      </div>
    </label>

    <div class="mt-4 flex items-center gap-2 text-sm">
      <span
        class="inline-block h-2 w-2 rounded-full transition-colors duration-180 ease-snappy"
        class:bg-fg-subtle={!savedTail}
        class:bg-success={!!savedTail}
      ></span>
      {#if savedTail}
        <span class="text-fg-muted">
          Key saved · <code class="tabular text-fg">•••• {savedTail}</code>
        </span>
      {:else}
        <span class="text-fg-subtle">No key set yet.</span>
      {/if}
    </div>

    {#if error}
      <Alert class="mt-4">{error}</Alert>
    {/if}

    <div class="mt-6 flex items-center justify-end gap-2">
      <button
        type="button"
        onclick={requestClear}
        disabled={busy || !savedTail}
        class="rounded-sm px-3 py-2 text-sm font-medium transition-colors duration-120 hover:bg-surface-sunken disabled:opacity-40 {clearArmed
          ? 'text-error'
          : 'text-fg-muted hover:text-fg'}"
      >
        {clearArmed ? "Really clear?" : "Clear"}
      </button>
      <Button disabled={busy || key.length === 0} onclick={save}>
        {#if justSaved}
          <svg
            width="14"
            height="14"
            viewBox="0 0 20 20"
            fill="none"
            stroke="currentColor"
            stroke-width="2.5"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <path d="M4 10.5l4 4 8-9" />
          </svg>
          Saved
        {:else}
          {busy ? "Saving…" : "Save"}
        {/if}
      </Button>
    </div>
  </div>

  <section
    class="mt-8 rounded-md border border-border bg-surface p-6 shadow-card"
    aria-labelledby="transcription-heading"
  >
    <h2 id="transcription-heading" class="text-md font-semibold text-fg">
      Transcription (optional)
    </h2>
    <p class="mt-1 text-sm text-fg-subtle">
      Choose the app-wide provider used for transcription-assisted detection.
      Keys stay in your OS keychain.
    </p>

    {#if !transcriptionLoaded}
      <p class="mt-4 text-sm text-fg-muted">Loading providers…</p>
    {:else}
      <fieldset class="mt-5 space-y-3" disabled={transcriptionBusy}>
        <legend class="sr-only">Transcription provider</legend>
        {#each transcribeProviders as provider (provider.id)}
          <div
            role="group"
            aria-label={`${provider.label} provider`}
            class="rounded-sm border p-4 transition-colors duration-120 {transcriptionPreferences.provider_id ===
            provider.id
              ? 'border-accent bg-accent-soft/30'
              : 'border-border'}"
          >
            <div class="flex items-start justify-between gap-4">
              <div class="flex items-start gap-3">
                <input
                  id={`transcription-provider-${provider.id}`}
                  type="radio"
                  name="transcription-provider"
                  value={provider.id}
                  aria-label={provider.label}
                  checked={transcriptionPreferences.provider_id === provider.id}
                  onchange={() => setTranscriptionProvider(provider.id)}
                  class="mt-1 h-4 w-4 accent-accent"
                />
                <label
                  for={`transcription-provider-${provider.id}`}
                  class="cursor-pointer"
                >
                  <span class="block text-sm font-semibold text-fg">
                    {provider.label}
                  </span>
                  <code class="tabular text-xs text-fg-muted">
                    {provider.model}
                  </code>
                </label>
              </div>
              <span
                class={provider.key_present
                  ? "text-xs font-medium text-success"
                  : "text-xs text-fg-subtle"}
              >
                {provider.key_present ? "Key saved" : "No key saved"}
              </span>
            </div>

            <p class="mt-3 text-sm text-fg-muted">
              {provider.pricing_hint.summary}
            </p>
            <p class="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-xs">
              <a
                href={provider.pricing_hint.docs_url}
                target="_blank"
                rel="noopener noreferrer"
                aria-label={`${provider.label} pricing documentation`}
                >Pricing documentation</a
              >
              <a
                href={provider.data_policy_url}
                target="_blank"
                rel="noopener noreferrer"
                aria-label={`${provider.label} data policy`}>Data policy</a
              >
            </p>

            {#if transcriptionPreferences.provider_id === provider.id}
              <label class="mt-4 block">
                <span
                  class="block text-xs font-medium tracking-[0.04em] text-fg-muted uppercase"
                >
                  API key
                </span>
                <input
                  type="password"
                  autocomplete="off"
                  spellcheck="false"
                  bind:value={transcriptionKeys[provider.id]}
                  disabled={transcriptionKeyBusy !== null}
                  placeholder={`Paste your ${provider.label} API key`}
                  class="mt-1.5 h-10 w-full rounded-sm border border-border bg-surface px-3 text-base text-fg outline-none transition-[box-shadow,border-color] duration-180 ease-snappy placeholder:text-fg-subtle focus:border-accent focus:shadow-[0_0_0_3px_var(--color-accent-soft)]"
                />
              </label>

              <div class="mt-4 flex items-center justify-end gap-2">
                <button
                  type="button"
                  aria-label={transcriptionClearArmed === provider.id
                    ? `Confirm removing ${provider.label} key`
                    : `Remove ${provider.label} key`}
                  onclick={() => requestTranscriptionClear(provider.id)}
                  disabled={transcriptionKeyBusy !== null ||
                    !provider.key_present}
                  class="rounded-sm px-3 py-2 text-sm font-medium transition-colors duration-120 hover:bg-surface-sunken disabled:opacity-40 {transcriptionClearArmed ===
                  provider.id
                    ? 'text-error'
                    : 'text-fg-muted hover:text-fg'}"
                >
                  {transcriptionClearArmed === provider.id
                    ? "Really clear?"
                    : "Clear"}
                </button>
                <Button
                  aria-label={`Save ${provider.label} key`}
                  disabled={transcriptionKeyBusy !== null ||
                    transcriptionKeys[provider.id].length === 0}
                  onclick={() => saveTranscriptionKey(provider.id)}
                >
                  {transcriptionKeyBusy === provider.id
                    ? "Saving…"
                    : "Save key"}
                </Button>
              </div>
            {/if}
          </div>
        {/each}
      </fieldset>

      <label class="mt-5 flex items-start gap-3 text-sm">
        <input
          type="checkbox"
          checked={transcriptionPreferences.auto_detect_start}
          disabled={transcriptionBusy}
          onchange={(event) => setAutoDetect(event.currentTarget.checked)}
          class="mt-0.5 h-4 w-4 accent-accent"
        />
        <span>
          <span class="font-medium text-fg"
            >Automatically detect text start</span
          >
          <span class="block text-fg-muted">
            Offer transcription-assisted start detection when a mismatch is
            eligible.
          </span>
        </span>
      </label>
    {/if}

    {#if transcriptionError}
      <Alert class="mt-4">{transcriptionError}</Alert>
    {/if}
  </section>

  <details
    class="mt-8 rounded-md border border-border bg-surface p-6 shadow-card"
    open={trashEntries.length > 0}
  >
    <summary class="cursor-pointer text-md font-semibold text-fg">
      Trash
      {#if trashLoaded && trashEntries.length > 0}
        <span class="ml-2 text-xs font-normal text-fg-muted">
          ({trashEntries.length})
        </span>
      {/if}
    </summary>
    <p class="mt-2 text-sm text-fg-muted">
      Projects you've moved to trash. Restore them or delete permanently.
    </p>

    {#if trashError}
      <Alert class="mt-3">{trashError}</Alert>
    {/if}

    {#if !trashLoaded}
      <p class="mt-4 text-sm text-fg-subtle">Loading trash…</p>
    {:else if trashEntries.length === 0}
      <p class="mt-4 text-sm text-fg-muted">Trash is empty.</p>
    {:else}
      <ul class="mt-4 divide-y divide-border">
        {#each trashEntries as entry (entry.trash_id)}
          <li class="py-3">
            {#if purgeConfirmId === entry.trash_id}
              <div
                role="alertdialog"
                aria-live="assertive"
                class="flex items-center justify-between gap-3"
              >
                <span class="text-sm text-fg-muted">
                  Permanently delete
                  <span class="font-medium text-fg">"{entry.title}"</span>?
                </span>
                <div class="flex items-center gap-2">
                  <button
                    type="button"
                    onclick={cancelPurge}
                    class="rounded-sm px-3 py-1.5 text-sm font-medium text-fg-muted hover:bg-surface-sunken hover:text-fg"
                  >
                    Cancel
                  </button>
                  <Button
                    variant="danger"
                    onclick={() => purge(entry)}
                    disabled={rowBusyId === entry.trash_id}
                  >
                    Delete permanently
                  </Button>
                </div>
              </div>
            {:else}
              <div class="flex items-center justify-between gap-3">
                <div class="min-w-0 flex-1">
                  <p class="truncate text-sm font-medium text-fg">
                    {entry.title}
                  </p>
                  <p class="text-xs text-fg-muted">
                    {entry.language} · trashed {formatRelative(
                      entry.trashed_at,
                    )}
                  </p>
                </div>
                <div class="flex items-center gap-2">
                  <button
                    type="button"
                    onclick={() => restore(entry)}
                    disabled={rowBusyId === entry.trash_id}
                    class="rounded-sm px-3 py-1.5 text-sm font-medium text-fg-muted hover:bg-surface-sunken hover:text-fg disabled:opacity-50"
                  >
                    Restore
                  </button>
                  <button
                    type="button"
                    onclick={() => startPurgeConfirm(entry)}
                    disabled={rowBusyId === entry.trash_id}
                    class="rounded-sm px-3 py-1.5 text-sm font-medium text-error hover:bg-error-soft disabled:opacity-50"
                  >
                    Delete permanently
                  </button>
                </div>
              </div>
            {/if}
            {#if rowErrors[entry.trash_id]}
              <p class="mt-2 text-xs text-error">
                {rowErrors[entry.trash_id]}
              </p>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </details>

  {#if devBackend?.is_debug}
    <details
      class="mt-8 rounded-md border border-border bg-surface p-6 shadow-card"
    >
      <summary class="cursor-pointer text-md font-semibold text-fg">
        Developer
      </summary>
      <p class="mt-2 text-sm text-fg-muted">
        Dev-only options. Hidden in release builds.
      </p>

      <div class="mt-5">
        <h3 class="text-sm font-semibold text-fg">Secrets backend</h3>
        <p class="mt-1 text-sm text-fg-subtle">
          The OS keychain prompts for your login password whenever the dev
          binary's signature changes — every <code class="tabular"
            >cargo tauri dev</code
          >
          rebuild. The file shim avoids that by storing dev keys in a
          <code class="tabular">0600</code> JSON file under app-data.
        </p>

        <fieldset
          class="mt-4 space-y-2"
          disabled={devBackendBusy || devBackend.env_override}
        >
          <label class="flex items-start gap-3">
            <input
              type="radio"
              name="dev-backend"
              value="file"
              checked={devBackend.current === "file"}
              onchange={() => setDevBackend("file")}
              class="mt-0.5 h-4 w-4 accent-accent"
            />
            <span class="text-sm">
              <span class="font-medium text-fg">File shim</span>
              <span class="block text-fg-muted">
                No keychain prompt across rebuilds. Recommended for dev.
              </span>
            </span>
          </label>

          <label class="flex items-start gap-3">
            <input
              type="radio"
              name="dev-backend"
              value="keychain"
              checked={devBackend.current === "keychain"}
              onchange={() => setDevBackend("keychain")}
              class="mt-0.5 h-4 w-4 accent-accent"
            />
            <span class="text-sm">
              <span class="font-medium text-fg">OS keychain</span>
              <span class="block text-fg-muted">
                Exercises the production code path; will prompt for your
                password after each rebuild.
              </span>
            </span>
          </label>
        </fieldset>

        {#if devBackend.env_override}
          <p class="mt-3 text-xs text-fg-muted">
            Forced to OS keychain by <code class="tabular"
              >LINGQ_USE_REAL_KEYCHAIN</code
            > env var. Unset it to use this toggle.
          </p>
        {/if}

        {#if devBackendError}
          <Alert class="mt-3">{devBackendError}</Alert>
        {/if}

        <p class="mt-4 text-xs text-fg-subtle">
          The previously-saved API key lives in whichever backend wrote it —
          switch and re-save to migrate.
        </p>
      </div>
    </details>
  {/if}
</section>
