<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { page } from "$app/state";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import {
    commands,
    type ChapterReceipt,
    type JobEvent,
    type PlanStep,
    type Project,
    type Stage,
  } from "$lib/ipc/bindings";
  import { appErrorMessage, isMissingApiKey } from "$lib/errors";
  import ChapterRow from "$lib/components/ChapterRow.svelte";
  import Button from "$lib/components/Button.svelte";
  import Alert from "$lib/components/Alert.svelte";
  import StepIndicator from "$lib/components/StepIndicator.svelte";

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
  let planSteps = $state<PlanStep[]>([]);
  let planFetched = false;
  let error = $state<string | null>(null);
  let errorNeedsKey = $state(false);
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
  let cancelling = $state(false);
  let completed = $state(false);
  let stage = $state<Stage["kind"] | null>(null);

  function uploadStatus(
    receipt: ChapterReceipt | undefined,
  ): Pick<Row, "status" | "timestamp"> {
    const uploaded = receipt?.lesson_id != null;
    return {
      status: uploaded ? "done" : "queued",
      timestamp: uploaded ? (receipt?.uploaded_at ?? null) : null,
    };
  }

  function receiptRow(r: ChapterReceipt): Row {
    return {
      index: r.chapter_index,
      title: `Chapter ${r.chapter_index + 1}`,
      ...uploadStatus(r),
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
    const loaded = result.data;
    project = loaded;

    // Fetch exactly once per mount, regardless of what it returns. The
    // preview re-probes every audio file on every call; reloadProject also
    // runs on Result and Cancelled, so latching only on a non-empty result
    // would leave a genuinely plan-less project (no audio yet) re-probing on
    // every terminal event. A finished or cancelled run cannot change the
    // plan; a mapping edit remounts the page.
    if (!planFetched) {
      planFetched = true;
      // cmd_project_load keys by string; every other project command takes
      // the structured ProjectId, which exists only once loaded.
      const preview = await commands.cmdProjectPlanPreview(loaded.id);
      planSteps = preview.status === "ok" ? preview.data : [];
    }

    const receipts = loaded.receipts ?? [];
    if (planSteps.length === 0) {
      // No plan yet (counts unmatched, or no audio source): receipts are the
      // only honest row source.
      rows = receipts.map(receiptRow).sort((a, b) => a.index - b.index);
      return;
    }

    const byIndex = new Map(receipts.map((r) => [r.chapter_index, r] as const));
    // Plan order is upload order; leftover and bucket steps are not in
    // numeric index order, so never re-sort a seeded queue.
    rows = planSteps.map((s) => {
      const receipt = byIndex.get(s.chapter_index);
      return {
        index: s.chapter_index,
        title: s.title,
        ...uploadStatus(receipt),
        degraded: receipt?.degraded ?? s.degraded,
        dimmed: false,
      };
    });
  }

  function upsertRow(
    index: number,
    patch: Partial<Row> & { title?: string },
  ): void {
    const idx = rows.findIndex((r) => r.index === index);
    // Seeded queues cover every planned index, so this branch means the
    // plan moved under us. Show the upload rather than hide it.
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
    completed = false;
    error = null;
    errorNeedsKey = false;
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
        // No jobId means no event stream: this page can't follow that run.
        info =
          "This project is already running elsewhere. Progress won't update here — press Cancel to stop it, or reopen from Library once it finishes.";
        running = true;
      } else {
        error = msg;
        errorNeedsKey = isMissingApiKey(res.error);
      }
      return;
    }
    jobId = res.data;
    running = true;
  }

  async function cancel() {
    if (!project || cancelling) return;
    cancelling = true;
    const res = await commands.cmdProjectCancel(project.id);
    if (res.status === "error") {
      error = appErrorMessage(res.error);
      cancelling = false;
      return;
    }
    // Started before this page mounted ("already running"): no jobId means
    // no event stream will deliver Cancelled — reset directly.
    if (jobId === null) {
      running = false;
      cancelling = false;
      await reloadProject();
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
  const doneCount = $derived(rows.filter((r) => r.status === "done").length);
  // Without a server-issued jobId every event is dropped, so nothing below
  // can advance. Progress UI must not imply otherwise.
  const attached = $derived(jobId !== null);
  // The run loop uploads sequentially (queue_cursor), so the first row that
  // isn't done yet is the one currently in flight. Gated on the uploading
  // stage: before it, the backend is still probing audio and parsing text,
  // and no chapter is in flight yet.
  const liveIndex = $derived(
    running && attached && stage === "uploading"
      ? rows.find((r) => r.status !== "done")?.index
      : undefined,
  );

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
        stage = ev.stage.kind;
      } else if (ev.kind === "StageChanged") {
        stage = ev.stage.kind;
      } else if (ev.kind === "ChapterDone") {
        upsertRow(ev.chapter_index, {
          status: "done",
          timestamp: new Date().toISOString(),
          degraded: ev.degraded,
        });
      } else if (ev.kind === "Result") {
        running = false;
        cancelling = false;
        if (ev.ok) {
          completed = true;
          await reloadProject();
        }
      } else if (ev.kind === "NeedsMatch") {
        running = false;
        goToMatch(ev);
      } else if (ev.kind === "Cancelled") {
        running = false;
        cancelling = false;
        await reloadProject();
      }
    });

    const auto = page.url.searchParams.get("autostart") === "1";
    if (auto && !running && !hasReceipts && project) {
      history.replaceState(null, "", `/run/${projectKey}`);
      await start();
    }
  });

  onDestroy(() => unlisten?.());
