<script lang="ts">
  import "../app.css";
  import { page } from "$app/state";

  let { children } = $props();

  const isActive = (path: string) =>
    path === "/"
      ? page.url.pathname === "/"
      : page.url.pathname.startsWith(path);
</script>

<header
  class="sticky top-0 z-10 flex h-13 items-center gap-4 bg-canvas px-8"
>
  <span class="text-sm font-medium text-fg-muted">LingQ Importer</span>
  <nav class="flex items-center gap-1">
    <a
      href="/"
      class="rounded-sm px-4 py-1.5 text-sm font-medium transition-colors duration-120 {isActive(
        '/',
      )
        ? 'bg-accent-soft text-fg'
        : 'text-fg-muted hover:bg-surface-sunken hover:text-fg'}"
    >
      Upload
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

<main class="px-8 pb-16">
  {@render children?.()}
</main>
