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

  type Q = {
    title?: string;
    chapters?: string;
    tracks?: string;
    condition?: MismatchCondition;
    options?: string;
    preselect?: MismatchResponse;
  };

  const params = $derived<Q>(Object.fromEntries(page.url.searchParams));
  const title = $derived(params.title ?? "Untitled");
  const chapters = $derived(Number(params.chapters ?? "0"));
  const tracks = $derived(Number(params.tracks ?? "0"));
  const condition = $derived<MismatchCondition>(
    params.condition ?? "count_off",
  );
  const options = $derived<MismatchResponse[]>(
    (params.options?.split(",") as MismatchResponse[]) ?? ["cancel"],
  );

  // The `NeedsMatch` JobEvent carries `bucket_preview` only for ManyToFew.
  // Run page stashes it in sessionStorage keyed by project id before
  // navigating; URL params can't carry the array cleanly.
  const projectKey = $derived(page.params.projectId ?? "");
  const previewKey = $derived(`bucketPreview:${projectKey}`);
  const bucketPreview = $derived.by<BucketPreview[] | null>(() => {
    if (typeof sessionStorage === "undefined") return null;
    const raw = sessionStorage.getItem(previewKey);
    if (!raw) return null;
    try {
      return JSON.parse(raw) as BucketPreview[];
    } catch {
      return null;
    }
  });

  let selected = $state<MismatchResponse>(
    (page.url.searchParams.get("preselect") as MismatchResponse) ?? "cancel",
  );
  let busy = $state(false);
  let error = $state<string | null>(null);

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
      disabled={busy}
      class="rounded-sm bg-accent px-3 py-1.5 text-sm font-medium text-white hover:bg-accent-hover disabled:bg-fg-subtle"
    >
      Confirm
    </button>
  </div>
</section>