</script>

<section class="col-wide space-y-4 pt-6">
  <header class="flex items-center justify-between">
    <div>
      <StepIndicator current={3} />
      <a href="/library" class="text-xs text-fg-muted hover:text-fg">← Library</a>
      <h1 class="font-serif text-lg font-semibold text-fg">
        {project?.settings.collection_title ?? "Run"}
      </h1>
      <p class="mt-1 text-xs text-fg-muted tabular">
        {project?.settings.language ?? projectKey}
      </p>
    </div>
    <div class="flex items-center gap-2">
      {#if running}
        {#if attached && rows.length > 0}
          <div class="flex items-center gap-2.5">
            <div
              class="h-1.5 w-24 overflow-hidden rounded-full bg-surface-sunken"
              role="progressbar"
              aria-label="Chapters uploaded"
              aria-valuemin={0}
              aria-valuemax={rows.length}
              aria-valuenow={doneCount}
            >
              <div
                class="h-full rounded-full bg-accent transition-[width] duration-180 ease-snappy motion-reduce:transition-none"
                style:width="{(doneCount / rows.length) * 100}%"
              ></div>
            </div>
            <span class="text-xs font-medium text-accent tabular">
              {doneCount}/{rows.length}
            </span>
          </div>
        {:else}
          <span
            class="rounded-sm bg-accent-soft px-2 py-1 text-xs font-medium text-accent"
          >
            running
          </span>
        {/if}
        <Button
          variant="secondary"
          size="sm"
          onclick={cancel}
          disabled={cancelling}
        >
          {cancelling ? "Cancelling…" : "Cancel"}
        </Button>
      {:else if project && (project.confirmed_at != null || hasReceipts)}
        <Button size="sm" onclick={start} disabled={starting}>
          {starting ? "Starting..." : hasReceipts ? "Resume" : "Start"}
        </Button>
      {/if}
    </div>
  </header>

  {#if completed}
    <Alert
      variant="success"
      data-testid="run-complete"
      class="flex items-center justify-between gap-3 px-4 py-3"
    >
      <span>All chapters uploaded.</span>
      <span class="flex items-center gap-3">
        {#if project?.lingq_collection_id != null}
          <button
            type="button"
            class="font-medium text-accent hover:underline"
            onclick={() =>
              void goto(`/course/${encodeURIComponent(projectKey)}`)}
          >
            View Course
          </button>
        {/if}
        <a href="/library" class="text-fg-muted hover:text-fg">Back to Library</a>
      </span>
    </Alert>
  {/if}

  {#if error}
    <Alert body class="px-4 py-2">
      {error}
      {#if errorNeedsKey}
        <a href="/settings" class="ml-1 font-medium text-accent underline">Open Settings</a>
      {/if}
    </Alert>
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
      {#if running}
        Preparing the queue… chapter rows appear as each one finishes.
      {:else}
        No chapters queued yet. Press Start to begin uploading.
      {/if}
    </p>
  {:else}
    <ul data-testid="chapter-rows">
      {#each rows as r (r.index)}
        <ChapterRow
          index={r.index}
          title={r.title}
          status={r.index === liveIndex ? "in_flight" : r.status}
          timestamp={r.timestamp}
          degraded={r.degraded}
          dimmed={r.dimmed}
        />
      {/each}
    </ul>
  {/if}
</section>
