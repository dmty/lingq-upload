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

**Decision:** Use the `keyring-rs` crate behind a `KeyringBackend` trait (so CI can stub it). The LingQ API key is the only secret today; it lives under service `com.lingq.upload` (matches `tauri.conf.json` bundle identifier), account `lingq_api_key`. The crate handles macOS Keychain, Windows Credential Manager, and `libsecret` on Linux.

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
│       ├── core/epub/         # EpubVendor enum dispatch + strategies (AD-016, AD-026)
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

## AD-016 — EPUB heading strategy dispatch

**Decision:** Heading detection is publisher-specific. Strategy selection is a closed `EpubVendor` enum (`Kindle`, `Kobo`, `Generic`) dispatched by exhaustive match in `core::epub`, chosen by the autodetect probe (AD-026) — not call-site branches, and not a dyn-trait registry.

**Why enum, not trait + registry:** the vendor set is closed and small, and the autodetect heuristic must enumerate every vendor anyway, so a registry would have no second consumer. The enum gives exhaustive-match safety and serialises directly over IPC (`specta`-exported). Revisit a trait + registry only when a third real vendor strategy lands (rule of three).

**Built-in:** Kindle parser (`parse.rs`), `KoboStrategy` (`kobo.rs`). `Generic` dispatches to the Kindle path — lowest-regression default for unknown books.

**UI escape hatch:** deferred — the chosen strategy is logged and surfaced on `JobEvent::Started` so field reports can identify false positives before a UI override knob is added (see AD-026 consequences).

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

**Decision:** The three strategy traits (`AudioCodec`, `LanguageProfile`, `IngestSource`), the `EpubVendor` enum dispatch (AD-016 / AD-026), the `JobEvent` enum, the `project.json` schema, and the `library.index.json` schema are the **public extension surface**. Breaking changes to these require a `schemaVersion` bump and a migrator. Everything else (concrete codec impls, internal carver helpers, frontend stores) is free to churn.

**Why:** Without naming the contract, every refactor risks invalidating a downstream extension. Naming it lets internal cleanup happen without ceremony.

**Convention:** Public extension traits live under `src-tauri/src/{codecs,languages,ingest}/mod.rs` and re-export via `lib.rs` so out-of-tree builds (a future plugin host) can depend on them. Heading strategies are not a trait — adding an EPUB vendor is an additive `EpubVendor` variant plus a strategy module (AD-016).

**`AudioSource` variant set.** `ingest::AudioSource` has four variants — `SingleFile`, `Folder`, `LibationManifest`, `MultipleFiles` — and the enum is the public contract for "where the audio bytes for a project come from". Adding a variant is additive but the compiler must enforce that every dispatch site updates in lockstep: the resolver in `core/job/mod.rs::resolve_audio_tracks` matches the variant set **exhaustively** (no `_ =>` arm), the upload guard in `commands/upload.rs` flows through the shared `audio_source_paths` helper in `ingest/mod.rs`, the builder helpers in `ingest/manual.rs` pick the right variant from the input shape, and any new `IngestSource` impl picks the right variant at scan time. Drop a variant and every one of those sites must be re-checked the same way.

## AD-021 — Project identity is a multi-key tuple, not a scalar

**Decision:** A project's identity is a small bag of external keys plus a derived content hash, not a single ID. All projects (Calibre-sourced, Libation-sourced, manually picked) carry the same shape; some slots may be `None`.

```rust
// src-tauri/src/core/identity.rs
pub struct ProjectId {
    pub content_hash: [u8; 32],         // sha256 over NFC(title) + "\x1f" + NFC(first_author)
    pub audible_asin: Option<String>,   // present when ingested from Libation
    pub isbn13: Option<String>,         // present when Calibre opf carries an ISBN
    pub calibre_uuid: Option<Uuid>,     // Calibre's local UUID; last-resort fallback
}

impl ProjectId {
    pub fn matches(&self, other: &Self) -> bool;  // any strong key agrees; None slots ignored
    pub fn join_key(&self) -> String;             // asin > isbn13 > uuid > hex(content_hash)
}
```

