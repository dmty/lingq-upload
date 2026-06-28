<script lang="ts">
  import type { LibraryEntry, ProjectId } from "$lib/ipc/bindings";
  import { joinKey } from "$lib/identity";
  import { primaryActionFor } from "$lib/library-actions";
  import LibraryRow from "./LibraryRow.svelte";

  let {
    entries,
    focusIndex = null,
    onfocuschange,
    ontrash,
    confirmRequestId = null,
    onconfirmhandled,
  }: {
    entries: LibraryEntry[];
    focusIndex?: number | null;
    onfocuschange?: (i: number) => void;
    ontrash?: (id: ProjectId) => void;
    confirmRequestId?: string | null;
    onconfirmhandled?: () => void;
  } = $props();

  function runPrimary(entry: LibraryEntry, i: number) {
    onfocuschange?.(i);
    primaryActionFor(entry).run();
  }
</script>

<ul class="divide-y divide-border" role="listbox" aria-label="Library">
  {#each entries as entry, i (joinKey(entry.id))}
    {@const focused = focusIndex === i}
    <li
      class="relative grid grid-cols-[1fr_auto] gap-4 px-2"
      class:border-l-2={focused}
      class:border-accent={focused}
      class:bg-accent-soft={focused}
      data-row-index={i}
      role="option"
      aria-selected={focused}
    >
      <LibraryRow
        {entry}
        prev={entries[i - 1] ?? null}
        next={entries[i + 1] ?? null}
        onPrimary={(e) => runPrimary(e, i)}
        ontrash={(e) => ontrash?.(e.id)}
        confirmRequested={confirmRequestId === joinKey(entry.id)}
        {onconfirmhandled}
      />
    </li>
  {/each}
</ul>
