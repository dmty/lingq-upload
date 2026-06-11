<script lang="ts">
  import { goto } from "$app/navigation";
  import { page } from "$app/state";
  import {
    commands,
    type AbsorbPolicy,
    type BucketPreview,
    type MappingOp,
    type MismatchCondition,
    type MismatchResponse,
    type ProjectId as ProjectIdType,
  } from "$lib/ipc/bindings";
  import { appErrorMessage } from "$lib/errors";
  import MismatchEvidence from "$lib/components/MismatchEvidence.svelte";
  import ResponseCard from "$lib/components/ResponseCard.svelte";
  import ChapterPicker from "$lib/components/ChapterPicker.svelte";
  import MappingGrid from "$lib/components/MappingGrid.svelte";
  import ProjectSettings from "$lib/components/ProjectSettings.svelte";
  import { mapping } from "$lib/stores/mapping.svelte";

  const projectKey = $derived(page.params.projectId ?? "");
  const previewKey = $derived(`bucketPreview:${projectKey}`);

  const pickerRows = $derived(
    mapping.chapters.map((c) => ({
      id: c.id ?? `idx:${c.order}`,
      order: c.order,
      title: c.title,
      kind: c.kind ?? "body",
    })),
  );

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

  // Project-scope settings (absorb policy) live here so the user can adjust
  // them before locking the mapping in. ProjectSettings owns the debounced
  // persistence; the page only feeds the current values once loaded.
  let projectIdValue = $state<ProjectIdType | null>(null);
  let absorbPolicy = $state<AbsorbPolicy>("forward");
  let settingsOpen = $state(false);

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

  function resetProjectState() {
    title = "Untitled";
    chapters = 0;
    tracks = 0;
    condition = "count_off";
    options = ["cancel"];
    bucketPreview = null;
    selected = "cancel";
    hydrating = true;
    busy = false;
    error = null;
    projectIdValue = null;
    absorbPolicy = "forward";
    settingsOpen = false;
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

  // Track rows for MappingGrid. Derived from pair assignments + parking lot.
  // Filename and duration default to the bare id until the audio pipeline
  // threads real metadata through.
  const trackRows = $derived.by(() => {
    const ms = mapping.mappingState;
    if (!ms) return [];
    const ids = new Set<string>();
    for (const p of ms.pairs) if (p.track_id) ids.add(p.track_id);
    for (const t of ms.parking_lot ?? []) ids.add(t);
    return Array.from(ids).map((id) => ({
      id,
      filename: id,
      durationSec: null as number | null,
    }));
  });

  const mappingGateOk = $derived(mapping.gateContinue());

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
    goto(`/run/${projectKey}`);
  }
</script>

<div class="flex h-full min-h-screen">
  <div class="w-72 shrink-0">
    {#if mapping.status === "ready"}
      <ChapterPicker
        chapters={pickerRows}
        skippedIds={mapping.skippedIds}
        revertEpoch={mapping.revertEpoch}
        onChange={(ids) => void mapping.setSkipped(ids)}
        onFlush={() => mapping.flush()}
      />
    {:else}
      <aside class="border-r border-border bg-surface p-3 text-sm text-fg-muted">
        Loading chapters…
      </aside>
    {/if}
  </div>

<section class="mx-auto max-w-3xl flex-1 space-y-6 pt-6">
  {#if mapping.mappingState}
    <header class="flex items-baseline justify-between gap-3">
      <h1 class="text-lg font-semibold text-fg">Confirm chapter ↔ track pairing</h1>
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
    </header>
    {#if settingsOpen && projectIdValue}
      <div id="project-settings-panel" data-testid="project-settings-panel">
        <ProjectSettings
          projectId={projectIdValue}
          bind:absorbPolicy
        />
      </div>
    {/if}
    <MappingGrid
      chapters={mapping.chapters}
      tracks={trackRows}
      mappingState={mapping.mappingState}
      lastSavedAt={mapping.lastSavedAt}
      saving={mapping.saving}
      canContinue={mappingGateOk}
      onOp={handleMappingOp}
      onConfirmPair={handleConfirmPair}
      onContinue={handleMappingContinue}
    />
  {:else}
  <header>
    <h1 class="text-lg font-semibold text-fg">Resolve mismatch</h1>
  </header>

  {#if hydrating}
    <p class="text-sm text-fg-muted">Re-probing project sources…</p>
  {:else}
    <MismatchEvidence {title} {chapters} {tracks} {condition} />

    <div class="space-y-2">
      {#each options as opt (opt)}
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