**Why:** Audible's ASIN and Calibre's ISBN/UUID name the same book in two different universes. Picking one as canonical loses the other; using `(title, author)` alone is fragile across editions and translations. The same multi-external-id pattern is what OpenLibrary, Calibre itself, and Audiobookshelf converged on.

**Reconciliation rule:** at `core::library::reconcile` time, two candidates are merged if **any** strong key matches (ASIN ↔ ASIN, or ISBN13 ↔ ISBN13, or UUID ↔ UUID). If no strong key is present on either side, fall back to `content_hash` equality. Below-threshold fuzzy `(title, author)` overlap surfaces as a conflict; the user picks the winner. Silent fuzzy merge is banned.

**`join_key` precedence:** ASIN beats ISBN13 beats UUID beats `hex(content_hash)`. The `join_key` is the filename for `$APP_DATA/projects/{join_key}.json` — stable across reruns, opaque to humans, debuggable as a hex string.

**Consequences:**
- Every `IngestSource::scan` result populates as many slots as it can. `LibationFolderSource` always sets `audible_asin` (extracted from the `[B0XXXXX]` folder suffix); `CalibreLibrarySource` always sets `calibre_uuid` and usually `isbn13`; `ManualSource` sets none of the strong keys and relies on `content_hash`.
- Renaming a book inside Calibre changes `content_hash` but not `calibre_uuid` — strong-key match still holds, so reconciliation does not orphan the project.

## AD-022 — Persistence is behind a `ProjectStore` trait

**Decision:** Reads and writes of `project.json` go through a `ProjectStore` trait. The Sprint-2 implementation is `JsonProjectStore` writing one file per project to `$APP_DATA/projects/{join_key}.json`. Future implementations (SQLite, alternative on-disk formats, in-memory test doubles) implement the same trait.

```rust
// src-tauri/src/core/store/mod.rs
pub trait ProjectStore: Send + Sync {
    fn put(&self, p: &Project) -> Result<(), StoreError>;
    fn get(&self, id: &ProjectId) -> Result<Option<Project>, StoreError>;
    fn list(&self) -> Result<Vec<ProjectSummary>, StoreError>;
}
```

**Why:** AD-010 picked JSON-on-disk as the persistence shape; AD-020 added the library index cache. Both remain correct for the current scale (p95 ~50 projects, single-writer per project). Naming the trait now means swapping in SQLite later is additive — every call site already depends on the trait, not on `serde_json::to_writer`.

**Atomic write:** `JsonProjectStore::put` writes to `{join_key}.json.tmp.{pid}.{nonce}`, `fsync`s, then renames over the destination. Rename is atomic on APFS, ext4, and NTFS. Power-cut between write and rename leaves the prior file intact; this is gated by a CI test.

**Forward-compat:** `Project` uses `#[serde(default)]` on every non-id field. A `project.json` written by a future schema_version=2 with new fields deserialises cleanly as the current schema with unknown fields dropped via `#[serde(other)]` on the affected enums.

**Contract test:** a single integration suite is parameterised over `JsonProjectStore` and `InMemoryProjectStore` (test double). Any future implementation must pass the same suite to ship.

**SQLite migration trigger:** as named in AD-020 — when project count crosses ~500, OR when cross-project queries become rich, OR when the first multi-document transactional update bites. None of these apply today; the trait is the seam, JSON is the impl.

**Anti-pattern banned:** `serde_json::to_writer` or `std::fs::write` against `project.json` outside `core/store/`. Surface them via the trait or write a new impl.

## AD-023 — m4b chapter atoms are a track source; ManyToFew uses proportional packing

