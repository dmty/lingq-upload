# Architecture Decisions — lingq-upload

> Evergreen contracts for the codebase. ADs are stable; their numbers are the canonical references that source code and other docs can cite. Add new ADs by appending; never renumber.

## AD-001 — SvelteKit in SPA mode on Tauri's file:// origin

**Decision:** Use `@sveltejs/adapter-static` with `ssr=false` and `prerender=true`. No SSR.

**Why:** Tauri serves the frontend from a `file://` (or `tauri://`) origin in a webview. SSR would be both pointless (no server) and broken (no Node runtime in production). Prerendering yields static assets the Rust binary embeds.

**Consequences:**
- `+page.server.ts` / `+layout.server.ts` are not used.
- Form actions don't work; substitute with `invoke()` calls to Rust commands.
- SEO / OG tags are irrelevant.

## AD-002 — Svelte 5 runes, no legacy reactive syntax

**Decision:** Greenfield code uses runes only: `$state`, `$derived`, `$effect`, `$props`. The legacy `$:` reactive statement and the `export let` pattern are banned in new code.

**Why:** Mixing the two models is the documented pain point in Svelte 5 migrations. The repo is greenfield — start clean.

**Consequences:**
- Component props use `let { foo, bar } = $props()`.
- Stores still allowed for cross-component shared state, but prefer rune-based `.svelte.ts` modules.

## AD-003 — IPC: invoke for request/response, emit/listen for streams

**Decision:**
- All synchronous backend calls go through `tauri::command` functions wrapped by `invoke()`. Return type is `Result<T, AppError>` where `AppError` is a single discriminated `thiserror` enum.
- All progress streams (transcoding %, upload %, log lines, integrity-check results) go through `tauri::Window::emit("job", &JobEvent)` with frontend subscribing via `listen('job', cb)`. **Never** poll `invoke` for progress.
- Each long-running job carries a `job_id: Uuid` so the frontend can route events to the right UI element.

**Why:** Conflating request/response with streaming forces awkward polling and burns CPU. `JobEvent` as a discriminated union maps cleanly to a TypeScript tagged union.

**Consequences:**
- Frontend stores subscribe on mount, unsubscribe on unmount — no leaked listeners.
- Cancellation: frontend calls `invoke('cancel_job', { job_id })` which signals a cancellation token on the Rust side.

## AD-004 — Type generation via specta / tauri-specta

**Decision:** Use `specta` + `tauri-specta` to generate `src/lib/ipc/bindings.ts` from Rust command signatures and shared types. Regenerate on every `cargo build` via a `build.rs` hook.

**Why:** Hand-maintained TS types drift from Rust signatures within days. `specta` is the most Tauri-2-native option; it generates the `invoke` wrappers too, so frontend code calls `commands.uploadLesson(args)` instead of `invoke('upload_lesson', args)` with stringly-typed names.

**Reversibility:** Hardest of all the choices listed here to reverse — would require touching every frontend call site. Worth locking in now.

**Fallback:** If specta proves problematic, fall back to hand-written `bindings.ts` + manual `invoke()` calls. Document the fallback and revisit.

## AD-005 — State: Svelte stores / rune modules, no XState yet

**Decision:** Early development uses plain Svelte stores and `.svelte.ts` rune modules for state. Defer the question of a formal state-machine library (XState, Robot) until the project orchestration state machine actually exists.

**Why:** Premature framework adoption is more expensive than two weeks of carefully-named booleans. The project state machine (`New | Parsed | Mapped | …`) lives in **Rust**, not the frontend — the frontend just renders the current stage label.

## AD-006 — Error model: single AppError enum, thiserror, surfaced as TS union

**Decision:** One top-level `AppError` enum in `src-tauri/src/error.rs` with module-specific variants (`Lingq(LingqError)`, `Audio(AudioError)`, `Epub(EpubError)`, `Project(ProjectError)`, `Secrets(SecretError)`, `Io(io::Error)`). Each module owns a focused `thiserror` enum and lifts into `AppError` via `#[from]`. Specta serialises this as a discriminated TS union; the frontend pattern-matches.

**Why:** Tauri commands need a single error type. Module-local errors keep the type signatures honest. The discriminated union on the TS side gives exhaustive `switch` checking.

