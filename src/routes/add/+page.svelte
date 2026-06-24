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
    type ConflictResolution,
    type ProjectId,
    type TextSource,
  } from "$lib/ipc/bindings";
  import { appErrorMessage } from "$lib/errors";
  import { basename, extOf, filenameStem } from "$lib/paths";
  import { joinKey } from "$lib/identity";
  import {
    formatLanguageOption,
    getSavedLanguage,
    languagesStore,
    saveLanguage,
  } from "$lib/stores/languages.svelte";
  import SourcePicker from "$lib/components/SourcePicker.svelte";
  import DropZone from "$lib/components/DropZone.svelte";
  import BookPicker from "$lib/components/BookPicker.svelte";

  type Source = "manual" | "calibre" | "libation";

  let source = $state<Source>("manual");
  let textPath = $state("");
  let audioPaths = $state<string[]>([]);
  let audioOriginFolder = $state<string | null>(null);
  let lang = $state("");
  let title = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);
  let pickedCandidate = $state<Candidate | null>(null);
  let conflict = $state<{
    existing: ProjectId;
    conflict_title: string;
  } | null>(null);

  let textDropEl = $state<HTMLButtonElement | null>(null);
  let audioDropEl = $state<HTMLButtonElement | null>(null);
  let hoverZone = $state<"text" | "audio" | null>(null);
  let unlistenDrop: UnlistenFn | undefined;

  let showAllLanguages = $state(false);
  let defaultApplied = false;

  const languages = $derived(languagesStore.languages);
  const languagesError = $derived(
    languages.length === 0 ? languagesStore.error : null,
  );

  const visibleLanguages = $derived(
    showAllLanguages ? languages : languages.filter((l) => l.known_words > 0),
  );

  function applyDefaultLanguage() {
    if (lang || defaultApplied || languages.length === 0) return;
    defaultApplied = true;
    const saved = getSavedLanguage();
    const match =
      (saved && languages.find((l) => l.code === saved)) ??
      visibleLanguages[0] ??
      languages[0];
    if (!match) return;
    lang = match.code;
    if (!visibleLanguages.some((l) => l.code === match.code)) {
      showAllLanguages = true;
    }
  }

  $effect(() => {
    if (languages.length > 0) applyDefaultLanguage();
  });

  $effect(() => {
    if (lang) saveLanguage(lang);
  });

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

  function appendAudio(newPaths: string[]) {
    if (!newPaths.length) return;
    const existing = new Set(audioPaths);
    const additions: string[] = [];
    for (const p of newPaths) {
      if (!existing.has(p)) {
        existing.add(p);
        additions.push(p);
      }
    }
    if (!additions.length) return;
    audioPaths = [...audioPaths, ...additions];
    audioOriginFolder = null;
  }

  function assignToZone(zone: "text" | "audio", paths: string[]) {
    if (zone === "audio") {
      const audio = paths.filter((p) => zoneForExt(extOf(p)) === "audio");
      if (audio.length) appendAudio(audio);
      return;
    }
    const matching =
      paths.find((p) => zoneForExt(extOf(p)) === zone) ?? paths[0];
    if (!matching) return;
    textPath = matching;
    if (!title) title = filenameStem(matching);
  }

  function assignDropped(paths: string[]) {
    if (!paths.length) return;
    let textCandidate: string | null = null;
    const audioCandidates: string[] = [];
    for (const p of paths) {
      const z = zoneForExt(extOf(p));
      if (z === "text" && !textCandidate) textCandidate = p;
      else if (z === "audio") audioCandidates.push(p);
    }
    if (textCandidate) {
      textPath = textCandidate;
      if (!title) title = filenameStem(textCandidate);
    }
    if (audioCandidates.length) appendAudio(audioCandidates);
  }

  async function expandFolderDrop(path: string): Promise<boolean> {
    const ext = extOf(path);
    if (ext) return false;
    const res = await commands.cmdExpandAudioDir(path);
    if (res.status !== "ok" || res.data.length === 0) return false;
    const existing = new Set(audioPaths);
    const additions = res.data.filter((p) => !existing.has(p));
    if (!additions.length) return true;
    audioPaths = [...audioPaths, ...additions];
    audioOriginFolder = basename(path);
    return true;
  }

  async function handleDrop(paths: string[]) {
    if (!paths.length) return;
    const leftover: string[] = [];
    for (const p of paths) {
      const ext = extOf(p);
      if (!ext) {
        const expanded = await expandFolderDrop(p);
        if (!expanded) leftover.push(p);
      } else {
        leftover.push(p);
      }
    }
    if (leftover.length) assignDropped(leftover);
  }

  async function handleDropOnZone(zone: "text" | "audio", paths: string[]) {
    if (zone !== "audio") {
      assignToZone(zone, paths);
      return;
    }
    const leftover: string[] = [];
    for (const p of paths) {
      const ext = extOf(p);
      if (!ext) {
        const expanded = await expandFolderDrop(p);
        if (!expanded) leftover.push(p);
      } else {
        leftover.push(p);
      }
    }
    if (leftover.length) assignToZone("audio", leftover);
  }

  function removeAudio(p: string) {
    audioPaths = audioPaths.filter((q) => q !== p);
    audioOriginFolder = null;
  }

  function clearAudio() {
    audioPaths = [];
    audioOriginFolder = null;
  }

  // Tauri 2's drag-drop event reports CSS pixels on macOS but physical pixels
  // on Windows/Linux. Try the raw coord first; if nothing hits and the device
  // pixel ratio is non-unity, retry divided by dpr. Works on both platforms
  // without sniffing the user agent.
  function resolveZone(x: number, y: number): "text" | "audio" | null {
    const direct = hitTestZone(x, y);
    if (direct) return direct;
    const dpr = window.devicePixelRatio || 1;
    if (dpr !== 1) return hitTestZone(x / dpr, y / dpr);
    return null;
  }

  onMount(() => {
    let disposed = false;
    (async () => {
      const off = await getCurrentWebview().onDragDropEvent((event) => {
        if (busy) return;
        const p = event.payload;
        if (p.type === "over") {
          hoverZone = resolveZone(p.position.x, p.position.y);
        } else if (p.type === "leave") {
          hoverZone = null;
        } else if (p.type === "drop") {
          const zone = resolveZone(p.position.x, p.position.y);
          if (zone) void handleDropOnZone(zone, p.paths);
          else void handleDrop(p.paths);
          hoverZone = null;
        }
      });
      if (disposed) {
        off();
        return;
      }
      unlistenDrop = off;
      void languagesStore.ensureLoaded();
    })();
    return () => {
      disposed = true;
      unlistenDrop?.();
    };
  });

  // When the user switches source modes, drop any in-flight conflict prompt
  // and any picked library candidate — they belong to the previous mode.
  $effect(() => {
    source;
    conflict = null;
    pickedCandidate = null;
  });

  const isManual = $derived(source === "manual");

  const canCreate = $derived(
    busy
      ? false
      : isManual
        ? !!textPath && audioPaths.length > 0 && !!lang.trim() && !!title.trim()
        : pickedCandidate !== null,
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
      multiple: true,
      filters: [{ name: "Audio", extensions: ["m4b", "m4a", "mp3"] }],
    });
    const picked: string[] =
      sel == null ? [] : Array.isArray(sel) ? sel : [sel];
    if (picked.length) appendAudio(picked);
  }

  function toTextSource(path: string): TextSource {
    return { kind: "epub", value: path } as TextSource;
  }

  function toAudioSource(paths: string[]): AudioSource {
    if (paths.length === 1) {
      return { kind: "single_file", value: paths[0] } as AudioSource;
    }
    return { kind: "multiple_files", value: paths } as AudioSource;
  }

  function buildPayload(): {
    candidate: Candidate;
    language: string;
    title: string;
  } | null {
    if (!isManual) {
      const c = pickedCandidate;
      if (!c) return null;
      return {
        candidate: c,
        language: c.language ?? "",
        title: c.title,
      };
    }
    if (!textPath || audioPaths.length === 0) return null;
    const c: Candidate = {
      source_id: source,
      title,
      authors: [],
      language: lang,
      series: null,
      cover_path: null,
      text_source: toTextSource(textPath),
      audio_source: toAudioSource(audioPaths),
      chapter_manifest: null,
      metadata_extras: {},
    };
    return { candidate: c, language: lang, title };
  }

  async function onCreate() {
    if (!canCreate) return;
    const payload = buildPayload();
    if (!payload) return;
    busy = true;
    error = null;
    conflict = null;
    const res = await commands.cmdCreateProject(
      payload.candidate,
      payload.language,
      payload.title,
    );
    busy = false;
    if (res.status === "error") {
      error = appErrorMessage(res.error);
      return;
    }
    if (res.data.status === "created") {
      goto(`/match/${encodeURIComponent(joinKey(res.data.id))}`);
      return;
    }
    // status === "conflict"
    conflict = {
      existing: res.data.existing,
      conflict_title: res.data.conflict_title,
    };
  }

  async function resolve(r: ConflictResolution) {
    if (!conflict) return;
    const payload = buildPayload();
    if (!payload) return;
    if (r === "skip") {
      const id = conflict.existing;
      conflict = null;
      const key = joinKey(id);
      const loaded = await commands.cmdProjectLoad(key);
      if (loaded.status === "ok" && loaded.data.confirmed_at != null) {
        goto(`/run/${encodeURIComponent(key)}`);
      } else {
        goto(`/match/${encodeURIComponent(key)}`);
      }
      return;
    }
    busy = true;
    error = null;
    const res = await commands.cmdCreateProjectWithResolution(
      payload.candidate,
      payload.language,
      payload.title,
      r,
    );
    busy = false;
    if (res.status === "error") {
      error = appErrorMessage(res.error);
      return;
    }
    conflict = null;
    goto(`/match/${encodeURIComponent(joinKey(res.data))}`);
  }
