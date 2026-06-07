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
  let info = $state<string | null>(null);
  let unlisten: UnlistenFn | undefined;
  let running = $state(false);
  // jobId is set only AFTER cmdStartProjectJob resolves. Events arriving
  // before that (e.g. from a concurrent run of the same project in another
  // tab) MUST be dropped — without the server id we can't tell ours from
  // theirs. The orchestrator persists receipts, so a drop here is recovered
  // by reloadProject() on terminal events.
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
    info = null;
    starting = true;
    const res = await commands.cmdStartProjectJob(project!.id);
    starting = false;
    if (res.status === "error") {
      const msg = appErrorMessage(res.error);
      // Backend rejects concurrent starts for the same project. Surface as
      // info rather than a red error — the user almost certainly clicked
      // twice; events from the existing run will keep streaming below.
      if (
        res.error.kind === "Other" &&
        msg.toLowerCase().includes("already running")
      ) {
        info =
          "This project is already running. Watch the chapter list update below.";
        running = true;
      } else {
        error = msg;
      }
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

  function goToMatch(ev: Extract<JobEvent, { kind: "NeedsMatch" }>): void {
    // bucket_preview is an array, so it rides via sessionStorage rather
    // than the URL. The match page reads + clears the key on Confirm.
    if (typeof sessionStorage !== "undefined") {
      const key = `bucketPreview:${projectKey}`;
      if (ev.bucket_preview) {
        sessionStorage.setItem(key, JSON.stringify(ev.bucket_preview));
      } else {
        sessionStorage.removeItem(key);
      }
    }
    const url =
      `/match/${encodeURIComponent(projectKey)}` +
      `?title=${encodeURIComponent(ev.title)}` +
      `&chapters=${ev.chapters}` +
      `&tracks=${ev.tracks}` +
      `&condition=${ev.condition}` +
      `&options=${ev.options.join(",")}` +
      `&preselect=${ev.preselect}`;
    goto(url);
  }

  const hasReceipts = $derived((project?.receipts?.length ?? 0) > 0);

  onMount(async () => {
    await reloadProject();

    unlisten = await listen<JobEvent>("job", async (e) => {
      const ev = e.payload;
      // Invariant: never accept an event unless we have a server-issued
      // jobId AND it matches. Drops cover the start-race window and any
      // crosstalk from concurrent project jobs on the same "job" channel.
      if (jobId === null || ev.job_id !== jobId) return;

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
        }
      } else if (ev.kind === "NeedsMatch") {
        running = false;
        goToMatch(ev);
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
      {:else if project}
        <button
          type="button"
          onclick={start}
          disabled={starting}
          class="rounded-sm bg-accent px-3 py-1 text-xs font-medium text-white hover:bg-accent-hover disabled:bg-fg-subtle"
        >
          {starting ? "Starting..." : hasReceipts ? "Resume" : "Start"}
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
  {/if}

  {#if info}
    <p
      class="rounded-sm border border-accent-soft bg-accent-soft/40 px-4 py-2 text-sm text-fg"
    >
      {info}
    </p>
  {/if}

  {#if rows.length === 0}
    <p
      class="rounded-sm border border-border bg-surface p-4 text-sm text-fg-muted"
    >
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