**Anti-pattern banned:** `anyhow::Error` at the `tauri::command` boundary. `anyhow` is fine deep inside, but the boundary must be typed.

## AD-007 — Event streaming: typed JobEvent discriminated union

**Decision:**
```rust
#[derive(Serialize, Type, Clone)]
#[serde(tag = "kind")]
pub enum JobEvent {
    Started   { job_id: Uuid, stage: Stage },
    Progress  { job_id: Uuid, pct: f32, message: Option<String> },
    Log       { job_id: Uuid, level: LogLevel, message: String },
    Result    { job_id: Uuid, ok: bool, payload: serde_json::Value },
    Cancelled { job_id: Uuid },
}
```

Emitted on a single channel `"job"`; frontend routes by `job_id` and `kind`.

**Why:** One channel = one `listen()` per app, simpler subscription lifecycle. Discriminated union prevents add-a-new-field-and-forget-the-other-end bugs.

## AD-008 — Audio: subprocess to bundled ffmpeg, not ffmpeg-next bindings

**Decision:** Shell out to a bundled ffmpeg binary via `tokio::process::Command`. Defer `ffmpeg-next` bindings to a possible post-v1 perf pass.

**Why:** Bindings add a C build dependency to a Rust-only project. Bundled binary keeps the build dead simple on all three platforms. Sequential file writes (AD-011) handle the known corruption hazard from parallel writes to a sync-mounted filesystem.

**Source of binary:** BtbN's LGPL-built static ffmpeg.

**Cancellation:** Dropping the `Child` kills ffmpeg. macOS verified; Windows requires further verification.

## AD-009 — Secrets: keyring-rs, never project.json

**Decision:** Use the `keyring-rs` crate behind a `KeyringBackend` trait (so CI can stub it). The LingQ API key is the only secret today; it lives under service `nz.verum.lingq-importer`, account `lingq_api_key`. The crate handles macOS Keychain, Windows Credential Manager, and `libsecret` on Linux.

**Banned:** Writing the API key to `project.json`, environment variables, or any log output. `tracing` redacts the `Authorization` header by default.

## AD-010 — Persistence: project.json with serde + schemaVersion

**Decision:** Single `project.json` per project. Atomic writes via write-to-tempfile-then-rename. Top-level `schemaVersion: 1`. Future migrations get explicit upgraders.

**Why:** Plain JSON keeps the door open to inspect / git / sync. Atomic rename is portable; sqlite would buy nothing at this scale.

**Reversibility:** Easy. The migration story is straightforward.

## AD-011 — Concurrency: sequential transcoding by default

**Decision:** Audio transcoding is sequential by default. A `concurrency: 1` setting exists in `project.settings.encoder` but defaults to 1 and is not exposed in v1 UI.

**Why:** Parallel ffmpeg invocations writing to a sync-mounted filesystem (iCloud, OneDrive, Dropbox) produced silently-corrupted mp3 files with arbitrary duration drift. Optional parallelism — if ever added — must transcode to a local scratch directory first, then move the verified files into place.

## AD-012 — Repository layout

Reflects the actual scaffold: package `lingq-upload`, Rust crate `lingq_upload_lib` (per the Tauri 2 Windows naming workaround), Bun as the package manager, SvelteKit 5 + adapter-static already wired.

```
lingq-upload/                  # repo root
├── src-tauri/                 # Rust backend
│   ├── Cargo.toml             # crate: lingq_upload_lib + bin: lingq-upload
│   ├── build.rs
│   ├── tauri.conf.json
│   └── src/
│       ├── main.rs            # boots Tauri
│       ├── lib.rs             # library entry
│       ├── commands/          # #[tauri::command] thin wrappers
│       ├── core/              # pure-Rust domain modules
│       ├── codecs/            # AudioCodec strategy registry (AD-014)
│       ├── languages/         # LanguageProfile registry (AD-015)
│       ├── epub/strategies/   # HeadingStrategy registry (AD-016)
│       ├── ingest/            # IngestSource registry (AD-019)
│       ├── events.rs
│       └── error.rs
├── src/                       # SvelteKit frontend
│   ├── routes/                # filesystem router
│   │   ├── +layout.svelte
│   │   ├── +page.svelte       # one-shot upload
│   │   ├── library/+page.svelte
│   │   ├── new-project/+page.svelte
│   │   ├── project/[id]/+page.svelte
│   │   └── settings/+page.svelte
│   ├── lib/
│   │   ├── ipc/bindings.ts    # specta-generated
│   │   ├── stores/            # rune-based modules
│   │   └── components/
│   └── app.html
├── static/                    # static assets
├── svelte.config.js
├── vite.config.ts
├── package.json
└── resources/                 # ffmpeg binary, icons
```

