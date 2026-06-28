<script lang="ts">
  import { fade } from "svelte/transition";
  import { commands, type LibraryEntry } from "$lib/ipc/bindings";
  import { appErrorMessage } from "$lib/errors";
  import { formatRelative } from "$lib/format";
  import { primaryActionFor } from "$lib/library-actions";
  import CoverThumb from "./CoverThumb.svelte";
  import StatusBadge from "./StatusBadge.svelte";

  let {
    entry,
    prev = null,
    next = null,
    onPrimary,
    ontrash,
    confirmRequested = false,
    onconfirmhandled,
  }: {
    entry: LibraryEntry;
    prev?: LibraryEntry | null;
    next?: LibraryEntry | null;
    onPrimary: (entry: LibraryEntry) => void;
    ontrash?: (entry: LibraryEntry) => void;
    confirmRequested?: boolean;
    onconfirmhandled?: () => void;
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
        return "Needs review";
      case "failed":
        return "failed";
      case "idle":
      default:
        return "idle";
    }
  });

  const action = $derived(primaryActionFor(entry));
  const rowDisabled = $derived(action.disabled);

  let confirming = $state(false);
  let confirmTimer: ReturnType<typeof setTimeout> | null = null;
  let trashError = $state<string | null>(null);
  let trashBusy = $state(false);

  function startConfirm() {
    if (confirmTimer != null) clearTimeout(confirmTimer);
    confirming = true;
    trashError = null;
    confirmTimer = setTimeout(() => {
      confirming = false;
      confirmTimer = null;
    }, 5000);
  }

  function cancelConfirm() {
    if (confirmTimer != null) {
      clearTimeout(confirmTimer);
      confirmTimer = null;
    }
    confirming = false;
    trashError = null;
  }

  $effect(() => {
    if (confirmRequested && !confirming) {
      startConfirm();
      onconfirmhandled?.();
    }
  });

  $effect(() => {
    return () => {
      if (confirmTimer != null) clearTimeout(confirmTimer);
    };
  });

  function handleRow() {
    if (rowDisabled || confirming) return;
    onPrimary(entry);
  }

  function handlePrimary(e: MouseEvent) {
    e.stopPropagation();
    if (rowDisabled) return;
    onPrimary(entry);
  }

  function handleTrashClick(e: MouseEvent) {
    e.stopPropagation();
    startConfirm();
  }

  async function handleConfirmTrash(e: MouseEvent) {
    e.stopPropagation();
    if (trashBusy) return;
    trashBusy = true;
    trashError = null;
    const res = await commands.cmdTrashProject(entry.id);
    trashBusy = false;
    if (res.status === "ok") {
      if (confirmTimer != null) clearTimeout(confirmTimer);
      confirmTimer = null;
      ontrash?.(entry);
    } else {
      trashError = appErrorMessage(res.error);
    }
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

{#if confirming}
  <div
    role="alertdialog"
    aria-live="assertive"
    aria-label={`Confirm move "${entry.title}" to trash`}
    class="col-span-2 flex min-h-[88px] flex-col justify-center gap-1 py-3"
    transition:fade={{ duration: 200 }}
  >
    <div class="flex items-center justify-between gap-3">
      <p class="text-sm text-fg-muted">
        Move <span class="font-medium text-fg">"{entry.title}"</span> to trash?
      </p>
      <div class="flex items-center gap-2">
        <button
          type="button"
          class="rounded-sm px-3 py-1.5 text-sm font-medium text-fg-muted hover:bg-surface-sunken hover:text-fg"
          onclick={(e) => {
            e.stopPropagation();
            cancelConfirm();
          }}
        >
          Cancel
        </button>
        <button
          type="button"
          class="rounded-sm bg-error px-3 py-1.5 text-sm font-medium text-white hover:bg-error/90 disabled:opacity-60"
          disabled={trashBusy}
          onclick={handleConfirmTrash}
        >
          {trashBusy ? "Moving…" : "Move to trash"}
        </button>
      </div>
    </div>
    {#if trashError}
      <p class="text-xs text-error">{trashError}</p>
    {/if}
  </div>
{:else}
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
        disabled={rowDisabled}
        onclick={handlePrimary}
      >
        {action.label}
      </button>
      <span class="text-fg-subtle">·</span>
      <button
        type="button"
        class="rounded-sm px-2 py-1 text-fg-muted hover:text-fg"
        aria-label={`Move "${entry.title}" to trash`}
        onclick={handleTrashClick}
      >
        Trash
      </button>
    </div>
  </div>
{/if}
