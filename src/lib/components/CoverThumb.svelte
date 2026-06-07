<script lang="ts">
  import { convertFileSrc } from "@tauri-apps/api/core";

  let {
    coverPath,
    title,
  }: {
    coverPath: string | null;
    title: string;
  } = $props();

  let errored = $state(false);

  function firstGrapheme(s: string): string {
    if (!s) return "?";
    const seg = new Intl.Segmenter(undefined, {
      granularity: "grapheme",
    }).segment(s);
    return seg[Symbol.iterator]().next().value?.segment ?? "?";
  }

  const src = $derived(coverPath ? convertFileSrc(coverPath) : null);
  const glyph = $derived(firstGrapheme(title));
  const showImage = $derived(src !== null && !errored);
</script>

{#if showImage}
  <img
    {src}
    alt=""
    loading="lazy"
    class="h-16 w-16 rounded-sm object-cover"
    onerror={() => (errored = true)}
  />
{:else}
  <div
    class="flex h-16 w-16 items-center justify-center rounded-sm bg-accent-soft text-2xl font-semibold text-accent"
    aria-hidden="true"
  >
    {glyph}
  </div>
{/if}
