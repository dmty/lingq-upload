<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { page } from "$app/state";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import type { JobEvent } from "$lib/ipc/bindings";
  import ChapterRow from "$lib/components/ChapterRow.svelte";

  type Row = {
    index: number;
    title: string;
    status: "queued" | "in_flight" | "done";
    timestamp: string | null;
    degraded: boolean;
    dimmed: boolean;
  };

  const projectKey = $derived(page.params.projectId);

  let rows = $state<Row[]>([]);
  let unlisten: UnlistenFn | undefined;
  let running = $state(false);

  onMount(async () => {
    // For Sprint 2 the row set is bootstrapped from a future
    // cmd_project_load(projectId). Until that lands, render an empty
    // resumable list and reflect live JobEvent streams.
    rows = [];

    unlisten = await listen<JobEvent>("job", (e) => {
      const ev = e.payload;
      if (ev.kind === "Started") {
        running = true;
      } else if (ev.kind === "Progress") {
        // pct tracked per stage — we surface message text on rows.
      } else if (ev.kind === "Result" || ev.kind === "Cancelled") {
        running = false;
      }
    });
  });

  onDestroy(() => unlisten?.());
</script>

<section class="mx-auto max-w-3xl space-y-4 pt-6">
  <header class="flex items-center justify-between">
    <div>
      <h1 class="text-lg font-semibold text-fg">Run</h1>
      <p class="mt-1 text-xs text-fg-muted tabular">{projectKey}</p>
    </div>
    {#if running}
      <span
        class="rounded-sm bg-accent-soft px-2 py-1 text-xs font-medium text-accent"
      >
        running
      </span>
    {/if}
  </header>

  {#if rows.length === 0}
    <p class="rounded-sm border border-border bg-surface p-4 text-sm text-fg-muted">
      No chapter receipts yet. Once a job starts, per-chapter rows stream here in
      order.
    </p>
  {:else}
    <ul>
      {#each rows as r (r.index)}
        <ChapterRow
          index={r.index}
          title={r.title}
          status={r.status}
          timestamp={r.timestamp}
          degraded={r.degraded}
          dimmed={r.dimmed}
        />
      {/each}
    </ul>
  {/if}
</section>
