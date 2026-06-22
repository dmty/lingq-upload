<script lang="ts">
  import { goto } from "$app/navigation";
  import { page } from "$app/state";
  import { open } from "@tauri-apps/plugin-dialog";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import {
    commands,
    type AbsorbPolicy,
    type AudioSource,
    type BucketPreview,
    type MappingOp,
    type MismatchCondition,
    type MismatchResponse,
    type ProjectId as ProjectIdType,
  } from "$lib/ipc/bindings";
  import { appErrorMessage } from "$lib/errors";
  import { basename, extOf } from "$lib/paths";
  import MismatchEvidence from "$lib/components/MismatchEvidence.svelte";
  import ResponseCard from "$lib/components/ResponseCard.svelte";
  import MappingGrid from "$lib/components/MappingGrid.svelte";
  import ChapterInspector from "$lib/components/ChapterInspector.svelte";
  import ProjectSettings from "$lib/components/ProjectSettings.svelte";
  import DropZone from "$lib/components/DropZone.svelte";
  import { mapping } from "$lib/stores/mapping.svelte";

  const projectKey = $derived(page.params.projectId ?? "");
  const previewKey = $derived(`bucketPreview:${projectKey}`);

  // The Run page seeds these via URL params on the live NeedsMatch event;
  // a cold entry from /library carries no params and must re-probe via
  // `cmd_matcher_inspect`. Local $state lets either source populate the form.
  let title = $state<string>("Untitled");
  let chapters = $state(0);
  let tracks = $state(0);
  let condition = $state<MismatchCondition>("count_off");
  let options = $state<MismatchResponse[]>(["cancel"]);
  let bucketPreview = $state<BucketPreview[] | null>(null);
  let selected = $state<MismatchResponse>("cancel");
  let hydrating = $state(true);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let strategy = $state<MismatchResponse>("split_proportional");

  // Project-scope settings (absorb policy) live here so the user can adjust
  // them before locking the mapping in. ProjectSettings owns the debounced
  // persistence; the page only feeds the current values once loaded.
  let projectIdValue = $state<ProjectIdType | null>(null);
  let absorbPolicy = $state<AbsorbPolicy>("forward");
  let settingsOpen = $state(false);

  // Audio-expand affordance (sub-case A only: no receipts yet). Toggled by the
  // "+ Add more audio" button; reveals a DropZone seeded from the project's
  // current audio source.
  let audioPaths = $state<string[]>([]);
  let audioOriginFolder = $state<string | null>(null);
  let receiptCount = $state(0);
  let expandOpen = $state(false);
  let busyReplace = $state(false);
  let replaceError = $state<string | null>(null);
  let audioDropEl = $state<HTMLButtonElement | null>(null);
  let localHover = $state(false);

  // Single load path: the store does the one cmd_project_load round trip and
  // exposes everything the page needs (id, absorb policy, mapping, chapters).
  // The page component is reused across `/match/:projectId` navigations, so
  // every project-scoped $state above must be re-seeded each run and any
  // in-flight load for a previous projectKey must not clobber the new one.
  $effect(() => {
    const key = projectKey;
    resetProjectState();
    if (!key) {
      hydrating = false;
      return;
    }
    let cancelled = false;
    void (async () => {
      await mapping.load(key);
      if (cancelled || projectKey !== key) return;
      if (mapping.status === "error") {
        error = mapping.error;
        hydrating = false;
        return;
      }
      projectIdValue = mapping.projectId;
      absorbPolicy = mapping.absorbPolicy;
      await refreshAudioState(key);
      if (cancelled || projectKey !== key) return;
      if (applyParams()) {
        hydrating = false;
        return;
      }
      await hydrateFromBackend(key);
      if (cancelled || projectKey !== key) return;
    })();
    return () => {
      cancelled = true;
    };
  });

  async function refreshAudioState(key: string) {
    const loaded = await commands.cmdProjectLoad(key);
    if (projectKey !== key) return;
    if (loaded.status === "error") {
      audioPaths = [];
      audioOriginFolder = null;
      receiptCount = 0;
      return;
    }
    const project = loaded.data;
    const resp = project.matcher_decision?.response;
    if (resp === "split_proportional" || resp === "single_lesson") {
      strategy = resp;
    }
    receiptCount = project.receipts?.length ?? 0;
    const src = project.sources.audio ?? null;
    if (src === null) {
      audioPaths = [];
      audioOriginFolder = null;
      return;
    }
    if (src.kind === "folder") {
      const expanded = await commands.cmdExpandAudioDir(src.value);
      if (projectKey !== key) return;
      audioPaths = expanded.status === "ok" ? expanded.data : [];
      audioOriginFolder = basename(src.value);
      return;
    }
    if (src.kind === "multiple_files") {
      audioPaths = src.value;
      audioOriginFolder = null;
      return;
    }
    // single_file | libation_manifest
    audioPaths = [src.value];
    audioOriginFolder = null;
  }

  function resetProjectState() {
    title = "Untitled";
    chapters = 0;
    tracks = 0;
    condition = "count_off";
    options = ["cancel"];
    bucketPreview = null;
    selected = "cancel";
    strategy = "split_proportional";
    hydrating = true;
    busy = false;
    error = null;
    projectIdValue = null;
    absorbPolicy = "forward";
    settingsOpen = false;
    audioPaths = [];
    audioOriginFolder = null;
    receiptCount = 0;
    expandOpen = false;
    busyReplace = false;
    replaceError = null;
    localHover = false;
  }

  function applyParams(): boolean {
    const p = page.url.searchParams;
    const c = Number(p.get("chapters") ?? "0");
    const t = Number(p.get("tracks") ?? "0");
    const opts = p.get("options")?.split(",") as MismatchResponse[] | undefined;
    if (!p.has("condition") && c === 0 && t === 0 && !opts) return false;
    title = p.get("title") ?? "Untitled";
    chapters = c;
    tracks = t;
    condition = (p.get("condition") as MismatchCondition) ?? "count_off";
    options = opts ?? ["cancel"];
    selected =
      (p.get("preselect") as MismatchResponse) ?? options[0] ?? "cancel";
    if (typeof sessionStorage !== "undefined") {
      const raw = sessionStorage.getItem(previewKey);
      if (raw) {
        try {
          bucketPreview = JSON.parse(raw) as BucketPreview[];
        } catch {
          bucketPreview = null;
        }
      }
    }
    return true;
  }

  async function hydrateFromBackend(key: string) {
    const pid = mapping.projectId;
    if (!pid) {
      hydrating = false;
      return;
    }
    const inspected = await commands.cmdMatcherInspect(pid);
    if (projectKey !== key) return;
    if (inspected.status === "error") {
      error = appErrorMessage(inspected.error);
      hydrating = false;
      return;
    }
    if (inspected.data == null) {
      goto(`/run/${key}`);
      return;
    }
    const data = inspected.data;
    title = data.title;
    chapters = data.chapter_count;
    tracks = data.track_count;
    condition = data.condition;
    options = data.options;
    selected = data.preselect;
    bucketPreview = data.bucket_preview;
    hydrating = false;
  }

  function formatDuration(sec: number): string {
    const total = Math.max(0, Math.round(sec));
    const h = Math.floor(total / 3600);
    const m = Math.floor((total % 3600) / 60);
    const s = total % 60;
    const pad = (n: number) => n.toString().padStart(2, "0");
    return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`;
  }

  function median(values: number[]): number {
    if (values.length === 0) return 0;
    const sorted = [...values].sort((a, b) => a - b);
    const mid = Math.floor(sorted.length / 2);
    return sorted.length % 2 === 0
      ? (sorted[mid - 1] + sorted[mid]) / 2
      : sorted[mid];
  }

  const driftMedian = $derived(
    median(
      (bucketPreview ?? []).map((b) => b.charsPerSec).filter((v) => v > 0),
    ),
  );

  function isDrifting(row: BucketPreview): boolean {
    if (driftMedian <= 0 || row.charsPerSec <= 0) return false;
    const deviation = Math.abs(row.charsPerSec - driftMedian) / driftMedian;
    return deviation > 0.3;
  }

  const mappingGateOk = $derived(mapping.gateContinue());

  const matterIds = $derived(
    mapping.chapters.filter((c) => c.kind !== "body").map((c) => c.id),
  );
  const allMatterSkipped = $derived(
    matterIds.length > 0 &&
      matterIds.every((id) => mapping.skippedIds.includes(id)),
  );

  function toggleMatter() {
    const set = new Set(mapping.skippedIds);
    if (allMatterSkipped) {
      for (const id of matterIds) set.delete(id);
    } else {
      for (const id of matterIds) set.add(id);
    }
    mapping.setSkipped([...set]).catch(() => {});
  }

  // submitOp/confirmPair reject when their flush turn fails — that is the
  // signal for awaiting callers; fire-and-forget call sites swallow it
  // (AD-025: the reverted row colour is the only failure surface).
  function handleMappingOp(op: MappingOp) {
    mapping.submitOp(op).catch(() => {});
  }

  function handleConfirmPair(chapterId: string) {
    mapping.confirmPair(chapterId).catch(() => {});
  }

  async function handleMappingContinue() {
    const epoch = mapping.revertEpoch;
    try {
      await mapping.flush();
    } catch {
      return;
    }
    // A revert during the final flush means the save failed — stay on the
    // page and let the reverted row colour speak (AD-025: no banner).
    if (mapping.revertEpoch !== epoch || !mapping.gateContinue()) return;
    goto(`/run/${projectKey}`);
  }

  async function confirm() {
    busy = true;
    error = null;
    if (selected === "cancel") {
      goto("/library");
      return;
    }
    const pid = mapping.projectId;
    if (!pid) {
      error = "Failed to load project";
      busy = false;
      return;
    }
    const resolved = await commands.cmdMatcherResolve(
      pid,
      condition,
      selected,
      chapters,
      tracks,
    );
    if (resolved.status === "error") {
      error = appErrorMessage(resolved.error);
      busy = false;
      return;
    }
    if (typeof sessionStorage !== "undefined") {
      sessionStorage.removeItem(previewKey);
    }
    // Re-hydrate from the backend so the seeded MappingState replaces the
    // resolver UI with the mapping grid. The grid's own Continue button
    // takes the user to /run.
    await mapping.load(projectKey);
    busy = false;
  }

  async function setStrategy(next: MismatchResponse) {
    const pid = mapping.projectId;
    if (!pid || next === strategy) return;
    const prev = strategy;
    strategy = next;
    const res = await commands.cmdMatcherResolve(pid, condition, next, chapters, tracks);
    if (res.status !== "ok") {
      strategy = prev;
      return;
    }
    await mapping.load(projectKey);
  }

  const AUDIO_EXTS = ["m4b", "m4a", "mp3"];

  function toAudioSource(paths: string[]): AudioSource {
    if (paths.length === 1) {
      return { kind: "single_file", value: paths[0] } as AudioSource;
    }
    return { kind: "multiple_files", value: paths } as AudioSource;
  }

  async function replaceAudio(nextPaths: string[], nextOrigin: string | null) {
    if (!projectIdValue) return;
    const pid = projectIdValue;
    const key = projectKey;
    busyReplace = true;
    replaceError = null;
    const prevPaths = audioPaths;
    const prevOrigin = audioOriginFolder;
    audioPaths = nextPaths;
    audioOriginFolder = nextOrigin;
    const res = await commands.cmdReplaceAudioSource(
      pid,
      toAudioSource(nextPaths),
    );
    if (projectKey !== key) {
      busyReplace = false;
      return;
    }
    if (res.status === "error") {
      replaceError = appErrorMessage(res.error);
      // Roll back local state from the server.
      audioPaths = prevPaths;
      audioOriginFolder = prevOrigin;
      await refreshAudioState(key);
      busyReplace = false;
      return;
    }
    // Re-load mapping so the mismatch panel re-renders against the new track
    // count, refresh audio + receipts, then re-probe the matcher. When the
    // count now matches, cmd_matcher_inspect returns None and we redirect.
    await mapping.load(key);
    if (projectKey !== key) {
      busyReplace = false;
      return;
    }
    projectIdValue = mapping.projectId;
    absorbPolicy = mapping.absorbPolicy;
    await refreshAudioState(key);
    if (projectKey !== key) {
      busyReplace = false;
      return;
    }
    const inspectPid = mapping.projectId;
    if (inspectPid) {
      const inspected = await commands.cmdMatcherInspect(inspectPid);
      if (projectKey !== key) {
        busyReplace = false;
        return;
      }
      if (inspected.status === "ok") {
        if (inspected.data == null) {
          busyReplace = false;
          goto(`/run/${encodeURIComponent(key)}`);
          return;
        }
        const data = inspected.data;
        title = data.title;
        chapters = data.chapter_count;
        tracks = data.track_count;
        condition = data.condition;
        options = data.options;
        selected = data.preselect;
        bucketPreview = data.bucket_preview;
      }
    }
    busyReplace = false;
  }

  async function pickAndReplace() {
    if (busyReplace) return;
    const sel = await open({
      multiple: true,
      filters: [{ name: "Audio", extensions: AUDIO_EXTS }],
    });
    const picked: string[] =
      sel == null ? [] : Array.isArray(sel) ? sel : [sel];
    if (!picked.length) return;
    const existing = new Set(audioPaths);
    const additions = picked.filter((p) => !existing.has(p));
    if (!additions.length) return;
    await replaceAudio([...audioPaths, ...additions], null);
  }

  async function removeAndReplace(path: string) {
    if (busyReplace) return;
    const next = audioPaths.filter((q) => q !== path);
    await replaceAudio(next, null);
  }

  async function clearAndReplace() {
    if (busyReplace) return;
    await replaceAudio([], null);
  }

  async function expandFolderDrop(path: string): Promise<boolean> {
    const ext = extOf(path);
    if (ext) return false;
    const res = await commands.cmdExpandAudioDir(path);
    if (res.status !== "ok" || res.data.length === 0) return false;
    const existing = new Set(audioPaths);
    const additions = res.data.filter((p) => !existing.has(p));
    if (!additions.length) return true;
    await replaceAudio([...audioPaths, ...additions], basename(path));
    return true;
  }

  async function handleAudioDrop(paths: string[]) {
    if (busyReplace) return;
    const leftover: string[] = [];
    for (const p of paths) {
      const ext = extOf(p);
      if (!ext) {
        const expanded = await expandFolderDrop(p);
        if (!expanded) leftover.push(p);
      } else if (AUDIO_EXTS.includes(ext)) {
        leftover.push(p);
      }
    }
    if (!leftover.length) return;
    const existing = new Set(audioPaths);
    const additions = leftover.filter((p) => !existing.has(p));
    if (!additions.length) return;
    await replaceAudio([...audioPaths, ...additions], null);
  }

  function hitTestAudio(clientX: number, clientY: number): boolean {
    const el = audioDropEl;
    if (!el) return false;
    const r = el.getBoundingClientRect();
    return (
      clientX >= r.left &&
      clientX <= r.right &&
      clientY >= r.top &&
      clientY <= r.bottom
    );
  }

  function resolveAudioHover(x: number, y: number): boolean {
    if (hitTestAudio(x, y)) return true;
    const dpr = window.devicePixelRatio || 1;
    if (dpr !== 1) return hitTestAudio(x / dpr, y / dpr);
    return false;
  }

  // Drag-drop wiring is gated on expandOpen && receiptCount === 0 — attach
  // when the affordance is live and tear down when it folds away.
  $effect(() => {
    if (!expandOpen || receiptCount > 0) return;
    let disposed = false;
    let off: UnlistenFn | undefined;
    void (async () => {
      const handler = await getCurrentWebview().onDragDropEvent((event) => {
        if (busyReplace) return;
        const p = event.payload;
        if (p.type === "over") {
          localHover = resolveAudioHover(p.position.x, p.position.y);
        } else if (p.type === "leave") {
          localHover = false;
        } else if (p.type === "drop") {
          const onZone = resolveAudioHover(p.position.x, p.position.y);
          localHover = false;
          if (onZone) void handleAudioDrop(p.paths);
        }
      });
      if (disposed) {
        handler();
        return;
      }
      off = handler;
    })();
    return () => {
      disposed = true;
      off?.();
      localHover = false;
    };
  });
</script>

<div class="flex h-full min-h-screen">
  <section class="mx-auto max-w-3xl flex-1 space-y-6 px-8 pt-6">
    {#if mapping.mappingState}
      <header class="flex items-baseline justify-between gap-3">
        <h1 class="text-lg font-semibold text-fg">
          Confirm chapter ↔ track pairing
        </h1>
        <div class="flex items-center gap-2">
          <div data-testid="strategy-toggle" class="flex gap-1">
            <button type="button" data-testid="strategy-split"
                    class="rounded-sm border px-2 py-1 text-xs {strategy === 'split_proportional' ? 'border-accent bg-accent-soft text-accent' : 'border-border text-fg-muted'}"
                    onclick={() => setStrategy('split_proportional')}>Split proportionally</button>
            <button type="button" data-testid="strategy-single"
                    class="rounded-sm border px-2 py-1 text-xs {strategy === 'single_lesson' ? 'border-accent bg-accent-soft text-accent' : 'border-border text-fg-muted'}"
                    onclick={() => setStrategy('single_lesson')}>One lesson</button>
          </div>
          {#if matterIds.length > 0}
            <button
              type="button"
              data-testid="skip-matter-chip"
              class="rounded-sm border border-border bg-surface px-2 py-1 text-xs text-fg-muted hover:bg-surface-sunken hover:text-fg"
              onclick={toggleMatter}
            >
              {allMatterSkipped ? "Restore front & back matter" : "Remove front & back matter"}
            </button>
          {/if}
          {#if projectIdValue}
            <button
              type="button"
              class="rounded-sm border border-border bg-surface px-2 py-1 text-xs text-fg hover:bg-surface-sunken"
              aria-expanded={settingsOpen}
              aria-controls="project-settings-panel"
              onclick={() => (settingsOpen = !settingsOpen)}
              data-testid="project-settings-toggle"
            >
              Project settings {settingsOpen ? "▲" : "▼"}
            </button>
          {/if}
        </div>
      </header>
      {#if settingsOpen && projectIdValue}
        <div id="project-settings-panel" data-testid="project-settings-panel">
          <ProjectSettings projectId={projectIdValue} bind:absorbPolicy />
        </div>
      {/if}
      <div class="match-body">
        <div class="min-w-0 flex-1">
        <MappingGrid
          chapters={mapping.chapters}
          mappingState={mapping.mappingState}
          buckets={mapping.buckets}
          skippedIds={mapping.skippedIds}
          lastSavedAt={mapping.lastSavedAt}
          saving={mapping.saving}
          canContinue={mappingGateOk}
          onOp={handleMappingOp}
          onConfirmPair={handleConfirmPair}
          onRemove={(id) => mapping.removeChapter(id)}
          onUndoRemove={() => mapping.setSkipped(mapping.skippedIds.slice(0, -1))}
          onContinue={handleMappingContinue}
        />
        </div>
        <ChapterInspector />
      </div>
    {:else}
      <header>
        <h1 class="text-lg font-semibold text-fg">Resolve mismatch</h1>
      </header>

      {#if hydrating}
        <p class="text-sm text-fg-muted">Re-probing project sources…</p>
      {:else}
        {#if receiptCount === 0 && error === null}
          <div class="space-y-2">
            <button
              type="button"
              class="text-xs text-fg-muted hover:text-fg"
              aria-expanded={expandOpen}
              aria-controls="add-more-audio-panel"
              onclick={() => (expandOpen = !expandOpen)}
            >
              {expandOpen ? "− Hide audio" : "+ Add more audio"}
            </button>
            {#if expandOpen}
              <div
                id="add-more-audio-panel"
                class="space-y-2 rounded-md border border-border bg-surface p-3"
              >
                <DropZone
                  variant="audio"
                  paths={audioPaths}
                  hovered={localHover}
                  disabled={busyReplace}
                  onPick={pickAndReplace}
                  onRemove={removeAndReplace}
                  onClear={clearAndReplace}
                  originLabel={audioOriginFolder ?? undefined}
                  ref={(el) => (audioDropEl = el)}
                />
                <div class="flex items-center justify-between gap-3">
                  <p class="text-xs text-fg-subtle">
                    Changes save as you add or remove files.
                  </p>
                  <button
                    type="button"
                    class="rounded-sm px-2 py-1 text-xs text-fg-muted hover:bg-surface-sunken hover:text-fg"
                    onclick={() => (expandOpen = false)}
                  >
                    Cancel
                  </button>
                </div>
                {#if replaceError}
                  <p
                    class="rounded-sm border border-error-soft bg-error-soft/30 px-3 py-2 text-xs text-fg"
                  >
                    {replaceError}
                  </p>
                {/if}
              </div>
            {/if}
          </div>
        {/if}

        <MismatchEvidence {title} {chapters} {tracks} {condition} />

        <div class="space-y-2">
          {#each options.filter((o) => o !== "cancel") as opt (opt)}
            <ResponseCard
              response={opt}
              selected={selected === opt}
              onSelect={() => (selected = opt)}
            />
          {/each}
        </div>
      {/if}

      {#if selected === "split_proportional" && bucketPreview && bucketPreview.length > 0}
        <div class="rounded-md border border-border bg-surface p-4">
          <p class="text-sm font-medium text-fg">Proposed split</p>
          <p class="mt-1 text-xs text-fg-muted">
            Text chapters are grouped proportionally by audio chapter duration.
          </p>
          <ul class="mt-3 space-y-1 text-sm text-fg">
            {#each bucketPreview as row, i (i)}
              {@const label = row.atomTitle ?? `Atom ${i + 1}`}
              {@const drift = isDrifting(row)}
              <li class="flex items-baseline gap-2 tabular">
                <span>
                  Ch {row.textRangeStart + 1}–{row.textRangeEnd} → {label} ({formatDuration(
                    row.atomDurationSec,
                  )})
                </span>
                <span class="text-xs text-fg-muted">
                  chars/sec: {row.charsPerSec.toFixed(1)}
                </span>
                {#if drift}
                  <span
                    class="inline-flex h-4 w-4 items-center justify-center rounded-full bg-warning/10 text-[10px] font-semibold text-warning"
                    title="This bucket's chars/sec deviates more than ±30% from the median — the narrator may have skipped or added material at this boundary."
                    aria-label="Drift warning"
                  >
                    i
                  </span>
                {/if}
              </li>
            {/each}
          </ul>
        </div>
      {/if}

      {#if error}
        <p
          class="rounded-sm border border-error-soft bg-error-soft/30 px-4 py-2 text-sm"
        >
          {error}
        </p>
      {/if}

      <div class="flex justify-end gap-2">
        <a
          href="/library"
          class="rounded-sm border border-border bg-surface px-3 py-1.5 text-sm font-medium text-fg hover:bg-surface-sunken"
        >
          Back
        </a>
        <button
          type="button"
          onclick={confirm}
          disabled={busy || hydrating}
          class="rounded-sm bg-accent px-3 py-1.5 text-sm font-medium text-white hover:bg-accent-hover disabled:bg-fg-subtle"
        >
          Confirm
        </button>
      </div>
    {/if}
  </section>
</div>

<style>
  .match-body {
    display: flex;
    align-items: flex-start;
    min-height: 0;
  }
</style>
