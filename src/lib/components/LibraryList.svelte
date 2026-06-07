<script lang="ts">
  import { goto } from "$app/navigation";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import type { LibraryEntry } from "$lib/ipc/bindings";
  import { joinKey } from "$lib/identity";
  import { lingqCollectionUrl } from "$lib/lingq";
  import LibraryRow from "./LibraryRow.svelte";

  let { entries }: { entries: LibraryEntry[] } = $props();

  function primaryActionFor(entry: LibraryEntry) {
    const status = entry.status ?? "idle";
    const key = encodeURIComponent(joinKey(entry.id));
    if (status === "done") {
      if (entry.lingq_collection_id == null) return;
      void openUrl(
        lingqCollectionUrl(entry.language, entry.lingq_collection_id),
      );
      return;
    }
    if (status === "needs_match") {
      void goto(`/match/${key}`);
      return;
    }
    void goto(`/run/${key}`);
  }
</script>

<ul class="divide-y divide-border">
  {#each entries as entry, i (joinKey(entry.id))}
    <li class="relative grid grid-cols-[1fr_auto] gap-4 px-2">
      <LibraryRow
        {entry}
        prev={entries[i - 1] ?? null}
        next={entries[i + 1] ?? null}
        onPrimary={primaryActionFor}
      />
    </li>
  {/each}
</ul>
