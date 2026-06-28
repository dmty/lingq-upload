<script lang="ts">
  import { mapping } from "$lib/stores/mapping.svelte";

  let id = $derived(mapping.selectedChapterId);
  let title = $derived(
    id ? (mapping.chapters.find((c) => c.id === id)?.title ?? "") : "",
  );
  let body = $derived(id ? mapping.chapterTextFor(id) : null);
  let audio = $derived(mapping.selectedBucketAudio());

  // The parent bucket this chapter's text rides — the eyebrow connects the
  // text being previewed to the audio segment it will ship with.
  let bucketLabel = $derived.by(() => {
    if (!id || !mapping.mappingState) return "";
    const pair = mapping.mappingState.pairs.find((p) => p.chapter_id === id);
    const buckets = mapping.buckets ?? [];
    const i = buckets.findIndex((b) => b.trackId === pair?.track_id);
    return i >= 0
      ? `Audio ${i + 1} · ${buckets[i].atomTitle ?? ""}`.trim()
      : "";
  });

  let el: HTMLAudioElement | undefined = $state();
  let playing = $state(false);
  let cur = $state(0); // seconds elapsed within the window
  let dur = $derived(audio ? Math.max(0.001, audio.end - audio.start) : 0);
  let frac = $derived(dur > 0 ? Math.min(1, Math.max(0, cur / dur)) : 0);

  // Reset transport whenever the selection changes.
  $effect(() => {
    id;
    if (el) el.pause();
    playing = false;
    cur = 0;
  });

  function snap(tag: string) {
    if (!el) return;
    console.log("inspector audio " + tag, {
      currentSrc: el.currentSrc,
      readyState: el.readyState,
      networkState: el.networkState,
      duration: el.duration,
      currentTime: el.currentTime,
      paused: el.paused,
      muted: el.muted,
      volume: el.volume,
    });
  }
  function toggle() {
    if (!el) return;
    snap("toggle pre");
    if (el.paused) {
      // Seek into the window BEFORE play(); mutating currentTime mid-play
      // aborts the play() promise with AbortError on WebKit.
      if (
        audio &&
        (el.currentTime < audio.start || el.currentTime >= audio.end)
      ) {
        el.currentTime = audio.start;
      }
      el.play().catch((err) => {
        console.warn("inspector audio play() rejected:", err);
      });
    } else el.pause();
  }
  function onMediaError() {
    if (!el) return;
    const e = el.error;
    console.warn("inspector audio error:", {
      code: e?.code,
      message: e?.message,
      networkState: el.networkState,
      readyState: el.readyState,
      src: el.currentSrc,
    });
  }
  function onPlay() {
    playing = true;
  }
  function onPause() {
    playing = false;
  }
  function onTimeUpdate() {
    if (!el || !audio) return;
    if (el.currentTime >= audio.end) {
      el.pause();
      cur = dur;
      return;
    }
    cur = Math.max(0, el.currentTime - audio.start);
  }
  function seek(e: MouseEvent) {
    if (!el || !audio) return;
    const track = e.currentTarget as HTMLElement;
    const ratio = Math.min(1, Math.max(0, e.offsetX / track.clientWidth));
    el.currentTime = audio.start + ratio * dur;
    cur = ratio * dur;
  }
  function nudge(e: KeyboardEvent) {
    if (!el || !audio) return;
    if (e.key === "ArrowRight")
      el.currentTime = Math.min(audio.end, el.currentTime + 5);
    else if (e.key === "ArrowLeft")
      el.currentTime = Math.max(audio.start, el.currentTime - 5);
    else return;
    e.preventDefault();
    cur = Math.max(0, el.currentTime - audio.start);
  }
  function fmt(s: number): string {
    const m = Math.floor(s / 60);
    const r = Math.floor(s % 60);
    return `${m}:${r.toString().padStart(2, "0")}`;
  }
</script>

