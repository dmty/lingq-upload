<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import ExternalLinkIcon from "./ExternalLinkIcon.svelte";

  let { url }: { url: string } = $props();
</script>

<!-- Revealed by the enclosing `.group` on hover or selection, like Music's row
     actions. Enter stops here so a page-level list shortcut doesn't also act
     on the selected row. -->
<button
  type="button"
  class="grid size-6 place-items-center rounded-sm text-fg-muted opacity-0 hover:text-fg group-hover:opacity-100 group-aria-selected:opacity-100 focus-visible:opacity-100"
  title="Open in LingQ"
  aria-label="Open in LingQ"
  onclick={() => void openUrl(url)}
  onkeydown={(e) => {
    if (e.key === "Enter") e.stopPropagation();
  }}
>
  <ExternalLinkIcon />
</button>
