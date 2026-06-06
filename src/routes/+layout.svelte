<script lang="ts">
  let { children } = $props();
</script>

<nav>
  <a href="/">Upload</a>
  <a href="/settings">Settings</a>
</nav>

<main>
  {@render children?.()}
</main>

<style>
  nav {
    display: flex;
    gap: 1rem;
    padding: 0.75rem 1rem;
    border-bottom: 1px solid #ddd;
    background: #fafafa;
  }

  nav a {
    text-decoration: none;
    color: #333;
    font-weight: 500;
  }

  nav a:hover {
    color: #535bf2;
  }

  main {
    padding: 1rem;
  }
</style>