Notes: `src/` follows SvelteKit's top-level convention. Tailwind config at root.

## AD-013 — Testing pyramid

- **Rust unit tests** in `src-tauri/src/**/*.rs` (`#[cfg(test)] mod tests`).
- **Rust integration tests** in `src-tauri/tests/` — snapshot tests via `insta`; HTTP mocks via `mockito`.
- **Frontend unit / component tests:** Vitest.
- **End-to-end:** Playwright against `bun tauri dev`. One golden-path test per release-relevant flow.
- **Manual smoke** against the live LingQ API on demand. The checklist lives in the local planning workspace, not in the source tree.

CI matrix runs Rust + Vitest on macOS / Windows / Ubuntu. Playwright runs on Ubuntu only.

## AD-014 — Audio codec strategy: pluggable AudioCodec trait

**Decision:** Wrap all audio decoding / transcoding behind a single trait so adding new input formats (`.opus`, `.aac`, `.wav`, `.flac`, raw `.mp3`) does not touch the carver, uploader, or UI.

```rust
// src-tauri/src/codecs/mod.rs
pub trait AudioCodec: Send + Sync {
    fn id(&self) -> &'static str;
    fn extensions(&self) -> &'static [&'static str];
    fn can_decode(&self, path: &Path) -> bool;
    fn probe(&self, path: &Path) -> BoxFuture<'_, Result<MediaInfo, AudioError>>;
    fn transcode(
        &self,
        src: &Path,
        dst: &Path,
        target: &EncoderSettings,
    ) -> BoxFuture<'_, Result<TranscodeReport, AudioError>>;
}

pub struct CodecRegistry { /* Vec<Box<dyn AudioCodec>> */ }
impl CodecRegistry {
    pub fn detect(&self, path: &Path) -> Option<&dyn AudioCodec>;
}
```

**Built-in registrations:** `FfmpegCodec` covers `m4b`, `m4a`, `mp3`, `aac`, `wav`, `opus`, `flac`, `ogg` via the bundled ffmpeg subprocess (ffmpeg autodetects by extension + container probe). The strategy boundary still pays off: a future `OpusCodec` using `libopus` bindings, or a hypothetical streaming-source codec, slots in without rewriting `core/`.

**Why:** Today everything happens to be ffmpeg-only, but the trait shape forces every consumer to call `registry.detect(path)?.transcode(…)` instead of `ffmpeg::transcode(…)` — so future codecs don't ripple.

**Anti-pattern banned:** `core::carver` or `core::project` importing anything from `codecs/ffmpeg.rs` directly. They depend on the trait only.

## AD-015 — Language profile strategy: pluggable LanguageProfile trait

**Decision:** Per-language behaviour (script-specific text normalisation, default LingQ `level`, default sample rate, fuzzy-match metric, paragraph rules) lives behind a `LanguageProfile` trait keyed by IETF BCP-47 language tag.

```rust
// src-tauri/src/languages/mod.rs
pub trait LanguageProfile: Send + Sync {
    fn tag(&self) -> &'static str;             // "ja", "zh-Hans", "en", "ko", "ru", …
    fn lingq_url_segment(&self) -> &'static str; // matches LingQ's /api/v3/{seg}/…
    fn normalise_text(&self, input: &str) -> String; // furigana strip, full-width fold, etc.
    fn fuzzy_metric(&self) -> FuzzyMetric;     // Jaro-Winkler vs Levenshtein vs custom
    fn defaults(&self) -> LangDefaults;        // level, status, tag, encoder hints
    fn paragraph_rules(&self) -> ParagraphRules; // CJK ideographic space, EN double-newline, …
}

pub struct LanguageRegistry { /* HashMap<&'static str, Box<dyn LanguageProfile>> */ }
```

**Built-in registrations:** `JapaneseProfile` (furigana strip + CJK orphan-space killer + Jaro-Winkler). All other languages route to `GenericLatinProfile` (Unicode NFC normalisation + Levenshtein + simple whitespace rules) so the app boots for any LingQ language out of the box, then gets refined per-language as users hit edges.

