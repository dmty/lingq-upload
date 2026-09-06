<script lang="ts">
  import { convertFileSrc } from "@tauri-apps/api/core";

  let {
    coverPath,
    imageUrl = null,
    title,
    class: cls = "h-16 w-16 text-2xl",
  }: {
    coverPath: string | null;
    imageUrl?: string | null;
    title: string;
    class?: string;
  } = $props();

  let errored = $state(false);

  function firstGrapheme(s: string): string {
    if (!s) return "?";
    const seg = new Intl.Segmenter(undefined, {
      granularity: "grapheme",
    }).segment(s);
    return seg[Symbol.iterator]().next().value?.segment ?? "?";
  }

  const src = $derived(
    imageUrl ? imageUrl : coverPath ? convertFileSrc(coverPath) : null,
  );

  $effect(() => {
    void src;
    errored = false;
  });
  const glyph = $derived(firstGrapheme(title));
  const showImage = $derived(src !== null && !errored);
</script>

{#if showImage}
  <img
    {src}
    alt=""
    loading="lazy"
    class="flex-none rounded-sm object-cover {cls}"
    onerror={() => (errored = true)}
  />
{:else}
  <div
    class="cover-placeholder flex flex-none items-center justify-center rounded-sm bg-accent-soft font-semibold text-accent {cls}"
    aria-hidden="true"
  >
    {glyph}
  </div>
{/if}