</script>

<section class="col-form space-y-6 pt-6">
  <header>
    <h1 class="text-lg font-semibold text-fg">Add Project</h1>
    <p class="mt-1 text-sm text-fg-muted">
      Pick a source, a book, and audio. Collection is created automatically.
    </p>
  </header>

  <SourcePicker bind:value={source} />

  {#if source !== "manual"}
    <BookPicker
      source={source as "calibre" | "libation"}
      bind:selectedCandidate={pickedCandidate}
    />
  {/if}

  {#if isManual}
    <fieldset class="space-y-2">
      <legend class="text-xs font-medium uppercase tracking-wide text-fg-muted">
        Book (EPUB or HTML)
      </legend>
      <DropZone
        variant="text"
        paths={textPath ? [textPath] : []}
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
        paths={audioPaths}
        hovered={hoverZone === "audio"}
        disabled={busy}
        onPick={pickAudio}
        onRemove={removeAudio}
        onClear={clearAudio}
        originLabel={audioOriginFolder ?? undefined}
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
          class="h-10 w-full rounded-sm border border-border bg-surface px-3 text-sm"
          disabled={busy}
        />
      </label>
      <label class="space-y-1">
        <span class="text-xs font-medium uppercase tracking-wide text-fg-muted">
          Language
        </span>
        <select
          bind:value={lang}
          disabled={busy || visibleLanguages.length === 0}
          class="h-10 w-full rounded-sm border border-border bg-surface px-3 text-sm text-fg outline-none disabled:bg-surface-sunken disabled:text-fg-subtle"
        >
          <option value="" disabled>
            {languagesError ? "Could not load languages" : "Select language…"}
          </option>
          {#each visibleLanguages as l (l.code)}
            <option value={l.code}>{formatLanguageOption(l)}</option>
          {/each}
        </select>
        {#if languagesError}
          <span class="block text-xs text-error">{languagesError}</span>
        {:else if languages.length > 0}
          <label
            class="flex cursor-pointer items-center gap-1.5 text-xs text-fg-muted"
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
    </div>
  {:else if pickedCandidate}
    <div class="rounded-sm border border-border bg-surface-sunken p-3 text-sm">
      <div class="font-medium text-fg">
        Selected: {pickedCandidate.title}
      </div>
      <div class="mt-0.5 text-xs text-fg-muted">
        {pickedCandidate.authors.length
          ? pickedCandidate.authors.join(", ")
          : "Unknown author"}
        — {pickedCandidate.language ?? "language unknown"}
      </div>
    </div>
  {/if}

  {#if error}
    <p
      class="rounded-sm border border-error-soft bg-error-soft/30 px-4 py-2 text-sm text-fg"
    >
      {error}
    </p>
  {/if}

  {#if conflict !== null}
    <div
      class="rounded-md border border-warning/40 bg-warning/10 p-4 space-y-3"
    >
      <p class="text-sm text-fg">
        A project already exists for <em>{conflict.conflict_title}</em>. What do
        you want to do?
      </p>
      <div class="flex flex-wrap gap-2">
        <button
          type="button"
          disabled={busy}
          onclick={() => resolve("replace")}
          class="rounded-sm bg-accent px-3 py-1.5 text-sm font-medium text-white hover:bg-accent-hover disabled:bg-fg-subtle"
        >
          Replace
        </button>
        <button
          type="button"
          disabled={busy}
          onclick={() => resolve("skip")}
          class="rounded-sm bg-accent px-3 py-1.5 text-sm font-medium text-white hover:bg-accent-hover disabled:bg-fg-subtle"
        >
          Skip and open existing
        </button>
        <button
          type="button"
          disabled={busy}
          onclick={() => resolve("new_project")}
          class="rounded-sm bg-accent px-3 py-1.5 text-sm font-medium text-white hover:bg-accent-hover disabled:bg-fg-subtle"
        >
          Create a copy
        </button>
        <button
          type="button"
          disabled={busy}
          onclick={() => (conflict = null)}
          class="ml-auto rounded-sm border border-border bg-surface px-3 py-1.5 text-sm text-fg hover:bg-surface-sunken disabled:text-fg-subtle"
        >
          Back
        </button>
      </div>
    </div>
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
