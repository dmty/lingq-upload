<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { goto } from "$app/navigation";
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

  const projectKey = $derived(page.params.projectId ?? "");

  let project = $state<Project | null>(null);
  let rows = $state<Row[]>([]);
  let error = $state<string | null>(null);
  let unlisten: UnlistenFn | undefined;
  let running = $state(false);
  let jobId = $state<string | null>(null);
  let starting = $state(false);

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

  async function reloadProject() {
    const result = await commands.cmdProjectLoad(projectKey);
    if (result.status === "error") {
      error = appErrorMessage(result.error);
      return;
    }
    project = result.data;
    rows = (project.receipts ?? []).map(receiptRow);
  }

  function upsertRow(
    index: number,
    patch: Partial<Row> & { title?: string },
  ): void {
    const idx = rows.findIndex((r) => r.index === index);
    if (idx === -1) {
      rows = [
        ...rows,
        {
          index,
          title: patch.title ?? `Chapter ${index + 1}`,
          status: patch.status ?? "queued",
          timestamp: patch.timestamp ?? null,
          degraded: patch.degraded ?? false,
          dimmed: patch.dimmed ?? false,
        },
      ];
    } else {
      rows = rows.map((r, i) => (i === idx ? { ...r, ...patch } : r));
    }
  }

  async function start() {
    error = null;
    starting = true;
    const res = await commands.cmdStartProjectJob({
      content_hash: project!.id.content_hash,
      audible_asin: project!.id.audible_asin ?? null,
      isbn13: project!.id.isbn13 ?? null,
      calibre_uuid: project!.id.calibre_uuid ?? null,
    });
    starting = false;
    if (res.status === "error") {
      error = appErrorMessage(res.error);
      return;
    }
    jobId = res.data;
    running = true;
  }

  async function cancel() {
    if (!jobId) return;
    const res = await commands.cmdCancelJob(jobId);
    if (res.status === "error") {
      error = appErrorMessage(res.error);
    }
  }

  function handleResultPayload(payload: unknown): void {
    if (typeof payload !== "object" || payload === null) return;
    const p = payload as Record<string, unknown>;
    if (p.needs_match === true) {
      const params = new URLSearchParams();
      if (typeof p.title === "string") params.set("title", p.title);
      if (typeof p.chapters === "number")
        params.set("chapters", String(p.chapters));
      if (typeof p.tracks === "number") params.set("tracks", String(p.tracks));
      if (typeof p.condition === "string") params.set("condition", p.condition);
      if (Array.isArray(p.options))
        params.set("options", (p.options as string[]).join(","));
      if (typeof p.preselect === "string") params.set("preselect", p.preselect);
      goto(`/match/${projectKey}?${params.toString()}`);
    }
  }

  onMount(async () => {
    await reloadProject();

    unlisten = await listen<JobEvent>("job", async (e) => {
      const ev = e.payload;
      // Filter by jobId once we have one — concurrent project events would
      // otherwise interleave through this same global "job" channel.
      if (jobId && "job_id" in ev && ev.job_id !== jobId) return;

      if (ev.kind === "Started") {
        running = true;
      } else if (ev.kind === "ChapterDone") {
        upsertRow(ev.chapter_index, {
          status: "done",
          timestamp: new Date().toISOString(),
          degraded: ev.degraded,
        });
      } else if (ev.kind === "Result") {
        running = false;
        if (ev.ok) {
          await reloadProject();
        } else {
          handleResultPayload(ev.payload);
        }
      } else if (ev.kind === "Cancelled") {
        running = false;
        await reloadProject();
      }
    });
  });

  onDestroy(() => unlisten?.());
</script>

<section class="mx-auto max-w-3xl space-y-4 pt-6">
  <header class="flex items-center justify-between">
    <div>
      <h1 class="text-lg font-semibold text-fg">
        {project?.settings.collection_title ?? "Run"}
      </h1>
      <p class="mt-1 text-xs text-fg-muted tabular">
        {project?.settings.language ?? projectKey}
      </p>
    </div>
    <div class="flex items-center gap-2">
      {#if running}
        <span
          class="rounded-sm bg-accent-soft px-2 py-1 text-xs font-medium text-accent"
        >
          running
        </span>
        <button
          type="button"
          onclick={cancel}
          class="rounded-sm border border-border bg-surface px-3 py-1 text-xs font-medium text-fg hover:bg-surface-sunken"
        >
          Cancel
        </button>
      {:else if project && rows.length === 0}
        <button
          type="button"
          onclick={start}
          disabled={starting}
          class="rounded-sm bg-accent px-3 py-1 text-xs font-medium text-white hover:bg-accent-hover disabled:bg-fg-subtle"
        >
          {starting ? "Starting..." : "Start"}
        </button>
      {/if}
    </div>
  </header>

  {#if error}
    <p
      class="rounded-sm border border-error-soft bg-error-soft/30 px-4 py-2 text-sm text-fg"
    >
      {error}
    </p>
  {:else if rows.length === 0}
    <p class="rounded-sm border border-border bg-surface p-4 text-sm text-fg-muted">
      No chapter receipts yet. Press Start to begin uploading; per-chapter rows
      will stream here in order.
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
