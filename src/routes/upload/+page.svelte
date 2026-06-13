<script lang="ts">
  import { onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import {
    commands,
    type Collection,
    type JobEvent,
    type Stage,
    type UploadResult,
  } from "$lib/ipc/bindings";
  import { appErrorMessage } from "$lib/errors";
  import { extOf, filenameStem } from "$lib/paths";
  import {
    formatLanguageOption,
    languagesStore,
  } from "$lib/stores/languages.svelte";
  import DropZone from "$lib/components/DropZone.svelte";
  import ProgressPanel from "$lib/components/ProgressPanel.svelte";
  import ResultPanel from "$lib/components/ResultPanel.svelte";

  type ProgressEntry = {
    stage: Stage["kind"];
    pct: number;
    message: string | null;
  };

  let textPath = $state<string>("");
  let audioPath = $state<string>("");
  let lang = $state<string>("");
  let collectionIdRaw = $state<string>("");
  let title = $state<string>("");
  let titleEdited = $state(false);

  let busy = $state(false);
  let progress = $state<ProgressEntry[]>([]);
  let currentStage = $state<string | null>(null);
  let error = $state<string | null>(null);
  let result = $state<UploadResult | null>(null);

  const languages = $derived(languagesStore.languages);
  const languagesError = $derived(
    languages.length === 0 ? languagesStore.error : null,
  );
  let showAllLanguages = $state(false);
  let collections = $state<Collection[]>([]);
  let collectionsError = $state<string | null>(null);
  let loadingCollections = $state(false);

  let unlisten: UnlistenFn | undefined;
  let unlistenDrop: UnlistenFn | undefined;

  let textDropEl = $state<HTMLButtonElement | null>(null);
  let audioDropEl = $state<HTMLButtonElement | null>(null);
  let hoverZone = $state<"text" | "audio" | null>(null);

  const TEXT_EXTS = ["xhtml", "html", "htm", "txt"];
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

  function assignDropped(paths: string[]) {
    if (!paths.length) return;
    let textCandidate: string | null = null;
    let audioCandidate: string | null = null;
    for (const p of paths) {
      const z = zoneForExt(extOf(p));
      if (z === "text" && !textCandidate) textCandidate = p;
      else if (z === "audio" && !audioCandidate) audioCandidate = p;
    }
    if (textCandidate) textPath = textCandidate;
    if (audioCandidate) {
      audioPath = audioCandidate;
      if (!titleEdited) title = filenameStem(audioCandidate);
    }
  }

  function assignToZone(zone: "text" | "audio", paths: string[]) {
    const matching = paths.find((p) => zoneForExt(extOf(p)) === zone);
    const path = matching ?? paths[0];
    if (!path) return;
    if (zone === "text") {
      textPath = path;
    } else {
      audioPath = path;
      if (!titleEdited) title = filenameStem(path);
    }
  }

  const visibleLanguages = $derived(
    showAllLanguages ? languages : languages.filter((l) => l.known_words > 0),
  );

  const collectionId = $derived(Number.parseInt(collectionIdRaw, 10));

  const collectionPicked = $derived(
    Number.isFinite(collectionId) && collectionId > 0,
  );

  const canSubmit = $derived(
    !busy &&
      textPath.length > 0 &&
      audioPath.length > 0 &&
      lang.trim().length > 0 &&
      title.trim().length > 0 &&
      collectionPicked,
  );

  // Latest progress percent for the bar (always reflects most recent emit).
  const livePct = $derived(progress.at(-1)?.pct ?? 0);
  const liveMessage = $derived(progress.at(-1)?.message ?? null);

  const submitLabel = $derived.by(() => {
    if (busy) return "Uploading…";
    if (!lang.trim()) return "Choose a language to continue";
    if (!collectionPicked) return "Pick a collection";
    if (!textPath) return "Add the chapter text";
    if (!audioPath) return "Add the audio";
    if (!title.trim()) return "Name the lesson";
    return "Upload lesson";
  });

  function stageLabel(stage: Stage["kind"]): string {
    switch (stage) {
      case "parsing":
        return "Reading text";
      case "transcoding":
        return "Transcoding audio";
      case "uploading":
        return "Uploading to LingQ";
    }
  }

  function handleJobEvent(ev: JobEvent) {
    switch (ev.kind) {
      case "Started":
        currentStage = stageLabel(ev.stage.kind);
        progress = [
          ...progress,
          { stage: ev.stage.kind, pct: 0, message: null },
        ];
        break;
      case "Progress":
        progress = [
          ...progress,
          {
            stage: (progress.at(-1)?.stage ?? "parsing") as Stage["kind"],
            pct: ev.pct,
            message: ev.message,
          },
        ];
        break;
      case "Result":
        currentStage = ev.ok ? "Done" : "Failed";
        break;
      case "Log":
      case "Cancelled":
        break;
    }
  }

  async function loadCollections(forLang: string) {
    if (!forLang) {
      collections = [];
      return;
    }
    loadingCollections = true;
    collectionsError = null;
    collections = [];
    collectionIdRaw = "";
    const res = await commands.cmdListCollections(forLang);
    if (res.status === "ok") {
      collections = res.data;
    } else {
      collectionsError = appErrorMessage(res.error);
    }
    loadingCollections = false;
  }

  function onLanguageChange() {
    void loadCollections(lang);
  }

  onMount(() => {
    (async () => {
      unlisten = await listen<JobEvent>("job", (event) => {
        handleJobEvent(event.payload);
      });
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
      void languagesStore.ensureLoaded();
    })();
    return () => {
      unlisten?.();
      unlistenDrop?.();
    };
  });

  async function pick(kind: "text" | "audio") {
    const sel = await open({
      multiple: false,
      filters:
        kind === "text"
          ? [
              {
                name: "Chapter text",
                extensions: ["xhtml", "html", "htm", "txt"],
              },
            ]
          : [{ name: "Audio", extensions: ["m4b", "m4a", "mp3"] }],
    });
    if (typeof sel === "string") {
      if (kind === "text") {
        textPath = sel;
      } else {
        audioPath = sel;
        if (!titleEdited) {
          title = filenameStem(sel);
        }
      }
    }
  }

  function clearFile(kind: "text" | "audio") {
    if (kind === "text") textPath = "";
    else audioPath = "";
  }

  function uploadAnother() {
    // Sticky destination: keep lang + collection for the next upload.
    textPath = "";
    audioPath = "";
    title = "";
    titleEdited = false;
    progress = [];
    currentStage = null;
    result = null;
    error = null;
  }

  async function upload() {
    if (!canSubmit) return;
    busy = true;
    error = null;
    result = null;
    progress = [];
    currentStage = null;

    const built = await commands.manualSourceFromFiles(
      textPath,
      audioPath,
      lang,
      title,
    );
    if (built.status !== "ok") {
      error = appErrorMessage(built.error);
      busy = false;
      return;
    }

    const res = await commands.uploadOneShot(built.data, collectionId, lang);
    if (res.status === "ok") {
      result = res.data;
    } else {
      error = appErrorMessage(res.error);
    }
    busy = false;
  }
</script>

<section class="mx-auto max-w-180 pt-6">
  <h1 class="text-xl font-semibold text-fg">New lesson</h1>
  <p class="mt-2 text-base text-fg-muted">
    Pick a destination, then drop in your text and audio.
  </p>

  <div
    class="mt-8 rounded-md border border-border bg-surface shadow-(--shadow-card)"
  >
    {#if result}
      <ResultPanel {title} {result} onUploadAnother={uploadAnother} />
    {:else if busy || progress.length > 0}
      <ProgressPanel stage={currentStage} pct={livePct} message={liveMessage} />
    {:else}
      <!-- Destination -->
      <div class="p-6">
        <h2
          class="text-xs font-medium tracking-[0.04em] text-fg-muted uppercase"
        >
          Destination
        </h2>
        <div class="mt-4 grid grid-cols-2 gap-4">
          <label class="block">
            <span class="text-sm text-fg-muted">Language</span>
            <select
              bind:value={lang}
              onchange={onLanguageChange}
              disabled={busy || visibleLanguages.length === 0}
              class="mt-1.5 h-10 w-full rounded-sm border border-border bg-surface px-3 text-base text-fg outline-none transition-[box-shadow,border-color] duration-180 ease-snappy focus:border-accent focus:shadow-[0_0_0_3px_var(--color-accent-soft)] disabled:bg-surface-sunken disabled:text-fg-subtle"
            >
              <option value="" disabled>
                {languagesError
                  ? "Could not load languages"
                  : "Select language…"}
              </option>
              {#each visibleLanguages as l (l.code)}
                <option value={l.code}>{formatLanguageOption(l)}</option>
              {/each}
            </select>
            {#if languagesError}
              <span class="mt-1 block text-xs text-error">
                {languagesError}
              </span>
            {:else if languages.length > 0}
              <label
                class="mt-1.5 flex cursor-pointer items-center gap-1.5 text-xs text-fg-muted"
              >
                <input
                  type="checkbox"
                  bind:checked={showAllLanguages}
                  class="h-3.5 w-3.5 accent-accent"
                />
                Show all LingQ languages
              </label>
            {/if}
          </label>

          <label class="block">
            <span class="text-sm text-fg-muted">Collection</span>
            <select
              bind:value={collectionIdRaw}
              disabled={busy || loadingCollections || collections.length === 0}
              class="mt-1.5 h-10 w-full rounded-sm border border-border bg-surface px-3 text-base text-fg outline-none transition-[box-shadow,border-color] duration-180 ease-snappy focus:border-accent focus:shadow-[0_0_0_3px_var(--color-accent-soft)] disabled:bg-surface-sunken disabled:text-fg-subtle"
            >
              <option value="" disabled>
                {#if !lang}
                  Pick a language first
                {:else if loadingCollections}
                  Loading…
                {:else if collectionsError}
                  Could not load collections
                {:else if collections.length === 0}
                  No collections in this language
                {:else}
                  Select collection…
                {/if}
              </option>
              {#each collections as c (c.id)}
                <option value={String(c.id)}>{c.title}</option>
              {/each}
            </select>
            {#if collectionsError}
              <span class="mt-1 block text-xs text-error">
                {collectionsError}
              </span>
            {/if}
          </label>
        </div>
      </div>

      <!-- Content -->
      <div class="border-t border-border p-6">
        <h2
          class="text-xs font-medium tracking-[0.04em] text-fg-muted uppercase"
        >
          Content
        </h2>

        <label class="mt-4 block">
          <span class="text-sm text-fg-muted">Lesson title</span>
          <input
            type="text"
            bind:value={title}
            oninput={() => (titleEdited = true)}
            disabled={busy}
            placeholder="Auto-fills from audio filename"
            class="mt-1.5 h-10 w-full rounded-sm border border-border bg-surface px-3 text-base text-fg outline-none transition-[box-shadow,border-color] duration-180 ease-snappy placeholder:text-fg-subtle focus:border-accent focus:shadow-[0_0_0_3px_var(--color-accent-soft)] disabled:bg-surface-sunken"
          />
          {#if !titleEdited && audioPath}
            <span class="mt-1 block text-xs text-fg-subtle">
              Auto-filled from audio · edit freely
            </span>
          {/if}
        </label>

        <div class="mt-4 grid gap-3">
          <DropZone
            variant="text"
            paths={textPath ? [textPath] : []}
            hovered={hoverZone === "text"}
            disabled={busy}
            onPick={() => pick("text")}
            onClear={() => clearFile("text")}
            ref={(el) => (textDropEl = el)}
          />
          <DropZone
            variant="audio"
            paths={audioPath ? [audioPath] : []}
            hovered={hoverZone === "audio"}
            disabled={busy}
            onPick={() => pick("audio")}
            onClear={() => clearFile("audio")}
            ref={(el) => (audioDropEl = el)}
          />
        </div>

        {#if error}
          <div
            role="alert"
            class="mt-4 rounded-sm border-l-[3px] border-error bg-error-soft p-3 text-sm text-error"
          >
            {error}
          </div>
        {/if}

        <button
          type="button"
          onclick={upload}
          disabled={!canSubmit}
          class="mt-6 inline-flex h-12 w-full items-center justify-center gap-2 rounded-sm bg-accent text-base font-medium text-white transition-colors duration-180 ease-snappy hover:bg-accent-hover disabled:cursor-not-allowed disabled:bg-surface-sunken disabled:text-fg-subtle"
        >
          {submitLabel}
        </button>
      </div>
    {/if}
  </div>
</section>
