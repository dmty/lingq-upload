<script lang="ts">
  import { goto } from "$app/navigation";
  import { page } from "$app/state";
  import {
    commands,
    type BucketPreview,
    type MismatchCondition,
    type MismatchResponse,
  } from "$lib/ipc/bindings";
  import { appErrorMessage } from "$lib/errors";
  import MismatchEvidence from "$lib/components/MismatchEvidence.svelte";
  import ResponseCard from "$lib/components/ResponseCard.svelte";

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

  async function hydrateFromBackend() {
    if (!projectKey) {
      hydrating = false;
      return;
    }
    const loaded = await commands.cmdProjectLoad(projectKey);
    if (loaded.status === "error") {
      error = appErrorMessage(loaded.error);
      hydrating = false;
      return;
    }
    const inspected = await commands.cmdMatcherInspect(loaded.data.id);
    if (inspected.status === "error") {
      error = appErrorMessage(inspected.error);
      hydrating = false;
      return;
    }
    if (inspected.data == null) {
      goto(`/run/${projectKey}`);
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

  $effect(() => {
    if (!applyParams()) {
      void hydrateFromBackend();
    } else {
      hydrating = false;
    }
  });

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

  async function confirm() {
    busy = true;
    error = null;
    if (selected === "cancel") {
      goto("/library");
      return;
    }
    const loaded = await commands.cmdProjectLoad(projectKey);
    if (loaded.status === "error") {
      error = appErrorMessage(loaded.error);
      busy = false;
      return;
    }
    const resolved = await commands.cmdMatcherResolve(
      loaded.data.id,
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

<section class="mx-auto max-w-2xl space-y-6 pt-6">
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
</section>
