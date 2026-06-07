<script lang="ts">
  import type { LibraryEntry } from "$lib/ipc/bindings";
  import CoverThumb from "./CoverThumb.svelte";
  import StatusBadge from "./StatusBadge.svelte";

  let {
    entry,
    prev = null,
    next = null,
    onPrimary,
  }: {
    entry: LibraryEntry;
    prev?: LibraryEntry | null;
    next?: LibraryEntry | null;
    onPrimary: (entry: LibraryEntry) => void;
  } = $props();

  const status = $derived(entry.status ?? "idle");
  const authors = $derived(entry.authors ?? []);
  const series = $derived(entry.series ?? null);
  const failedReason = $derived(entry.failed_reason ?? null);

  const connectsTop = $derived(
    series != null && prev?.series?.name === series.name,
  );
  const connectsBottom = $derived(
    series != null && next?.series?.name === series.name,
  );

  const authorLine = $derived.by(() => {
    const left = authors.join(" · ");
    if (series) {
      const tail =
        series.index == null
          ? series.name
          : `${series.name} #${Math.trunc(series.index)}`;
      return left ? `${left} · ${tail}` : tail;
    }
    return left;
  });

  const stateLine = $derived.by(() => {
    const lang = entry.language;
    const collectionTail =
      entry.lingq_collection_id != null
        ? ` · LingQ #${entry.lingq_collection_id}`
        : "";
    switch (status) {
      case "done":
        return `${lang} · ${entry.completed_lesson_count} lessons${collectionTail}`;
      case "running":
        return `${lang} · uploading…`;
      case "paused":
        return `${lang} · ${entry.completed_lesson_count} of ${entry.receipt_count} uploaded`;
      case "needs_match":
        return `${lang} · ${entry.receipt_count} chapters`;
      case "failed":
        return `${lang} · ${failedReason ?? "upload stopped"}`;
      case "idle":
      default:
        return entry.mtime
          ? `${lang} · added ${formatRelative(entry.mtime)}`
          : lang;
    }
  });

  const statusHumanLabel = $derived.by(() => {
    switch (status) {
      case "done":
        return "done";
      case "running":
        return "uploading";
      case "paused":
        return "paused";
      case "needs_match":
        return "needs match";
      case "failed":
        return "failed";
      case "idle":
      default:
        return "idle";
    }
  });

  const primaryLabel = $derived.by(() => {
    switch (status) {
      case "done":
        return "Open";
      case "running":
        return "Watch";
      case "paused":
        return "Resume";
      case "needs_match":
        return "Resolve";
      case "failed":
        return "Retry";
      case "idle":
      default:
        return "Start";
    }
  });

  const rowDisabled = $derived(
    status === "done" && entry.lingq_collection_id == null,
  );
  const primaryDisabled = $derived(rowDisabled);

  function formatRelative(iso: string): string {
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return iso;
    const now = new Date();
    const startOfDay = (x: Date) =>
      new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
    const dayDiff = Math.floor((startOfDay(now) - startOfDay(d)) / 86_400_000);
    if (dayDiff <= 0) return "today";
    if (dayDiff === 1) return "yesterday";
    if (dayDiff < 7) {
      return new Intl.DateTimeFormat(undefined, { weekday: "short" }).format(d);
    }
    if (d.getFullYear() === now.getFullYear()) {
      return new Intl.DateTimeFormat(undefined, {
        month: "short",
        day: "numeric",
      }).format(d);
    }
    return new Intl.DateTimeFormat(undefined, {
      month: "short",
      day: "numeric",
      year: "numeric",
    }).format(d);
  }

  function handleRow() {
    if (rowDisabled) return;
    onPrimary(entry);
  }

  function handlePrimary(e: MouseEvent) {
    e.stopPropagation();
    if (primaryDisabled) return;
    onPrimary(entry);
  }
</script>

{#if connectsTop || connectsBottom}
  <span
    class="pointer-events-none absolute left-0 w-px bg-border {connectsTop &&
    connectsBottom
      ? 'inset-y-0'
      : connectsTop
        ? 'top-0 bottom-1/2'
        : 'top-1/2 bottom-0'}"
    aria-hidden="true"
  ></span>
{/if}

<button
  type="button"
  class="grid grid-cols-[64px_1fr] gap-4 items-center text-left w-full min-h-[88px] py-3 transition-colors hover:bg-surface-sunken disabled:cursor-not-allowed disabled:hover:bg-transparent"
  aria-label={`Open "${entry.title}" — ${statusHumanLabel}`}
  disabled={rowDisabled}
  title={rowDisabled ? "No LingQ collection id" : undefined}
  onclick={handleRow}
>
  <CoverThumb coverPath={entry.cover_path ?? null} title={entry.title} />

  <div class="min-w-0">
    <div class="truncate text-sm font-medium text-fg" title={entry.title}>
      {entry.title}
    </div>
    {#if authorLine}
      <div class="truncate text-xs text-fg-muted">{authorLine}</div>
    {/if}
    <div class="truncate text-xs text-fg-subtle">{stateLine}</div>
  </div>
</button>

<div class="flex h-full flex-col items-end justify-between gap-2 py-3">
  <StatusBadge {status} {failedReason} />
  <div class="flex items-center gap-2 text-xs">
    <button
      type="button"
      class="rounded-sm px-2 py-1 font-medium text-accent hover:bg-accent-soft disabled:cursor-not-allowed disabled:opacity-50"
      disabled={primaryDisabled}
      onclick={handlePrimary}
    >
      {primaryLabel}
    </button>
    <span class="text-fg-subtle">·</span>
    <button
      type="button"
      class="rounded-sm px-2 py-1 text-fg-muted hover:text-fg disabled:cursor-not-allowed disabled:opacity-50"
      aria-label={`Move "${entry.title}" to trash`}
      title="Coming soon"
      disabled
    >
      Trash
    </button>
  </div>
</div>
