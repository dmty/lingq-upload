<script lang="ts">
  import Button from "$lib/components/Button.svelte";

  interface Props {
    stage: string | null;
    pct: number;
    message: string | null;
    onCancel?: () => void;
  }

  let { stage, pct, message, onCancel }: Props = $props();
</script>

<div class="p-6">
  <div class="flex items-baseline justify-between">
    <h2 class="text-lg font-semibold text-fg">{stage ?? "Working…"}</h2>
    <span class="tabular text-sm text-fg-muted">
      {Math.round(pct * 100)}%
    </span>
  </div>
  <div
    class="mt-3 h-1.5 w-full overflow-hidden rounded-full bg-surface-sunken"
    aria-live="polite"
  >
    <div
      class="h-full rounded-full bg-accent transition-[width] duration-180 ease-snappy"
      style:width="{Math.max(2, pct * 100)}%"
    ></div>
  </div>
  {#if message}
    <p class="mt-2 text-sm text-fg-muted">{message}</p>
  {/if}
  {#if onCancel}
    <div class="mt-4 flex justify-end">
      <Button variant="secondary" onclick={onCancel}>Cancel</Button>
    </div>
  {/if}
</div>
