<script lang="ts">
  import "../app.css";
  import "@fontsource-variable/literata";
  import { page } from "$app/state";
  import { check } from "@tauri-apps/plugin-updater";
  import { relaunch } from "@tauri-apps/plugin-process";
  import Button from "$lib/components/Button.svelte";

  let { children } = $props();

  const isActive = (path: string) => page.url.pathname.startsWith(path);

  const sections = [
    { href: "/library", label: "Library", icon: "M3 4h10v8H3z M3 7h10 M3 10h10" },
    { href: "/add", label: "Add", icon: "M8 3v10 M3 8h10" },
    { href: "/upload", label: "Quick upload", icon: "M8 12V4 M5 7l3-3 3 3 M3 13h10" },
    { href: "/settings", label: "Settings", icon: "M8 6a2 2 0 100 4 2 2 0 000-4 M8 2v1.5 M8 12.5V14 M2 8h1.5 M12.5 8H14" },
  ];

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

<div class="app-shell">
  <aside
    class="flex flex-col gap-1 border-r border-sidebar-border bg-sidebar px-2 pt-3"
  >
    <span class="px-2 pb-2 text-xs font-semibold text-fg-muted">
      LingQ Importer
    </span>
    <nav aria-label="Sections" class="flex flex-col gap-0.5">
      {#each sections as section (section.href)}
        <a
          href={section.href}
          class="source-row"
          aria-current={isActive(section.href) ? "page" : undefined}
        >
          <svg
            width="16"
            height="16"
            viewBox="0 0 16 16"
            fill="none"
            stroke="currentColor"
            stroke-width="1.3"
            stroke-linecap="round"
            aria-hidden="true"
          >
            <path d={section.icon} />
          </svg>
          {section.label}
        </a>
      {/each}
    </nav>
  </aside>

  <main class="px-8 pb-8">
    {@render children?.()}
  </main>
</div>

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
