<script lang="ts">
  import { onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import {
    commands,
    type AppError,
    type AudioError,
    type Collection,
    type IngestError,
    type JobEvent,
    type Language,
    type LingqError,
    type SecretError,
    type Stage,
    type TextError,
    type UploadResult,
  } from "$lib/ipc/bindings";

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

  let languages = $state<Language[]>([]);
  let languagesError = $state<string | null>(null);
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

  function extOf(path: string): string {
    const dot = path.lastIndexOf(".");
    return dot >= 0 ? path.slice(dot + 1).toLowerCase() : "";
  }

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

  function formatLanguageOption(l: Language): string {
    return l.known_words > 0
      ? `${l.title} (${l.known_words.toLocaleString()})`
      : l.title;
  }

  function filenameStem(path: string): string {
    const sep = path.lastIndexOf("/") >= 0 ? "/" : "\\";
    const base = path.split(sep).pop() ?? path;
    const dot = base.lastIndexOf(".");
    return dot > 0 ? base.slice(0, dot) : base;
  }

  function shortPath(path: string): string {
    if (!path) return "";
    const sep = path.lastIndexOf("/") >= 0 ? "/" : "\\";
    return path.split(sep).pop() ?? path;
  }

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

  function secretMessage(e: SecretError): string {
    switch (e.kind) {
      case "LockedKeychain":
        return "Your OS keychain is locked.";
      case "UserDenied":
        return "Keychain access denied.";
      case "MissingEntry":
        return "No saved API key.";
      case "Backend":
        return `Keychain: ${e.message}`;
    }
  }

  function lingqMessage(e: LingqError): string {
    switch (e.kind) {
      case "Unauthorized":
        return "LingQ rejected the API key.";
      case "NotFound":
        return "LingQ resource not found (check collection ID and language).";
      case "BadRequest":
        return `LingQ bad request: ${e.message}`;
      case "Server":
        return `LingQ server error: ${e.message}`;
      case "Schema":
        return `LingQ response schema: ${e.message}`;
      case "Transport":
        return `Network: ${e.message}`;
      case "Io":
        return `I/O: ${e.message}`;
    }
  }

  function audioMessage(e: AudioError): string {
    switch (e.kind) {
      case "FfmpegNotFound":
        return `ffmpeg not found at ${e.message}`;
      case "FfmpegFailed":
        return `ffmpeg exited ${e.message.status}: ${e.message.stderr}`;
      case "Probe":
        return `ffprobe: ${e.message}`;
      case "DurationMismatch":
        return `Transcode duration mismatch (delta ${e.message.delta_sec}s)`;
      case "Io":
        return `I/O: ${e.message}`;
      case "Cancelled":
        return "Transcode cancelled";
    }
  }

  function textErrorMessage(e: TextError): string {
    return `Text: ${e.message}`;
  }

  function ingestMessage(e: IngestError): string {
    switch (e.kind) {
      case "NotSupported":
        return "This ingest source is not supported.";
      case "Io":
      case "Parse":
      case "Other":
        return `Ingest: ${e.message}`;
    }
  }

  function appErrorMessage(e: AppError): string {
    switch (e.kind) {
      case "Io":
        return `I/O error: ${e.message}`;
      case "Internal":
        return e.message;
      case "Secrets":
        return secretMessage(e.message);
      case "Lingq":
        return lingqMessage(e.message);
      case "Audio":
        return audioMessage(e.message);
      case "Text":
        return textErrorMessage(e.message);
      case "Ingest":
        return ingestMessage(e.message);
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

  async function loadLanguages() {
    languagesError = null;
    let username: string | null = null;
    const profileRes = await commands.cmdAccountProfile();
    if (profileRes.status === "ok") {
      username = profileRes.data.username;
    }
    const res = await commands.cmdListLanguages(username);
    if (res.status === "ok") {
      languages = [...res.data].sort((a, b) => b.known_words - a.known_words);
      if (username) showAllLanguages = false;
    } else {
      languagesError = appErrorMessage(res.error);
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
      await loadLanguages();
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
    // Sally's "sticky destination": keep lang + collection.
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

<section class="mx-auto max-w-180 pt-12">
  <h1 class="text-xl font-semibold text-fg">New lesson</h1>
  <p class="mt-2 text-base text-fg-muted">
    Pick a destination, then drop in your text and audio.
  </p>

  <div
    class="mt-8 rounded-md border border-border bg-surface shadow-(--shadow-card)"
  >
    {#if result}
      <div class="p-6">
        <div class="flex items-start gap-3">
          <span
            class="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-success-soft text-success"
            aria-hidden="true"
          >
            <svg
              width="16"
              height="16"
              viewBox="0 0 20 20"
              fill="none"
              stroke="currentColor"
              stroke-width="2.5"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <path d="M4 10.5l4 4 8-9" />
            </svg>
          </span>
          <div class="flex-1">
            <h2 class="text-md font-semibold text-fg">
              Lesson added to your library.
            </h2>
            <p class="mt-1 text-sm text-fg-muted">
              {title || "Untitled lesson"} · ID
              <code class="tabular text-fg">{result.lesson_id}</code>
            </p>
            <div class="mt-4 flex items-center gap-2">
              <a
                href={result.lesson_url}
                target="_blank"
                rel="noopener noreferrer"
                class="inline-flex h-9 items-center gap-2 rounded-sm bg-accent px-4 text-sm font-medium text-white no-underline transition-colors duration-180 ease-snappy hover:bg-accent-hover hover:no-underline"
              >
                Open in LingQ
                <svg
                  width="13"
                  height="13"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  aria-hidden="true"
                >
                  <path d="M7 17L17 7" />
                  <path d="M8 7h9v9" />
                </svg>
              </a>
              <button
                type="button"
                onclick={uploadAnother}
                class="rounded-sm px-3 py-2 text-sm font-medium text-fg-muted transition-colors duration-120 hover:bg-surface-sunken hover:text-fg"
              >
                Upload another
              </button>
            </div>
          </div>
        </div>
      </div>
    {:else if busy || progress.length > 0}
      <div class="p-6">
        <div class="flex items-baseline justify-between">
          <h2 class="text-lg font-semibold text-fg">
            {currentStage ?? "Working…"}
          </h2>
          <span class="tabular text-sm text-fg-muted">
            {Math.round(livePct * 100)}%
          </span>
        </div>
        <div
          class="mt-3 h-1.5 w-full overflow-hidden rounded-full bg-surface-sunken"
          aria-live="polite"
        >
          <div
            class="h-full rounded-full bg-accent transition-[width] duration-180 ease-snappy"
            style:width="{Math.max(2, livePct * 100)}%"
          ></div>
        </div>
        {#if liveMessage}
          <p class="mt-2 text-sm text-fg-muted">{liveMessage}</p>
        {/if}
      </div>
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
          <button
            type="button"
            bind:this={textDropEl}
            onclick={() => pick("text")}
            disabled={busy}
            class="group flex items-center gap-3 rounded-md border-[1.5px] border-dashed px-4 py-5 text-left transition-[background,border-color] duration-120 {hoverZone ===
            'text'
              ? 'border-accent bg-accent-soft'
              : textPath
                ? 'border-success bg-success-soft'
                : 'border-border-strong bg-surface hover:border-accent hover:bg-accent-soft'}"
          >
            <svg
              width="22"
              height="22"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.5"
              stroke-linecap="round"
              stroke-linejoin="round"
              class={textPath ? "text-success" : "text-fg-muted"}
              aria-hidden="true"
            >
              <path
                d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"
              />
              <polyline points="14 2 14 8 20 8" />
              <line x1="9" y1="14" x2="15" y2="14" />
              <line x1="9" y1="18" x2="13" y2="18" />
            </svg>
            <div class="flex-1">
              {#if textPath}
                <div class="text-sm font-medium text-fg">
                  {shortPath(textPath)}
                </div>
                <div class="text-xs text-fg-subtle">
                  Click to choose a different file
                </div>
              {:else}
                <div class="text-sm font-medium text-fg">
                  Drop chapter text or click to choose
                </div>
                <div class="text-xs text-fg-subtle">.xhtml, .html, .txt</div>
              {/if}
            </div>
            {#if textPath}
              <span
                role="button"
                tabindex="0"
                aria-label="Clear chapter text"
                onclick={(e) => {
                  e.stopPropagation();
                  clearFile("text");
                }}
                onkeydown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.stopPropagation();
                    e.preventDefault();
                    clearFile("text");
                  }
                }}
                class="rounded-sm px-2 py-1 text-xs text-fg-muted hover:bg-surface hover:text-fg"
              >
                ×
              </span>
            {/if}
          </button>

          <button
            type="button"
            bind:this={audioDropEl}
            onclick={() => pick("audio")}
            disabled={busy}
            class="group flex items-center gap-3 rounded-md border-[1.5px] border-dashed px-4 py-5 text-left transition-[background,border-color] duration-120 {hoverZone ===
            'audio'
              ? 'border-accent bg-accent-soft'
              : audioPath
                ? 'border-success bg-success-soft'
                : 'border-border-strong bg-surface hover:border-accent hover:bg-accent-soft'}"
          >
            <svg
              width="22"
              height="22"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.5"
              stroke-linecap="round"
              stroke-linejoin="round"
              class={audioPath ? "text-success" : "text-fg-muted"}
              aria-hidden="true"
            >
              <path d="M9 18V5l12-2v13" />
              <circle cx="6" cy="18" r="3" />
              <circle cx="18" cy="16" r="3" />
            </svg>
            <div class="flex-1">
              {#if audioPath}
                <div class="text-sm font-medium text-fg">
                  {shortPath(audioPath)}
                </div>
                <div class="text-xs text-fg-subtle">
                  Click to choose a different file
                </div>
              {:else}
                <div class="text-sm font-medium text-fg">
                  Drop audio or click to choose
                </div>
                <div class="text-xs text-fg-subtle">.m4b, .m4a, .mp3</div>
              {/if}
            </div>
            {#if audioPath}
              <span
                role="button"
                tabindex="0"
                aria-label="Clear audio"
                onclick={(e) => {
                  e.stopPropagation();
                  clearFile("audio");
                }}
                onkeydown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.stopPropagation();
                    e.preventDefault();
                    clearFile("audio");
                  }
                }}
                class="rounded-sm px-2 py-1 text-xs text-fg-muted hover:bg-surface hover:text-fg"
              >
                ×
              </span>
            {/if}
          </button>
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
