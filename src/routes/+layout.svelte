<script lang="ts">
  import "../app.css";
  import "@fontsource-variable/literata";
  import "@fontsource-variable/nunito/wght.css";
  import { page } from "$app/state";
  import { check } from "@tauri-apps/plugin-updater";
  import { relaunch } from "@tauri-apps/plugin-process";
  import Button from "$lib/components/Button.svelte";
  import { commands } from "$lib/ipc/bindings";
  import { sidebar } from "$lib/stores/sidebar.svelte";

  let { children } = $props();

  const isActive = (path: string) => page.url.pathname.startsWith(path);

  const sections = [
    { href: "/library", label: "Library", icon: "M3 4h10v8H3z M3 7h10 M3 10h10" },
    { href: "/add", label: "Add", icon: "M8 3v10 M3 8h10" },
    { href: "/upload", label: "Quick upload", icon: "M8 12V4 M5 7l3-3 3 3 M3 13h10" },
    {
      href: "/settings",
      label: "Settings",
      icon:
        "M8 3.1a4.9 4.9 0 100 9.8 4.9 4.9 0 100-9.8 M8 5.9a2.1 2.1 0 100 4.2 2.1 2.1 0 100-4.2" +
        " M12.9 8h1.4 M1.7 8h1.4 M8 12.9v1.4 M8 1.7v1.4" +
        " M11.47 11.47l.99.99 M4.53 4.53l-.99-.99 M4.53 11.47l-.99.99 M11.47 4.53l.99-.99",
    },
  ];

  let dialog = $state<HTMLDialogElement | null>(null);
  let pending = $state<Awaited<ReturnType<typeof check>>>(null);
  let installing = $state(false);
  let error = $state<string | null>(null);
  let scrolled = $state(false);

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

  // The user's accent lives in NSColor, not in CSS — see app.css for why
  // `AccentColor` can't be used. Refreshed on every focus gain because that is
  // when a change made in System Settings becomes visible to us; AppKit has no
  // web-facing notification for it.
  async function pullSystemAccent() {
    const result = await commands.cmdSystemAccent();
    if (result.status !== "ok" || !result.data) return;
    const root = document.documentElement;
    root.style.setProperty("--color-accent", result.data.accent);
    root.style.setProperty("--color-accent-fg", result.data.accent_fg);
  }

  // AppKit dims accent-filled controls while the window is in the background.
  // Outside Tauri (browser, e2e) the window is always treated as active.
  $effect(() => {
    const root = document.documentElement;
    let stop: (() => void) | undefined;
    const apply = (focused: boolean) =>
      root.toggleAttribute("data-window-inactive", !focused);
    void (async () => {
      try {
        const { getCurrentWindow } = await import("@tauri-apps/api/window");
        const win = getCurrentWindow();
        apply(await win.isFocused());
        await pullSystemAccent();
        stop = await win.onFocusChanged(({ payload }) => {
          apply(payload);
          if (payload) void pullSystemAccent();
        });
      } catch {
        apply(true);
      }
    })();
    return () => {
      stop?.();
      apply(true);
    };
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

  const effectiveWidth = $derived(sidebar.collapsed ? 0 : sidebar.width);
  let isDragging = $state(false);

  function startResize(event: PointerEvent) {
    event.preventDefault();
    isDragging = true;
    const move = (ev: PointerEvent) => sidebar.setWidth(ev.clientX);
    const stop = () => {
      isDragging = false;
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", stop);
      window.removeEventListener("pointercancel", stop);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop);
    window.addEventListener("pointercancel", stop);
  }
</script>

{#snippet toggleIcon()}
  <svg
    width="16"
    height="16"
    viewBox="0 0 16 16"
    fill="none"
    stroke="currentColor"
    stroke-width="1.3"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    <rect x="2" y="3" width="12" height="10" rx="1.5" />
    <line x1="6" y1="3" x2="6" y2="13" />
  </svg>
{/snippet}

<div
  class="app-shell"
  class:dragging={isDragging}
  data-sidebar-collapsed={sidebar.collapsed}
  style="grid-template-columns: {effectiveWidth}px 1fr; --sidebar-width: {effectiveWidth}px"
>
  <div
    id="app-sidebar"
    class="app-sidebar flex flex-col gap-[4px] border-r border-sidebar-border px-[8px]"
  >
    <div class="flex h-[52px] flex-none items-center px-[8px] pb-[6px]">
      <div
        data-tauri-drag-region="deep"
        class="flex flex-1 items-center"
      >
        <span class="pl-[64px] text-sm text-fg-muted">
          <span class="brand-wordmark">LingQ</span> Importer
        </span>
      </div>
      <button
        type="button"
        class="sidebar-toggle"
        data-testid="sidebar-toggle"
        aria-label={sidebar.collapsed ? "Expand sidebar" : "Collapse sidebar"}
        aria-expanded={!sidebar.collapsed}
        aria-controls="app-sidebar"
        onclick={() => sidebar.toggle()}
      >
        {@render toggleIcon()}
      </button>
    </div>
    <nav aria-label="Sections" class="flex flex-col gap-[2px]">
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
  </div>

  <div
    class="sidebar-resize-handle"
    data-testid="sidebar-resize-handle"
    role="separator"
    aria-orientation="vertical"
    aria-label="Resize sidebar"
    hidden={sidebar.collapsed}
    onpointerdown={startResize}
  ></div>

  <main
    class="border-t px-8 pt-[55px] pb-8 transition-colors duration-120 {scrolled
      ? 'border-sidebar-border'
      : 'border-transparent'}"
    onscroll={(event) => (scrolled = event.currentTarget.scrollTop > 0)}
  >
    {@render children?.()}
  </main>

  <!-- Last in the shell so the sidebar strip stays the first drag region:
       it spans only main's 32px top padding, which no content occupies until
       the page scrolls — and scrolled-under content is inert in a titlebar
       on macOS anyway. -->
  <div data-tauri-drag-region="deep" class="titlebar-drag"></div>

  {#if sidebar.collapsed}
    <button
      type="button"
      class="sidebar-floating-toggle"
      data-testid="sidebar-floating-toggle"
      aria-label="Expand sidebar"
      aria-expanded="false"
      aria-controls="app-sidebar"
      onclick={() => sidebar.toggle()}
    >
      {@render toggleIcon()}
    </button>
  {/if}
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
  <div class="modal-card sheet-card">
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
      <Button variant="secondary" disabled={installing} onclick={later}>
        Later
      </Button>
      <Button disabled={installing} onclick={install}>
        {installing ? "Installing…" : "Install and restart"}
      </Button>
    </div>
  </div>
</dialog>

<style>
  /* px, not rem: exact pixel sizing, literal so it can't drift with the
     type scale — see src/app.css. */
  dialog {
    width: min(384px, calc(100vw - 32px));
  }

  .modal-card {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
