<script lang="ts">
  let { current }: { current: 1 | 2 | 3 } = $props();
  const steps = ["Add", "Match", "Upload"];
</script>

<ol data-testid="step-indicator" class="flex items-center gap-1.5 text-xs">
  {#each steps as label, i}
    {#if i > 0}
      <li aria-hidden="true" class="text-fg-subtle">›</li>
    {/if}
    <li
      aria-current={i + 1 === current ? "step" : undefined}
      class={i + 1 === current
        ? "font-medium text-accent"
        : i + 1 < current
          ? "text-fg-muted"
          : "text-fg-subtle"}
    >
      {i + 1} {label}
    </li>
  {/each}
</ol>