**Decision:** A single m4b file dropped by the user is **not always one track**. If the file carries embedded chapter atoms (`nb_chapters >= 2` per `ffprobe`), the audio probe expands those atoms into N virtual tracks and the matcher sees the audio as having N tracks, not 1. When the resulting track count is *less* than the text-chapter count (both ≥ 2) the matcher emits a new `MismatchCondition::ManyToFew` and offers a new `MismatchResponse::SplitProportional` that packs text chapters into audio buckets proportional to atom duration.

```rust
// src-tauri/src/core/matcher/mismatch.rs
#[serde(rename_all = "snake_case")]
pub enum MismatchCondition {
    OneToMany,
    ManyToOne,
    ManyToFew,           // NEW: chapters > tracks, both >= 2, ratio > 1.5, ratio <= 30
    CountOff,
    Unalignable,
    #[serde(other)]
    Unknown,             // NEW: serde forward-compat fallback (see "Serde forward-compat")
}

#[serde(rename_all = "snake_case")]
pub enum MismatchResponse {
    PairAccept,
    PairDrop,
    SingleLesson,
    SplitProportional,   // NEW: pack text proportionally across audio atoms
    Cancel,
    #[serde(other)]
    Unknown,             // NEW: serde forward-compat fallback (see "Serde forward-compat")
}
```

**`allowed` rule:** `ManyToFew → ([SplitProportional, SingleLesson, Cancel], SplitProportional)`. Preselect is `SplitProportional` because the embedded atoms are the publisher's own chapter boundaries — almost always the right answer. `Unknown` is **not** a value `allowed()` can be called with — it only appears during deserialisation of unknown-tagged data; treat it as `Unalignable` with response set `[Cancel]` and preselect `Cancel` (force the user to redo the match step on a build that understands the new variant).

**`classify` precedence update:** equal-nonzero → empty-vs-content (Unalignable) → OneToMany → ManyToOne → CountOff → **ManyToFew (chapters > tracks, both ≥ 2, 2·chapters > 3·tracks, chapters ≤ 30·tracks)** → Unalignable. CountOff still wins on the small-delta near-miss case (`|c − t| ≤ 2` with ratio close to 1) so e.g. `classify(22, 20)` keeps returning CountOff — pair-accept of 20 + drop-2 stays the right answer there. ManyToFew is for *genuinely* coarser audio than text: it triggers only when the ratio exceeds 1.5×, which captures (4, 2), (5, 3), (6, 3), (85, 6) but excludes (22, 20), (10, 8), (5, 4). The integer form `2·chapters > 3·tracks` avoids floats; the upper bound `chapters ≤ 30·tracks` is the same sanity guard as before (beyond 30× the packer output quality degrades enough that `SingleLesson` is a better fallback).

**Audio probe is per-file, always.** Same-series files differ — a single Drama-CD folder can mix files with 5 atoms and files with 0. The probe does not cache by folder.

**Atom representation:** `core::audio::ChapterAtom { start: f64, end: f64, title: Option<String> }`. Times are seconds. The integer `start` / `end` + `time_base` triple from ffprobe is discarded; the float `start_time` / `end_time` fields are the contract surface. This isolates the rest of the codebase from `time_base` variance (`1/1000` on Lavf-encoded sources, `1/44100` on Audible delivery).

**`IngestSource` extension is unchanged.** The `IngestSource::audio_tracks` shape per AD-019 + AD-018 stays `Vec<PathBuf>` — adding a slice variant would break every existing impl and consumer. The atom probe + fanout happens **inside** the job orchestrator's `resolve_audio_tracks` step (the single place that converts `AudioSource::SingleFile` and `AudioSource::LibationManifest` into `AudioTrack` records). For a single-file audio source that probes to N ≥ 2 atoms, `resolve_audio_tracks` emits N `AudioTrack` records — one per atom — instead of the current one-per-path. `AudioSource::Folder` stays as-is (one `AudioTrack` per file, no probe).

