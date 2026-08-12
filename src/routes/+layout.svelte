<script lang="ts">
  import "../app.css";
  import "@fontsource-variable/literata";
  import { page } from "$app/state";
  import { check } from "@tauri-apps/plugin-updater";
  import { relaunch } from "@tauri-apps/plugin-process";
  import Button from "$lib/components/Button.svelte";

  let { children } = $props();

  const isActive = (path: string) => page.url.pathname.startsWith(path);

  let dialog = $state<HTMLDialogElement | null>(null);
  let pending = $state<Awaited<ReturnType<typeof check>>>(null);
  let installing = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    if (!import.meta.env.PROD) return;
    void (async () => {
      try {
        pending = await check();
      } catch (err) {
        console.error("updater check failed", err);
      }
    })();
  });

  $effect(() => {
    if (pending && dialog && !dialog.open) dialog.showModal();
  });

  function later() {
    if (installing) return;
    pending = null;
    error = null;
    dialog?.close();
  }

  async function install() {
    if (!pending || installing) return;
    installing = true;
    error = null;
    try {
      await pending.downloadAndInstall();
      await relaunch();
    } catch (err) {
      error = err instanceof Error ? err.message : "Update failed.";
      installing = false;
    }
  }
</script>

<header class="sticky top-0 z-10 flex h-13 items-center gap-4 bg-canvas px-8">
  <span class="text-sm font-medium text-fg-muted">LingQ Importer</span>
  <nav class="flex items-center gap-1">
    <a
      href="/library"
      class="rounded-sm px-4 py-1.5 text-sm font-medium transition-colors duration-120 {isActive(
        '/library',
      )
        ? 'bg-accent-soft text-fg'
        : 'text-fg-muted hover:bg-surface-sunken hover:text-fg'}"
    >
      Library
    </a>
    <a
      href="/add"
      class="rounded-sm px-4 py-1.5 text-sm font-medium transition-colors duration-120 {isActive(
        '/add',
      )
        ? 'bg-accent-soft text-fg'
        : 'text-fg-muted hover:bg-surface-sunken hover:text-fg'}"
    >
      Add
    </a>
    <a
      href="/upload"
      class="rounded-sm px-4 py-1.5 text-sm font-medium transition-colors duration-120 {isActive(
        '/upload',
      )
        ? 'bg-accent-soft text-fg'
        : 'text-fg-muted hover:bg-surface-sunken hover:text-fg'}"
    >
      Quick upload
    </a>
    <a
      href="/settings"
      class="rounded-sm px-4 py-1.5 text-sm font-medium transition-colors duration-120 {isActive(
        '/settings',
      )
        ? 'bg-accent-soft text-fg'
        : 'text-fg-muted hover:bg-surface-sunken hover:text-fg'}"
    >
      Settings
    </a>
  </nav>
</header>

<main class="px-8 pb-8">
  {@render children?.()}
</main>

<dialog
  bind:this={dialog}
  aria-labelledby="update-title"
  aria-describedby="update-copy"
  aria-modal="true"
  oncancel={(event) => {
    event.preventDefault();
    later();
  }}
>
  <div class="modal-card">
    <h2 id="update-title" class="text-lg font-semibold text-fg">
      Update available
    </h2>
    <p id="update-copy" class="text-sm text-fg-muted">
      Version {pending?.version} is ready. Install and restart now?
    </p>
    {#if error}
      <p class="text-sm text-error" role="alert" aria-live="assertive">
        {error}
      </p>
    {/if}
    <div class="actions">
      <button
        type="button"
        class="rounded-sm border border-border bg-surface px-3 py-1.5 text-sm font-medium text-fg hover:bg-surface-sunken disabled:opacity-50"
        disabled={installing}
        onclick={later}
      >
        Later
      </button>
      <Button disabled={installing} onclick={install}>
        {installing ? "Installing…" : "Install and restart"}
      </Button>
    </div>
  </div>
</dialog>

<style>
  dialog {
    width: min(24rem, calc(100vw - 2rem));
    max-width: none;
    padding: 0;
    border: 0;
    border-radius: 0.5rem;
    background: transparent;
    box-shadow: var(--shadow-card);
  }

  dialog::backdrop {
    background: rgb(0 0 0 / 0.55);
  }

  .modal-card {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    padding: 1.5rem;
    background: var(--color-surface);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
  }
</style>
