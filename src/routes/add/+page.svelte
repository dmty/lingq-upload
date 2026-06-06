<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { open } from "@tauri-apps/plugin-dialog";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import {
    commands,
    type AudioSource,
    type Candidate,
    type TextSource,
  } from "$lib/ipc/bindings";
  import { appErrorMessage } from "$lib/errors";
  import { extOf, filenameStem } from "$lib/paths";
  import { joinKey } from "$lib/identity";
  import SourcePicker from "$lib/components/SourcePicker.svelte";
  import DropZone from "$lib/components/DropZone.svelte";

  type Source = "manual" | "calibre" | "libation";

  let source = $state<Source>("manual");
  let textPath = $state("");
  let audioPath = $state("");
  let lang = $state("");
  let title = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);

  let textDropEl = $state<HTMLButtonElement | null>(null);
  let audioDropEl = $state<HTMLButtonElement | null>(null);
  let hoverZone = $state<"text" | "audio" | null>(null);
  let unlistenDrop: UnlistenFn | undefined;

  const TEXT_EXTS = ["epub", "xhtml", "html", "htm", "txt"];
  const AUDIO_EXTS = ["m4b", "m4a", "mp3"];

  function zoneForExt(ext: string): "text" | "audio" | null {
    if (TEXT_EXTS.includes(ext)) return "text";
    if (AUDIO_EXTS.includes(ext)) return "audio";
    return null;
  }

  function hitTestZone(
    clientX: number,
    clientY: number,
  ): "text" | "audio" | null {
    const inRect = (el: HTMLElement | null) => {
      if (!el) return false;
      const r = el.getBoundingClientRect();
      return (
        clientX >= r.left &&
        clientX <= r.right &&
        clientY >= r.top &&
        clientY <= r.bottom
      );
    };
    if (inRect(textDropEl)) return "text";
    if (inRect(audioDropEl)) return "audio";
    return null;
  }

  function assignToZone(zone: "text" | "audio", paths: string[]) {
    const matching = paths.find((p) => zoneForExt(extOf(p)) === zone);
    const path = matching ?? paths[0];
    if (!path) return;
    if (zone === "text") {
      textPath = path;
      if (!title) title = filenameStem(path);
    } else {
      audioPath = path;
    }
  }

  function assignDropped(paths: string[]) {
    if (!paths.length) return;
    let textCandidate: string | null = null;
    let audioCandidate: string | null = null;
    for (const p of paths) {
      const z = zoneForExt(extOf(p));
      if (z === "text" && !textCandidate) textCandidate = p;
      else if (z === "audio" && !audioCandidate) audioCandidate = p;
    }
    if (textCandidate) {
      textPath = textCandidate;
      if (!title) title = filenameStem(textCandidate);
    }
    if (audioCandidate) audioPath = audioCandidate;
  }

  onMount(() => {
    (async () => {
      unlistenDrop = await getCurrentWebview().onDragDropEvent((event) => {
        if (busy) return;
        const p = event.payload;
        const dpr = window.devicePixelRatio || 1;
        if (p.type === "over") {
          hoverZone = hitTestZone(p.position.x / dpr, p.position.y / dpr);
        } else if (p.type === "leave") {
          hoverZone = null;
        } else if (p.type === "drop") {
          const zone = hitTestZone(p.position.x / dpr, p.position.y / dpr);
          if (zone) assignToZone(zone, p.paths);
          else assignDropped(p.paths);
          hoverZone = null;
        }
      });
    })();
    return () => {
      unlistenDrop?.();
    };
  });

  const canCreate = $derived(
    !!textPath && !!audioPath && !!lang.trim() && !!title.trim() && !busy,
  );

  async function pickText() {
    const sel = await open({
      multiple: false,
      filters: [{ name: "Text", extensions: ["epub", "xhtml", "html", "txt"] }],
    });
    if (typeof sel === "string") {
      textPath = sel;
      if (!title) title = filenameStem(sel);
    }
  }

  async function pickAudio() {
    const sel = await open({
      multiple: false,
      filters: [{ name: "Audio", extensions: ["m4b", "m4a", "mp3"] }],
    });
    if (typeof sel === "string") audioPath = sel;
  }

  function toTextSource(path: string): TextSource {
    return { kind: "epub", value: path } as TextSource;
  }

  function toAudioSource(path: string): AudioSource {
    return { kind: "single_file", value: path } as AudioSource;
  }

  async function onCreate() {
    if (!canCreate) return;
    busy = true;
    error = null;
    const candidate: Candidate = {
      source_id: source,
      title,
      authors: [],
      language: lang,
      series: null,
      cover_path: null,
      text_source: toTextSource(textPath),
      audio_source: toAudioSource(audioPath),
      chapter_manifest: null,
      metadata_extras: {},
    };
    const res = await commands.cmdCreateProject(candidate, lang, title);
    busy = false;
    if (res.status === "error") {
      error = appErrorMessage(res.error);
      return;
    }
    const key = joinKey(res.data);
    goto(`/run/${encodeURIComponent(key)}`);
  }