**`AudioTrack` grows a window field.** `AudioTrack` is the universal track handle across the matcher, transcode, and upload pipeline. Rather than introducing a parallel `TrackRef` type next to it, `AudioTrack` gains `window: Option<(f64, f64)>` where `None` means whole-file (current behaviour, default for all `AudioSource::Folder` entries) and `Some((start, end))` names the slice. This is the single struct change; every downstream caller that already handles `AudioTrack` keeps working with `None`.

```rust
// src-tauri/src/core/audio/mod.rs
pub struct AudioTrack {
    pub order: usize,
    pub path: PathBuf,
    pub duration_sec: f64,
    pub title: Option<String>,
    pub window: Option<(f64, f64)>, // NEW: None = whole file, Some = atom slice
}
```

**Transcode integration:** `core::audio::transcode` gains a fourth arg, `window: Option<(f64, f64)>`. `None` calls the existing path. `Some((start, end))` prepends `-ss <start> -to <end>` to the input. The per-slice mp3 output is still verified against the slice duration (`|slice_dur − mp3_dur| < 1.0 s`) by the existing duration-check codepath. The signature change ripples to 5 call sites (`core/audio/batch.rs`, `core/audio/mod.rs` test, `core/job/mod.rs` production caller, `commands/upload.rs`) — four of them pass `None`; the production caller in `core/job/mod.rs` passes `track.window`.

**Why:** The previous matcher pushed every "single m4b file with internal chapters" case into `SingleLesson` (concat all text into one lesson, audio not chapter-aligned). That's a lossy fallback — the publisher already encoded chapter boundaries; ignoring them produces a worse learning experience. Proportional packing reclaims chapter-alignment whenever the publisher gave it to us.

**Why not re-slice audio:** The packer never splits an audio atom. Atoms are the publisher's own boundaries and the only ground truth we have; subdividing them by text length would introduce mid-sentence cuts. The packer instead concatenates text into buckets sized to match the atoms.

**Why character-count over word-count for text length:** Japanese has no word boundaries. Character count (codepoints, whitespace + markup stripped) is the only portable proxy for narration duration across CJK + Latin-script source material. Variance from dialog vs prose narration is bounded enough for one-pass packing; closer alignment would require speech transcription (out of scope).

**Serde forward-compat:** Both `MismatchCondition` and `MismatchResponse` are persisted inside `MatcherDecision` in `project.json`. Adding `ManyToFew` / `SplitProportional` is a forward-incompat change for older builds reading newer files (the `serde(rename_all = "snake_case")` enums would error on unknown tags) — violating AD-022's "forward-compat" promise. The fix is the `#[serde(other)] Unknown` variants shown in the code snippet above. Old builds reading a `project.json` written by a new build deserialise the unknown tag as `Unknown` and the matcher UI re-prompts the user for a decision the old build understands. **No `SCHEMA_V1` bump is needed**, because the structural shape of `MatcherDecision` is unchanged — only the enum tag set widens. `Unknown` is **not** emitted by the new build's own classifier or UI; it can only enter memory via deserialisation of a file written by an even-newer build.

See `docs/specs/m4b-chapters.md` for the full probe + filter + packer contract, edge cases, and synthetic fixtures.

## AD-024 — Trash subsystem: soft-delete by directory move

**Decision:** Removing a project from the library is a *move*, not an unlink. `core::library::trash::trash_project` renames `<app_data>/projects/<slug>/` to `<app_data>/projects/.trash/<slug>-<unix_ts>/`. `restore_project` reverses the move; `purge_project` runs `fs::remove_dir_all`. The trash root lives inside `projects/` so any backup tool that copies `projects/` also captures the trash.

**Listing:** `list_trash` decodes `project.json` from each `.trash/*/` dir and returns `TrashEntry { trash_id, project_id, title, language, trashed_at }`. `trashed_at` reconstructs from the trash dir's filesystem mtime — slugs may contain `-`, so parsing the trash_id suffix is unsound.

