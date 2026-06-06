<script lang="ts">
  import { goto } from "$app/navigation";
  import type { LibraryEntry } from "$lib/ipc/bindings";

  let { entries }: { entries: LibraryEntry[] } = $props();

  function joinKey(entry: LibraryEntry): string {
    const id = entry.id;
    if (id.audible_asin) return `asin:${id.audible_asin}`;
    if (id.isbn13) return `isbn13:${id.isbn13}`;
    if (id.calibre_uuid) return `uuid:${id.calibre_uuid}`;
    return `ch:${id.content_hash}`;
  }

  function open(entry: LibraryEntry) {
    const key = joinKey(entry);
    if (entry.receipt_count > 0) {
      goto(`/run/${encodeURIComponent(key)}`);
    } else {
      goto(`/add?prefill=${encodeURIComponent(key)}`);
    }
  }
</script>

<ul class="divide-y divide-border">
  {#each entries as entry (entry.id.content_hash)}
    <li>
      <button
        type="button"
        class="flex w-full items-center justify-between py-3 px-2 text-left hover:bg-surface-sunken transition-colors duration-120"
        onclick={() => open(entry)}
      >
        <span class="flex flex-col">
          <span class="text-sm font-medium text-fg">{entry.title}</span>
          <span class="text-xs text-fg-muted">{entry.language}</span>
        </span>
        <span class="text-xs text-fg-muted">
          {entry.completed_lesson_count}/{entry.receipt_count}
        </span>
      </button>
    </li>
  {/each}
</ul>
