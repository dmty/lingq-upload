<script lang="ts">
  import { onMount } from "svelte";
  import { commands, type AppError, type SecretError } from "$lib/ipc/bindings";

  let key = $state("");
  let status = $state<string>("Loading…");
  let error = $state<string | null>(null);
  let busy = $state(false);

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
    }
  }

  async function refresh() {
    error = null;
    const res = await commands.cmdLoadLingqKey();
    if (res.status === "ok") {
      status = res.data ? "Key saved (•••)" : "No key set";
    } else {
      status = "Could not load key";
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
      status = "Saved";
      key = "";
      await refresh();
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
      status = "Cleared";
      await refresh();
    } else {
      error = appErrorMessage(res.error);
    }
    busy = false;
  }

  onMount(() => {
    void refresh();
  });
</script>

<section>
  <h1>Settings</h1>

  <h2>LingQ API key</h2>
  <p class="status">{status}</p>

  <label>
    <span>API key</span>
    <input
      type="password"
      autocomplete="off"
      spellcheck="false"
      bind:value={key}
      disabled={busy}
      placeholder="Paste your LingQ API key"
    />
  </label>

  <div class="actions">
    <button onclick={save} disabled={busy || key.length === 0}>
      {busy ? "Working…" : "Save"}
    </button>
    <button onclick={clear} disabled={busy} class="secondary">
      Clear
    </button>
  </div>

  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}
</section>

<style>
  section {
    max-width: 520px;
    margin: 0 auto;
    padding: 1rem;
  }

  label {
    display: block;
    margin-top: 1rem;
  }

  label span {
    display: block;
    font-size: 0.85rem;
    margin-bottom: 0.25rem;
  }

  input {
    width: 100%;
    padding: 0.5rem;
    border-radius: 4px;
    border: 1px solid #888;
    font: inherit;
  }

  .actions {
    margin-top: 1rem;
    display: flex;
    gap: 0.5rem;
  }

  button {
    padding: 0.5rem 1rem;
    border-radius: 6px;
    border: 1px solid #535bf2;
    background: #535bf2;
    color: white;
    font-weight: 500;
    cursor: pointer;
  }

  button.secondary {
    background: transparent;
    color: inherit;
  }

  button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .status {
    color: #555;
    font-size: 0.9rem;
  }

  .error {
    margin-top: 1rem;
    color: #c00;
    font-size: 0.9rem;
  }
</style>
