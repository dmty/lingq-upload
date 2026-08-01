<script lang="ts">
  import Spinner from "$lib/components/Spinner.svelte";

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
  data-testid="chapter-row"
  data-status={status}
  class="flex items-center gap-3 border-b border-border py-2 last:border-b-0 {dimmed
    ? 'opacity-60'
    : ''} {status === 'in_flight'
    ? 'border-l-2 border-l-accent bg-accent-soft/40 pl-2'
    : ''}"
>
  <span class="text-xs font-medium text-fg-subtle tabular w-8">
    {index + 1}
  </span>
  {#if status === "done"}
    <span
      role="img"
      aria-label="Uploaded"
      class="inline-flex h-4 w-4 items-center justify-center rounded-full bg-success text-canvas text-[10px]"
    >
      ✓
    </span>
  {:else if status === "in_flight"}
    <!-- aria-label is ignored on a bare span; name it from a sibling instead. -->
    <Spinner tone="muted" aria-hidden="true" />
    <span class="sr-only">Uploading</span>
  {:else}
    <span
      role="img"
      aria-label="Queued"
      class="inline-block h-4 w-4 rounded-full border border-fg-subtle"
    ></span>
  {/if}

  <span class="flex-1 text-sm {status === 'done' ? 'text-fg-muted' : 'text-fg'}"
    >{title}</span
  >
  {#if degraded}
    <span
      class="rounded-sm bg-warning-soft px-2 py-0.5 text-[11px] font-medium text-warning"
    >
      degraded
    </span>
  {/if}
  {#if status === "done" && timestamp}
    <span class="text-xs text-fg-subtle">{fmt(timestamp)}</span>
  {/if}
</li>
