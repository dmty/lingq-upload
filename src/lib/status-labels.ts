import type { LibraryStatus } from "$lib/ipc/bindings";

// Single source of truth for status wording so the badge and the row's
// aria-label/state text never drift apart (see StatusBadge, LibraryRow).
export const STATUS_LABELS: Record<LibraryStatus, string> = {
  idle: "Idle",
  done: "Done",
  running: "Uploading",
  paused: "Paused",
  needs_match: "Needs review",
  failed: "Failed",
};

export function statusLabel(status: LibraryStatus): string {
  return STATUS_LABELS[status];
}