{#if id}
  <aside
    data-testid="chapter-inspector"
    class="sticky top-4 ml-5 flex max-h-[calc(100vh-2rem)] w-[360px] flex-none flex-col overflow-hidden rounded-lg border border-border bg-surface shadow-card"
  >
    <header class="border-b border-border px-5 pb-4 pt-5">
      {#if bucketLabel}
        <div
          class="mb-1 text-[11px] font-medium uppercase tracking-[0.08em] text-accent"
        >
          {bucketLabel}
        </div>
      {/if}
      <h2 class="text-md font-semibold leading-snug text-fg">{title}</h2>
    </header>

    {#if audio}
      <div
        class="flex items-center gap-3 border-b border-border bg-surface-sunken px-5 py-3"
      >
        <button
          type="button"
          data-testid="inspector-play"
          onclick={toggle}
          aria-label={playing ? "Pause" : "Play"}
          class="grid h-9 w-9 flex-none place-items-center rounded-full bg-accent text-white transition hover:bg-accent-hover active:scale-95"
        >
          {#if playing}
            <svg
              width="13"
              height="13"
              viewBox="0 0 12 12"
              fill="currentColor"
              aria-hidden="true"
            >
              <rect x="2" y="1.5" width="2.6" height="9" rx="0.6" />
              <rect x="7.4" y="1.5" width="2.6" height="9" rx="0.6" />
            </svg>
          {:else}
            <svg
              width="13"
              height="13"
              viewBox="0 0 12 12"
              fill="currentColor"
              aria-hidden="true"
            >
              <path
                d="M3 1.8c0-.6.66-.98 1.18-.66l6 3.7a.78.78 0 0 1 0 1.32l-6 3.7A.78.78 0 0 1 3 9.2z"
              />
            </svg>
          {/if}
        </button>
        <div class="min-w-0 flex-1">
          <div
            class="group relative h-1.5 w-full cursor-pointer rounded-full bg-border-strong/40"
            role="slider"
            tabindex="0"
            aria-label="Seek audio"
            aria-valuemin="0"
            aria-valuemax={Math.round(dur)}
            aria-valuenow={Math.round(cur)}
            onclick={seek}
            onkeydown={nudge}
          >
            <div
              class="absolute inset-y-0 left-0 rounded-full bg-accent"
              style="width: {frac * 100}%"
            ></div>
            <div
              class="absolute top-1/2 h-3 w-3 -translate-x-1/2 -translate-y-1/2 rounded-full bg-accent opacity-0 shadow transition group-hover:opacity-100"
              style="left: {frac * 100}%"
            ></div>
          </div>
          <div
            class="mt-1.5 flex justify-between text-[10px] tabular-nums text-fg-subtle"
          >
            <span>{fmt(cur)}</span>
            <span>{fmt(dur)}</span>
          </div>
        </div>
        <audio
          bind:this={el}
          data-testid="inspector-audio"
          data-window-start={audio.start}
          data-window-end={audio.end}
          class="hidden"
          onplay={onPlay}
          onpause={onPause}
          ontimeupdate={onTimeUpdate}
          onerror={onMediaError}
          onloadedmetadata={() => snap("loadedmetadata")}
          oncanplay={() => snap("canplay")}
          onstalled={() => snap("stalled")}
          onwaiting={() => snap("waiting")}
          onseeked={() => snap("seeked")}
        >
          <source src={audio.src} type={audio.type} />
        </audio>
      </div>
    {/if}

    <div
      data-testid="inspector-text"
      class="inspector-text min-h-0 flex-1 overflow-y-auto whitespace-pre-wrap px-5 py-4 text-[14px] leading-[1.95] text-fg [text-wrap:pretty]"
    >
      {#if body === null}
        <span class="text-fg-subtle italic">Loading…</span>
      {:else}
        {body}
      {/if}
    </div>

    <div class="flex items-center border-t border-border px-5 py-3">
      <button
        type="button"
        data-testid="inspector-remove"
        onclick={() => mapping.removeChapter(id!)}
        class="ml-auto rounded-md px-3 py-1.5 text-xs font-medium text-fg-subtle transition hover:bg-error-soft hover:text-error"
        >Remove chapter</button
      >
    </div>
  </aside>
{/if}

<style>
  /* Soft fade where the scrollable text meets the actions bar. */
  .inspector-text {
    mask-image: linear-gradient(
      to bottom,
      black calc(100% - 1.5rem),
      transparent
    );
  }
</style>
