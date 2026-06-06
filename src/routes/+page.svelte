<script lang="ts">
  import { onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
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

  let busy = $state(false);
  let progress = $state<ProgressEntry[]>([]);
  let currentStage = $state<string | null>(null);
  let error = $state<string | null>(null);
  let result = $state<UploadResult | null>(null);

  let languages = $state<Language[]>([]);
  let languagesError = $state<string | null>(null);
  let collections = $state<Collection[]>([]);
  let collectionsError = $state<string | null>(null);
  let loadingCollections = $state(false);

  let unlisten: UnlistenFn | undefined;

  const collectionId = $derived(Number.parseInt(collectionIdRaw, 10));
  const canSubmit = $derived(
    !busy &&
      textPath.length > 0 &&
      audioPath.length > 0 &&
      lang.trim().length > 0 &&
      title.trim().length > 0 &&
      Number.isFinite(collectionId) &&
      collectionId > 0,
  );

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
    const res = await commands.cmdListLanguages();
    if (res.status === "ok") {
      // Sort by known words descending so the user's main languages float up.
      languages = [...res.data].sort((a, b) => b.known_words - a.known_words);
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
      // Subscribe BEFORE any command can fire so we don't miss the Started event.
      unlisten = await listen<JobEvent>("job", (event) => {
        handleJobEvent(event.payload);
      });
      await loadLanguages();
    })();

    return () => {
      unlisten?.();
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
        if (!title.trim()) {
          title = filenameStem(sel);
        }
      }
    }
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

<section>
  <h1>One-shot Upload</h1>

  <div class="grid">
    <label>
      <span>Language</span>
      <select
        bind:value={lang}
        onchange={onLanguageChange}
        disabled={busy || languages.length === 0}
      >
        <option value="" disabled>
          {languagesError ? "Could not load languages" : "Select language…"}
        </option>
        {#each languages as l (l.code)}
          <option value={l.code}>{formatLanguageOption(l)}</option>
        {/each}
      </select>
      {#if languagesError}<span class="hint err">{languagesError}</span>{/if}
    </label>

    <label>
      <span>Collection</span>
      <select
        bind:value={collectionIdRaw}
        disabled={busy || loadingCollections || collections.length === 0}
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
          <option value={String(c.id)}>{c.title} ({c.id})</option>
        {/each}
      </select>
      {#if collectionsError}<span class="hint err">{collectionsError}</span>{/if}
    </label>

    <label class="span2">
      <span>Lesson title</span>
      <input
        type="text"
        bind:value={title}
        disabled={busy}
        placeholder="Auto-fills from audio filename"
      />
    </label>

    <div class="span2 picker">
      <span class="picker-label">Chapter text</span>
      <div class="picker-row">
        <button
          type="button"
          onclick={() => pick("text")}
          disabled={busy}
          class="secondary"
        >
          Choose file…
        </button>
        <code class="path">{textPath || "(none)"}</code>
      </div>
    </div>

    <div class="span2 picker">
      <span class="picker-label">Audio</span>
      <div class="picker-row">
        <button
          type="button"
          onclick={() => pick("audio")}
          disabled={busy}
          class="secondary"
        >
          Choose file…
        </button>
        <code class="path">{audioPath || "(none)"}</code>
      </div>
    </div>
  </div>

  <div class="actions">
    <button onclick={upload} disabled={!canSubmit}>
      {busy ? "Uploading…" : "Upload"}
    </button>
  </div>

  {#if currentStage || progress.length > 0}
    <div class="progress" aria-live="polite">
      <h2>Progress</h2>
      {#if currentStage}
        <p class="stage">{currentStage}</p>
      {/if}
      <ul>
        {#each progress as p, i (i)}
          <li>
            <strong>{stageLabel(p.stage)}</strong>
            <span class="pct">{Math.round(p.pct * 100)}%</span>
            {#if p.message}<span class="msg">{p.message}</span>{/if}
          </li>
        {/each}
      </ul>
    </div>
  {/if}

  {#if result}
    <div class="result" role="status">
      <h2>Uploaded</h2>
      <p>
        Lesson ID: <code>{result.lesson_id}</code>
      </p>
      <p>
        <a href={result.lesson_url} target="_blank" rel="noopener noreferrer">
          Open course in LingQ
        </a>
      </p>
    </div>
  {/if}

  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}
</section>

<style>
  section {
    max-width: 720px;
    margin: 0 auto;
  }

  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.75rem 1rem;
  }

  .span2 {
    grid-column: 1 / -1;
  }

  label {
    display: block;
  }

  label span,
  .picker-label {
    display: block;
    font-size: 0.85rem;
    margin-bottom: 0.25rem;
    color: #333;
  }

  input,
  select {
    width: 100%;
    padding: 0.5rem;
    border-radius: 4px;
    border: 1px solid #888;
    font: inherit;
    box-sizing: border-box;
    background: white;
  }

  .hint {
    display: block;
    font-size: 0.75rem;
    margin-top: 0.25rem;
  }

  .hint.err {
    color: #c00;
  }

  .picker-row {
    display: flex;
    gap: 0.5rem;
    align-items: center;
  }

  .path {
    flex: 1;
    background: #f4f4f4;
    padding: 0.4rem 0.6rem;
    border-radius: 4px;
    font-size: 0.8rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .actions {
    margin-top: 1rem;
    display: flex;
    gap: 0.5rem;
  }

  button {
    padding: 0.5rem 1rem;
    border-radius: 6px;
    border: 1px solid #535bf2;
    background: #535bf2;
    color: white;
    font-weight: 500;
    cursor: pointer;
  }

  button.secondary {
    background: transparent;
    color: inherit;
    border-color: #888;
  }

  button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .progress {
    margin-top: 1.5rem;
    border: 1px solid #ddd;
    border-radius: 6px;
    padding: 0.75rem 1rem;
    background: #fafafa;
  }

  .progress h2,
  .result h2 {
    margin: 0 0 0.5rem 0;
    font-size: 1rem;
  }

  .progress ul {
    list-style: none;
    padding: 0;
    margin: 0;
    max-height: 220px;
    overflow: auto;
    font-size: 0.85rem;
  }

  .progress li {
    padding: 0.2rem 0;
    border-bottom: 1px dashed #eee;
  }

  .progress li:last-child {
    border-bottom: none;
  }

  .pct {
    margin-left: 0.5rem;
    color: #555;
  }

  .msg {
    margin-left: 0.5rem;
    color: #777;
  }

  .stage {
    font-weight: 500;
    color: #444;
    margin: 0 0 0.5rem 0;
  }

  .result {
    margin-top: 1.5rem;
    border: 1px solid #cfd;
    background: #f4fff7;
    border-radius: 6px;
    padding: 0.75rem 1rem;
  }

  .error {
    margin-top: 1rem;
    color: #c00;
    font-size: 0.9rem;
  }
</style>
