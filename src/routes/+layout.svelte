<script lang="ts">
  import "../app.css";
  import { page } from "$app/state";
  import { check } from "@tauri-apps/plugin-updater";
  import { relaunch } from "@tauri-apps/plugin-process";

  let { children } = $props();

  const isActive = (path: string) => page.url.pathname.startsWith(path);

  // ponytail: confirm() prompt is the minimum viable update UX;
  // upgrade to an in-app modal when releases have real users.
  $effect(() => {
    if (import.meta.env.DEV) return;
    void (async () => {
      try {
        const update = await check();
        if (!update) return;
        const accept = confirm(
          `Update ${update.version} available. Install and restart?`,
        );
        if (!accept) return;
        await update.downloadAndInstall();
        await relaunch();
      } catch (err) {
        console.error("updater check failed", err);
      }
    })();
  });
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
