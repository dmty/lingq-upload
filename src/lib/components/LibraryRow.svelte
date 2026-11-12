<script lang="ts">
  import { fade } from "svelte/transition";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { commands, type LibraryEntry } from "$lib/ipc/bindings";
  import { appErrorMessage } from "$lib/errors";
  import { formatRelative } from "$lib/format";
  import { lingqCollectionUrl } from "$lib/lingq";
  import { primaryActionFor } from "$lib/library-actions";
  import { statusLabel } from "$lib/status-labels";
  import CoverThumb from "./CoverThumb.svelte";
  import StatusBadge from "./StatusBadge.svelte";
  import Button from "$lib/components/Button.svelte";
  import { course } from "$lib/stores/course.svelte";

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

  // One meta line, so the author and the state share it. The language lives in
  // the sidebar filter and the LingQ course id on the course page; neither
  // earns a place here.
  const metaLine = $derived.by(() => {
    const left = authors.join(" · ");
    const seriesTail = series
      ? series.index == null
        ? series.name
        : `${series.name} #${Math.trunc(series.index)}`
      : "";
    const state = (() => {
      switch (status) {
        case "done":
          return `${entry.completed_lesson_count} lessons`;
        case "running":
          return "uploading…";
        case "paused":
          return `${entry.completed_lesson_count} of ${entry.receipt_count} uploaded`;
        case "needs_match":
          return `${entry.receipt_count} chapters`;
        case "failed":
          return failedReason ?? "upload stopped";
        case "idle":
        default:
          return entry.mtime ? `added ${formatRelative(entry.mtime)}` : "";
      }
    })();
    return [left || seriesTail, state].filter(Boolean).join(" · ");
  });

  const statusHumanLabel = $derived(statusLabel(status));

  const remoteCover = $derived.by(() => {
    if (entry.cover_path) return null;
    const cid = entry.lingq_collection_id;
    if (cid == null) return null;
    return course.entry(entry.language, cid).view?.collection.image_url ?? null;
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

  // Right-click opens the real NSMenu. Outside Tauri (browser, e2e) there is
  // no menu to open, and the keyboard path (↩ opens, ⌫ trashes) still covers
  // every item in it.
  async function openContextMenu(event: MouseEvent) {
    event.preventDefault();
    const collectionId = entry.lingq_collection_id;
    try {
      const { Menu } = await import("@tauri-apps/api/menu");
      const menu = await Menu.new({
        items: [
          {
            id: "primary",
            text: action.label,
            enabled: !action.disabled,
            action: () => onPrimary(entry),
          },
          ...(collectionId != null
            ? [
                {
                  id: "lingq",
                  text: "Open in LingQ",
                  action: () =>
                    void openUrl(
                      lingqCollectionUrl(entry.language, collectionId),
                    ),
                },
              ]
            : []),
          { item: "Separator" as const },
          { id: "trash", text: "Move to Trash", action: startConfirm },
        ],
      });
      await menu.popup();
    } catch {
      // No native menu here; the keyboard shortcuts remain the way in.
    }
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
    class="col-span-2 flex min-h-[64px] flex-col justify-center gap-1 py-2"
    transition:fade={{ duration: 200 }}
  >
    <div class="flex items-center justify-between gap-3">
      <p class="text-base text-fg-muted">
        Move <span class="font-medium text-fg">"{entry.title}"</span> to trash?
      </p>
      <div class="flex items-center gap-2">
        <Button variant="secondary" onclick={cancelConfirm}>Cancel</Button>
        <Button
          variant="danger"
          disabled={trashBusy}
          onclick={handleConfirmTrash}
        >
          {trashBusy ? "Moving…" : "Move to Trash"}
        </Button>
      </div>
    </div>
    {#if trashError}
      <p class="text-xs text-error">{trashError}</p>
    {/if}
  </div>
{:else}
  <button
    type="button"
    class="grid min-h-[64px] w-full grid-cols-[48px_1fr] items-center gap-3 py-2 text-left"
    aria-label={`Open "${entry.title}" — ${statusHumanLabel}`}
    disabled={rowDisabled}
    title={rowDisabled ? "No LingQ collection id" : undefined}
    onclick={handleRow}
    oncontextmenu={openContextMenu}
  >
    <CoverThumb
      coverPath={entry.cover_path ?? null}
      imageUrl={remoteCover}
      title={entry.title}
      class="size-12 object-top text-base"
    />

    <div class="min-w-0">
      <div
        class="truncate text-base text-fg"
        data-testid="library-title"
        title={entry.title}
      >
        {entry.title}
      </div>
      {#if metaLine}
        <div class="truncate text-xs text-fg-muted">{metaLine}</div>
      {/if}
    </div>
  </button>

  <div class="flex items-center justify-end py-1">
    <StatusBadge {status} {failedReason} />
  </div>
{/if}
