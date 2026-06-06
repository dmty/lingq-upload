<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { page } from "$app/state";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import {
    commands,
    type ChapterReceipt,
    type JobEvent,
    type Project,
  } from "$lib/ipc/bindings";
  import { appErrorMessage } from "$lib/errors";
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

  let project = $state<Project | null>(null);
  let rows = $state<Row[]>([]);
  let error = $state<string | null>(null);
  let unlisten: UnlistenFn | undefined;
  let running = $state(false);

  function receiptRow(r: ChapterReceipt): Row {
    return {
      index: r.chapter_index,
      title: `Chapter ${r.chapter_index + 1}`,
      status: "done",
      timestamp: r.uploaded_at ?? null,
      degraded: !!r.degraded,
      dimmed: false,
    };
  }

  onMount(async () => {
    const result = await commands.cmdProjectLoad(projectKey);
    if (result.status === "error") {
      error = appErrorMessage(result.error);
    } else {
      project = result.data;
      const receipts = project.receipts ?? [];
      rows = receipts.map(receiptRow);
    }

    // JobEvent kinds are PascalCase (Started/Progress/Result/Cancelled — see
    // events::JobEvent). Channel is the global "job" emitter; concurrent
    // multi-project runs would interleave here. Live per-row updates land in
    // a future iteration.
    unlisten = await listen<JobEvent>("job", (e) => {
      const ev = e.payload;
      if (ev.kind === "Started") {
        running = true;
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

  {#if error}
    <p
      class="rounded-sm border border-error-soft bg-error-soft/30 px-4 py-2 text-sm text-fg"
    >
      {error}
    </p>
  {:else if rows.length === 0}
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
