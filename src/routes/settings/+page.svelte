<script lang="ts">
  import { onMount } from "svelte";
  import { commands, type AppError, type SecretError } from "$lib/ipc/bindings";

  let key = $state("");
  let savedTail = $state<string | null>(null); // last 4 chars of stored key
  let busy = $state(false);
  let error = $state<string | null>(null);
  let justSaved = $state(false);

  function secretMessage(e: SecretError): string {
    switch (e.kind) {
      case "LockedKeychain":
        return "Your OS keychain is locked. Unlock it and try again.";
      case "UserDenied":
        return "Access to the keychain was denied. Approve the prompt and retry.";
      case "MissingEntry":
        return "No saved key was found.";
      case "Backend":
        return `Keychain error: ${e.message}`;
    }
  }

  function appErrorMessage(e: AppError): string {
    switch (e.kind) {
      case "Secrets":
        return secretMessage(e.message);
      case "Io":
        return `I/O error: ${e.message}`;
      case "Internal":
        return `Internal error: ${e.message}`;
      case "Lingq":
        return `LingQ: ${"message" in e.message ? e.message.message : e.message.kind}`;
      case "Audio":
        return `Audio: ${"message" in e.message ? e.message.message : e.message.kind}`;
      case "Text":
        return `Text: ${e.message.message}`;
      case "Ingest":
        return `Ingest: ${"message" in e.message ? e.message.message : e.message.kind}`;
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
      await refresh();
      justSaved = true;
      setTimeout(() => (justSaved = false), 600);
    } else {
      error = appErrorMessage(res.error);
    }
    busy = false;
  }

  async function clear() {
    busy = true;
    error = null;
    const res = await commands.cmdClearLingqKey();
    if (res.status === "ok") {
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
      // Browser may deny clipboard access; the input is still usable.
    }
  }

  onMount(() => {
    void refresh();
  });
</script>

<section class="mx-auto max-w-140 pt-12">
  <h1 class="text-xl font-semibold text-fg">Settings</h1>
  <p class="mt-2 text-base text-fg-muted">
    Where this app keeps your keys and preferences.
  </p>

  <div
    class="mt-8 rounded-md border border-border bg-surface p-6 shadow-(--shadow-card)"
  >
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
      <div
        role="alert"
        class="mt-4 rounded-sm border-l-[3px] border-error bg-error-soft p-3 text-sm text-error"
      >
        {error}
      </div>
    {/if}

    <div class="mt-6 flex items-center justify-end gap-2">
      <button
        type="button"
        onclick={clear}
        disabled={busy || !savedTail}
        class="rounded-sm px-3 py-2 text-sm font-medium text-fg-muted transition-colors duration-120 hover:bg-surface-sunken hover:text-fg disabled:opacity-40"
      >
        Clear
      </button>
      <button
        type="button"
        onclick={save}
        disabled={busy || key.length === 0}
        class="inline-flex h-9 items-center gap-2 rounded-sm bg-accent px-4 text-sm font-medium text-white transition-colors duration-180 ease-snappy hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-40"
      >
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
      </button>
    </div>
  </div>
</section>