**Read-side invariant:** Every reader of `projects/` except `cmd_list_trash` skips the `.trash` entry. `JsonProjectStore::scan` filters it before the corrupt-file path so trashed projects never count toward dedup, list, or health metrics. `cmd_create_project`'s collision check rides on `store.get` / `store.list`, both of which traverse the filtered scan — re-adding a previously-trashed source path succeeds with no manual cleanup.

**Why soft-delete:** A two-button click ("Trash" then realising it was the wrong row) must not destroy data. With 30-80 projects in a typical library and most actions reversible, an irreversible delete is the wrong default. Soft-delete survives every misclick until the user explicitly purges from the Trash settings panel.

**Why directory move (not OS trash):** A `fs::rename` on the same filesystem is atomic and platform-uniform. `tauri-plugin-trash` adds a dependency and is unaware of project semantics — it would scatter `<id>/project.json` files into the user's recycle bin with no coherent restore path, and Linux recycle-bin behaviour varies. The in-project trash is fully owned by the app, ships zero extra plugins, and round-trips through a single `rename` in either direction.

## AD-025 — Invisible resilience as a user-experience contract

**Decision:** The persisted state machine (`ProjectStage` in `core/project.rs`), the project-level cancellation token map (`commands::jobs::JobCancelMap`), and the atomic `ProjectStore::patch_chapter` write path all exist to make crash recovery *invisible* to the user. The UI never names the recovery event.

**Scope:** the contract covers *user-visible* rendered text — anything reachable by reading the DOM, screen-reader output, or attribute payloads (`aria-label`, `title`, `alt`). Developer-facing prose — source comments, tracing log messages, test names, commit messages — is exempt. Resilience plumbing has to be discussable in code.

**Banned words anywhere a user can read them** — labels, toasts, banners, modal copy, route titles, chip states, error strings surfaced to the surface:

- `crash`, `recover`, `recovery`, `interrupted`, `restored`.
- `resume` outside a user-driven action button (the Start/Resume button on the Run screen is the only allowed usage).

**Allowed concrete behaviours:**

- On app start, the Run screen rehydrates `project.receipts` from `JsonProjectStore::get` and renders them as chips. Receipts with `lesson_id == Some(_)` show as "done"; receipts with `lesson_id == None` show as "queued". The user sees their queue, not a recovery event.
- Cancellation surfaces a single chip-state transition: `running → queued`. No `cancelled` chip variant exists.
- The on-disk lifecycle stage (`ProjectStage::Mapped`, `Transcoded`, `Uploaded`, `Done`) is plumbing — never rendered as user copy. `JobEvent::Result { skipped: true }` for a `Done` re-run silently no-ops the run.

**Verification:** `e2e/invisible-resume.spec.ts` greps the rendered DOM of `/library`, `/add`, and `/run/<id>` for the banned-word list (case-insensitive). Button labels are excluded from the grep via a DOM walk that strips `button, [role="button"]` before reading `innerText`. The spec is mandatory in CI (the `playwright` job on macOS).

**Why it matters:** The librarian's most expensive moment is the first reopen after a crash mid-upload. A modal that says "Recovery required" wins the architecture battle and loses the user — it teaches them the app is fragile. Silence at that moment teaches the opposite: their queue resumes, the chips light up green, and the next chapter starts uploading. The plumbing carries the surprise; the user carries the confidence.

**Consequences:**

- `JobEvent::Cancelled` is a terminal event for the runner but the UI consumes it by calling `reloadProject()` — no dedicated banner, no toast.
- `StoreError::Corrupt` is the single exception. A corrupt `project.json` is a developer-grade failure (not a recovery event) and may surface a loud recovery prompt. All other failure modes route through silent rehydration.
- New routes added in future sprints must run through this DOM-grep spec before merging.

## AD-026 — EPUB vendor autodetect and heading-strategy contract