**Why:** Without the profile, Japanese-specific transforms (`furigana_strip`, CJK orphan-space killer, the fuzzy ratio metric) bleed into the pipeline in three places. Pulling them behind a profile makes "add Chinese" or "add Russian" an additive PR, not a refactor.

**Decision boundary:** Heading-detection strategies are EPUB-publisher-specific (Kindle / Kobo), not language-specific — they stay in AD-016. A Japanese Kindle book and a Russian Kindle book share the heading strategy but differ on the language profile.

## AD-016 — EPUB heading strategy registry

**Decision:** Heading detection is a publisher-specific `HeadingStrategy`. Detection is by autodetect probe, not hard-coded branches.

```rust
// src-tauri/src/epub/strategies/mod.rs
pub trait HeadingStrategy: Send + Sync {
    fn id(&self) -> &'static str;                  // "kindle", "kobo", "generic-h1"
    fn confidence(&self, sample: &EpubProbe) -> f32; // 0..1 against a sampled XHTML excerpt
    fn detect(&self, html: &Html, node: ElementRef) -> Option<HeadingLevel>;
}

pub fn autodetect(probe: &EpubProbe, registry: &StrategyRegistry) -> Box<dyn HeadingStrategy> {
    registry.iter().max_by(|a, b| a.confidence(probe).total_cmp(&b.confidence(probe))).unwrap()
}
```

**Built-in:** `KindleStrategy`, `KoboStrategy`. Fallback `GenericH1Strategy` for anything where confidence < 0.4 on the named strategies — strips `<h1>` / `<h2>` only.

**UI escape hatch:** If autodetect picks wrong, the wizard exposes a "Heading style" dropdown listing every registered strategy. User can force.

## AD-017 — LingQ API language threading

**Decision:** `LingqClient` takes a `lang: &str` constructor arg and templates every URL: `/api/v3/{lang}/collections/…`, `/api/v3/{lang}/lessons/…`. No hardcoded `ja` anywhere. The language string comes from `project.settings.lang`, which in turn defaults from `LanguageProfile::lingq_url_segment()`.

**Why:** LingQ uses the language segment as a tenant boundary — calling `…/ja/…` with a Russian collection ID returns 404.

**Open probe:** Verify that the multipart `language` field in `import_lesson` must also match the URL segment. Existing implementations pass both; carry that forward and document if either becomes optional.

## AD-019 — Ingest source strategy: pluggable IngestSource trait

**Decision:** Library content (candidate books to import) enters the app through a single trait. Calibre, Libation, and "raw folder" are three implementations of the same interface; the rest of the pipeline doesn't care which produced a given candidate.

```rust
// src-tauri/src/ingest/mod.rs
pub trait IngestSource: Send + Sync {
    fn id(&self) -> &'static str;            // "calibre", "libation", "manual"
    fn label(&self) -> &'static str;         // human-readable, for UI picker
    fn scan(&self, root: &Path) -> BoxFuture<'_, Result<Vec<Candidate>, IngestError>>;
    fn enrich(&self, c: &mut Candidate) -> BoxFuture<'_, Result<(), IngestError>>;
}

pub struct Candidate {
    pub source_id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub series: Option<SeriesRef>,
    pub cover_path: Option<PathBuf>,
    pub text_source: TextSource,    // Epub(PathBuf) | LooseFiles { paths } | …
    pub audio_source: Option<AudioSource>, // Folder(PathBuf) | LibationManifest(PathBuf) | …
    pub chapter_manifest: Option<ChapterManifest>, // pre-known chapter breaks
    pub metadata_extras: HashMap<String, serde_json::Value>, // ASIN, narrator, etc.
}

pub struct IngestRegistry { /* Vec<Box<dyn IngestSource>> */ }
```

**Built-in registrations:**

- **`ManualSource`:** user picks an EPUB file + audio file directly. No scan, no enrich. The baseline.
- **`CalibreLibrarySource`:** point at a Calibre library root (`metadata.db` + per-book folders). Scan returns every book whose `metadata.opf` declares the user's target languages. Enrich pulls title, authors, language, series, `series_index`, tags, cover from `metadata.opf`. See `docs/specs/calibre-opf.md`.
- **`LibationFolderSource`:** point at a Libation download root. Scan returns every audiobook with a JSON sidecar. Enrich pulls title, narrator, ASIN, chapter manifest, durations. See `docs/specs/libation-sidecar.md`.

