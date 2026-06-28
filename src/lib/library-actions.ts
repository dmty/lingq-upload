import { goto } from "$app/navigation";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { LibraryEntry } from "$lib/ipc/bindings";
import { joinKey } from "$lib/identity";
import { lingqCollectionUrl } from "$lib/lingq";

export type PrimaryAction = {
  label: string;
  run: () => Promise<void> | void;
  disabled: boolean;
};

export function primaryActionFor(entry: LibraryEntry): PrimaryAction {
  const status = entry.status ?? "idle";
  const key = encodeURIComponent(joinKey(entry.id));

  switch (status) {
    case "done": {
      const disabled = entry.lingq_collection_id == null;
      return {
        label: "Open",
        disabled,
        run: () => {
          if (disabled) return;
          void openUrl(
            lingqCollectionUrl(entry.language, entry.lingq_collection_id!),
          );
        },
      };
    }
    case "running":
      return {
        label: "Watch",
        disabled: false,
        run: () => void goto(`/run/${key}`),
      };
    case "paused":
      return {
        label: "Resume",
        disabled: false,
        run: () => void goto(`/run/${key}`),
      };
    case "needs_match":
      return {
        label: "Resolve",
        disabled: false,
        run: () => void goto(`/match/${key}`),
      };
    case "failed":
      return {
        label: "Retry",
        disabled: false,
        run: () => void goto(`/run/${key}`),
      };
    case "idle":
    default:
      return {
        label: "Start",
        disabled: false,
        run: () => void goto(`/run/${key}`),
      };
  }
}
