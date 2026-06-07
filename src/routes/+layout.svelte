<script lang="ts">
  import "../app.css";
  import { page } from "$app/state";

  let { children } = $props();

  const isActive = (path: string) => page.url.pathname.startsWith(path);
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