**Why:** Hardcoding Calibre paths into the wizard would couple every UI flow to a single source. The trait keeps the pipeline ignorant — a `Candidate` from Calibre and a `Candidate` from `ManualSource` flow through `core::carver`, `core::audio`, `core::lingq` identically.

**Pairing rule:** when two sources scan the same library, the app reconciles candidates by `(normalised_title, normalised_author)`. Conflicts surface in UI; the user picks the winning record. Reconciliation is **not** an `IngestSource` concern — it's an upstream service in `core::library::reconcile`.

## AD-020 — Library catalog cache: library.index.json

**Decision:** Maintain a single `~/.lingq-importer/library.index.json` file that lists every known project with denormalised metadata for fast filter / search / sort in the Library UI. The file is a **cache rebuildable from project.json files** — never the source of truth.

```jsonc
// ~/.lingq-importer/library.index.json
{
  "schemaVersion": 1,
  "generatedAt": "2026-06-06T12:34:56Z",
  "entries": [
    {
      "projectId": "uuid",
      "projectPath": "/abs/path/to/project.json",
      "title": "海辺のカフカ（下）",
      "authors": ["村上春樹"],
      "language": "ja",
      "series": { "name": "海辺のカフカ", "index": 2 },
      "tags": ["books", "literary"],
      "lingqStatus": "uploaded",   // none | pending | uploaded | needsFix | done
      "lastImportedAt": "...",
      "coverThumbPath": "..."
    }
    // …
  ]
}
```

**Maintenance:**
- Updated atomically (tempfile + rename) after every `Project::save_atomic`.
- Rebuilt from scratch on app start if missing, corrupt, or `schemaVersion` mismatched. Walk all project dirs, parse each `project.json`, regenerate.
- Per-entry update on project change; never partial-write.

**Why:** project.json-per-directory is fine for write. It collapses on **query** — "show me everything in Japanese", "show me uploaded books by series", "sort by last imported." Filesystem walk + N parses every keystroke is unacceptable past ~50 projects. The index file makes the Library screen feel instant.

**Boundary:** This is **AD-010 (persistence) amended**. The `project.json` per project remains the source of truth. The index is derivable; if it disagrees with a project.json, the index loses.

**SQLite threshold:** Migrate to SQLite when (a) project count crosses ~500, OR (b) cross-project queries become rich (full-text search across lessons, SRS state, library-wide tag editing) — whichever first. Both are post-v1.

## AD-018 — Public extension points are stable, internal impls are not

**Decision:** The four strategy traits (`AudioCodec`, `LanguageProfile`, `HeadingStrategy`, `IngestSource`) plus the `JobEvent` enum, the `project.json` schema, and the `library.index.json` schema are the **public extension surface**. Breaking changes to these require a `schemaVersion` bump and a migrator. Everything else (concrete codec impls, internal carver helpers, frontend stores) is free to churn.

**Why:** Without naming the contract, every refactor risks invalidating a downstream extension. Naming it lets internal cleanup happen without ceremony.

**Convention:** Public extension traits live under `src-tauri/src/{codecs,languages,epub/strategies,ingest}/mod.rs` and re-export via `lib.rs` so out-of-tree builds (a future plugin host) can depend on them.

## Open architecture questions

1. **Re-import / diff behaviour** — when the same Candidate is re-scanned (Calibre edit, Libation re-rip), what's the per-chapter conflict policy? Overwrite / append / skip / prompt? Current default: append-by-default, prompt on conflict.
2. **Log persistence** — `tracing` to file alongside `project.json`? Or in-memory ring buffer surfaced via a `JobEvent::Log` stream? Decide before resumability work begins.
3. **i18n of the app UI itself** — currently English-only; `lang` is per-project, not per-app. The target user is a multi-language learner, so this is likely worth doing.
4. **Local lesson cache scope** — the persistence layer captures full parsed lessons locally so future on-device study features stay viable without immediate scope expansion. Schema: in `project.json` `parsed.lessons[]`, or in a sibling `lessons/` directory of chunked files? Decide before any consumer depends on the shape.