**Decision:** Strategy selection runs through a single discriminator function `core::epub::detect::detect_vendor`. The result drives `parse_epub_bytes`'s dispatch into `KindleStrategy` or `KoboStrategy`. `EpubVendor::Generic` falls back to `Kindle` (lowest-regression default for unknown books). The chosen vendor is surfaced on `JobEvent::Started.strategy: Option<EpubVendor>` so the UI and logs can show which path ran.

**Kobo classification rule:** a body file (XHTML, not CSS / OPF / NCX) must carry ≥3 combined Kobo marker hits — `koboSpan` and `font-1[246]0per` occurrences accumulate, since both marker families are Kobo-exclusive — or be one of ≥3 distinct body files each carrying a marker. XML and CSS comments are stripped before counting. One stray marker in an otherwise-Kindle book does not flip the verdict.

**Confidence:** `VendorDetection.confidence: f32` is logical [0, 1]. Positive matches land ≥ 0.8; unknown / empty EPUBs land ≤ 0.4. The orchestrator does not gate on confidence today — it only logs — but Generic falls back to Kindle so a misclassification degrades to the prior behaviour.

**`Chapter.id` form:** `ChapterId::from_chapter_parts(strategy, spine_key, title)` hashes `(strategy, spine_href, NFKC-lowercased-whitespace-collapsed title)` via sha256, takes the first 16 hex chars, and emits `"{strategy}:{hex16}"`. Stable across re-parse on the same EPUB bytes and across drops of empty mid-spine chapters (the spine href is the anchor, not the post-filter index). `normalize_title` also strips `Default_Ignorable_Code_Point` characters (ZWJ, ZWNJ, BOM, soft hyphen, variation selectors) so a re-save that inserts or removes them does not flip the id. Legacy `idx:N` strings deserialise as `ChapterId` placeholders; the job orchestrator logs a `tracing::warn!` once per project load when persisted skip ids do not match any parsed chapter.

**Public entrypoint:** the path API is `parse_epub(path) -> Result<Vec<Chapter>, EpubError>`; it always autodetects internally. `parse_epub_with_strategy` exists for tests only — production callers cannot bypass autodetect.

**Reversibility:** `EpubVendor` is a public IPC type (`specta`-exported). Adding variants is additive (TS bindings see a wider union). Renaming variants is a frontend break.

**Where it lives:**
- `src-tauri/src/core/epub/detect.rs` — discriminator + cluster heuristic.
- `src-tauri/src/core/epub/{kobo.rs, parse.rs}` — strategies (Kindle parser lives inside `parse.rs`).
- `src-tauri/src/core/epub/mod.rs` — `autodetect_vendor`, `autodetect_vendor_bytes`, `parse_epub`, `parse_epub_bytes`, `ChapterId::from_chapter_parts`.
- `src-tauri/tests/epub_vendor_detect.rs` — heuristic gates.
- `src-tauri/tests/epub_kobo_parse.rs` — Kobo strategy snapshot.
- `src-tauri/tests/epub_kindle_regression.rs` — Kindle baseline snapshot.

**Consequences:**
- A future vendor (e.g. Apple Books, Adobe) adds a new `EpubVendor::*` variant plus a `*Strategy`. The autodetect heuristic grows by one branch.
- User override of vendor detection is deferred — the orchestrator logs the chosen strategy so field reports can identify false positives before we add a UI knob.

## AD-027 — Chapter-divider absorb policy is a pure carver function

**Decision:** `core::audio::carver` is a pure-function module. `boundaries_from_silences` maps detected `SilenceRun`s to `Boundary` records per `AbsorbPolicy`; `carve` adds the ffmpeg `silencedetect` driver. The orchestrator does not yet consume the output — wiring lands in a follow-up. The on-disk `Project::absorb_policy` field is persisted today so user intent survives until the wire-in.

**Boundary shape:** every `Boundary` carries a `BoundaryKind`:

- `Forward` / `Backward` emit one `BoundaryKind::Cut` per silence run (silence is absorbed into the next / previous chapter).
- `Drop` emits a paired `(BoundaryKind::DropStart, BoundaryKind::DropEnd)` per silence run, sharing one `track_index`. The span between them is excluded from both neighbours. The tagged form lets the future carve consumer distinguish "start of cut" from "end of cut" without re-deriving from position.

**Defaults:** `CarveOpts::silence_db = -45.0`, `min_silence_ms = 500`, `absorb = Forward`. -45 dB sits between hard-zero and the -50…-60 dB noise floors typical in audiobook narration; tighter than -30 dB which mis-detects on real material.

**Persistence:** `Project::absorb_policy` (`#[serde(default)]` → `Forward`) is mutated by `cmd_set_absorb_policy`. The Tauri command is debounced from the Svelte radio and silently reverts on error (AD-025). Until the carver is wired into the job runner (open question 5), the radio renders disabled with an inline hint — persisted intent must not masquerade as runtime behaviour.

**Fixtures:** `src-tauri/tests/fixtures/audio/silence_corpus/clip_{a,b}.wav` are immutable test inputs pinned by sha256. Regenerate via `scripts/fixtures/gen_silence_corpus.sh`. The ffmpeg-backed integration tests are `#[ignore]`-flagged and opt-in via `cargo test -- --include-ignored`; setting `LINGQ_E2E_AUDIO=0` skips them when run that way.

**Golden contract:** the offsets JSONs pin **detected silence edges** as reported by ffmpeg `silencedetect`, not authored midpoints — detection skew makes authored-timing assertions brittle across ffmpeg versions. Each golden records the generating ffmpeg version as an informational field; a version-bump-induced diff is a tripwire to re-inspect, not an automatic regeneration.

## Open architecture questions

1. **Re-import / diff behaviour** — when the same Candidate is re-scanned (Calibre edit, Libation re-rip), what's the per-chapter conflict policy? Overwrite / append / skip / prompt? Current default: append-by-default, prompt on conflict.
2. **Log persistence** — `tracing` to file alongside `project.json`? Or in-memory ring buffer surfaced via a `JobEvent::Log` stream? Decide before resumability work begins.
3. **i18n of the app UI itself** — currently English-only; `lang` is per-project, not per-app. The target user is a multi-language learner, so this is likely worth doing.
4. **Local lesson cache scope** — the persistence layer captures full parsed lessons locally so future on-device study features stay viable without immediate scope expansion. Schema: in `project.json` `parsed.lessons[]`, or in a sibling `lessons/` directory of chunked files? Decide before any consumer depends on the shape.
5. **Wire `core::audio::carver` into the transcode pipeline** — `Project::absorb_policy` is persisted today and the UI mounts the chooser, but `core::job::run_project_job` never calls `carve()`. The pure function exists; the consumer does not. Either wire the carver into the job runner or remove the chooser from the UI until the policy actually influences output. Tracked in AD-027 ("wiring lands in a follow-up").
6. **Real Kobo EPUB fixture coverage** — `KoboStrategy` is currently tested against synthetic zips built in-test plus one committed adversarial fixture. Acquire 2–3 real Kobo books across publishers, store under `src-tauri/tests/fixtures/epub/kobo/`, pin snapshots by sha256, and assert against them. First field report of a misclassified Kobo book reopens this gap.
7. **Cover-image upload outcome (AD-028)** — the cover-image API spike was deferred because it requires live LingQ credentials on-hand. AD-028 (cover-image upload outcome) is unwritten. When credentials are available, run the `POST` / `PATCH` / sub-resource probe hypotheses in order against a dev LingQ account, write AD-028 with the chosen path (or "no path found"), and add the contract test (`src-tauri/tests/lingq_cover_contract.rs`) pinned by mockito cassette. Until then, cover upload is silently absent — the cover ring renders Green from local lesson state regardless.
