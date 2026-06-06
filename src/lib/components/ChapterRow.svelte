<script lang="ts">
  type Status = "queued" | "in_flight" | "done";
  let {
    index,
    title,
    status,
    timestamp,
    degraded = false,
    dimmed = false,
  }: {
    index: number;
    title: string;
    status: Status;
    timestamp?: string | null;
    degraded?: boolean;
    dimmed?: boolean;
  } = $props();

  function fmt(ts: string | null | undefined): string {
    if (!ts) return "";
    try {
      const d = new Date(ts);
      return `uploaded ${d.toLocaleDateString(undefined, { weekday: "short" })}`;
    } catch {
      return "";
    }
  }
</script>

<li
  class="flex items-center gap-3 border-b border-border py-2 last:border-b-0 {dimmed
    ? 'opacity-60'
    : ''}"
>
  <span class="text-xs font-medium text-fg-subtle tabular w-8">
    {index + 1}
  </span>
  {#if status === "done"}
    <span class="inline-flex h-4 w-4 items-center justify-center rounded-full bg-success text-white text-[10px]">
      ✓
    </span>
  {:else if status === "in_flight"}
    <span
      class="inline-block h-4 w-4 animate-spin rounded-full border-2 border-fg-subtle border-t-accent"
      aria-label="in flight"
    ></span>
  {:else}
    <span class="inline-block h-4 w-4 rounded-full border border-fg-subtle"></span>
  {/if}

  <span class="flex-1 text-sm text-fg">{title}</span>
  {#if degraded}
    <span
      class="rounded-sm bg-warning/10 px-2 py-0.5 text-[11px] font-medium text-warning"
    >
      degraded
    </span>
  {/if}
  {#if status === "done" && timestamp}
    <span class="text-xs text-fg-subtle">{fmt(timestamp)}</span>
  {/if}
</li>