</script>

<section class="mx-auto max-w-2xl space-y-6 pt-6">
  <header>
    <h1 class="text-lg font-semibold text-fg">Add Project</h1>
    <p class="mt-1 text-sm text-fg-muted">
      Pick a source, a book, and audio. Collection is created automatically.
    </p>
  </header>

  <SourcePicker bind:value={source} />

  {#if source !== "manual"}
    <p
      class="rounded-sm border border-border bg-surface-sunken p-4 text-sm text-fg-muted"
    >
      {source === "calibre" ? "Calibre" : "Libation"} library auto-discovery is staged
      in the backend (see <code>core::library::reconcile</code>). UI picker for
      it ships next sprint — use Manual for now.
    </p>
  {/if}

  <fieldset class="space-y-2">
    <legend class="text-xs font-medium uppercase tracking-wide text-fg-muted">
      Book (EPUB or HTML)
    </legend>
    <DropZone
      variant="text"
      path={textPath}
      hovered={hoverZone === "text"}
      disabled={busy}
      onPick={pickText}
      onClear={() => (textPath = "")}
      ref={(el) => (textDropEl = el)}
    />
  </fieldset>

  <fieldset class="space-y-2">
    <legend class="text-xs font-medium uppercase tracking-wide text-fg-muted">
      Audio
    </legend>
    <DropZone
      variant="audio"
      path={audioPath}
      hovered={hoverZone === "audio"}
      disabled={busy}
      onPick={pickAudio}
      onClear={() => (audioPath = "")}
      ref={(el) => (audioDropEl = el)}
    />
  </fieldset>

  <div class="grid grid-cols-2 gap-3">
    <label class="space-y-1">
      <span class="text-xs font-medium uppercase tracking-wide text-fg-muted">
        Title
      </span>
      <input
        type="text"
        bind:value={title}
        class="w-full rounded-sm border border-border bg-surface px-3 py-1.5 text-sm"
        disabled={busy}
      />
    </label>
    <label class="space-y-1">
      <span class="text-xs font-medium uppercase tracking-wide text-fg-muted">
        Language
      </span>
      <input
        type="text"
        bind:value={lang}
        placeholder="ja, en, …"
        class="w-full rounded-sm border border-border bg-surface px-3 py-1.5 text-sm"
        disabled={busy}
      />
    </label>
  </div>

  {#if error}
    <p
      class="rounded-sm border border-error-soft bg-error-soft/30 px-4 py-2 text-sm text-fg"
    >
      {error}
    </p>
  {/if}

  <div class="flex justify-end">
    <button
      type="button"
      disabled={!canCreate}
      onclick={onCreate}
      class="rounded-sm bg-accent px-4 py-2 text-sm font-medium text-white hover:bg-accent-hover disabled:bg-fg-subtle"
    >
      {busy ? "Creating…" : "Create"}
    </button>
  </div>
</section>
