# Glossary

Plain-English term list. Update whenever the codebase coins a new term or repurposes a borrowed one.

## Domain

- **Audiobook** — an EPUB + an audio folder + (optionally) a cover image. Input to the importer.
- **EPUB** — the book file in EPUB 2 or 3 format. Contains XHTML chapters, metadata, optionally embedded fonts and images.
- **Track** — a single audio file in the input folder (`.m4b`, `.m4a`, `.mp3`, …). Each track maps to one or more EPUB headings.
- **Heading** — a chapter or section marker extracted from the EPUB by a `HeadingStrategy`. Has a level (`L1` / `L2` / `L3`) and a position in the spine.
- **Mapping** — the user-confirmed association of audio tracks to EPUB headings. Fuzzy-matched by default, editable in the Mapping Editor.
- **Carve / Carving** — the process of slicing chapter text into per-track segments based on the mapping. Produces a `Lesson` per track.
- **Furigana** — small phonetic annotations (`<ruby>` / `<rt>`) above Japanese kanji. Stripped before upload so the text shows raw kanji only.

## LingQ-specific

- **Course** — LingQ's user-facing container for a series of lessons. The app calls LingQ's `/collections/` endpoints to manage these. (LingQ web UI says "course"; API says "collection". We use both; treat as synonyms.)
- **Collection** — LingQ API term for a course. Same thing.
- **Lesson** — a single text + audio unit in a LingQ course. One uploaded mp3 + the chapter-segment text. The app calls `/lessons/import/` per track.
- **Level** — LingQ's beginner-to-advanced scale (`1` Beginner-1 through `6` Advanced-2). Set per lesson at upload time; defaulted per `LanguageProfile`.
- **Status** — `private` (only the uploader sees it) or `public` (visible in LingQ's library). Default `private`.

## App / engineering

- **Project** — a single audiobook-import workspace. Tracks state across stages and persists to `project.json`. Resumable.
- **Stage** — a state-machine node in a Project: `New | Parsed | Mapped | Carved | Transcoded | Uploaded | Done`.
- **Job** — a long-running async task on the Rust side. Identified by a `JobId` (UUID); emits `JobEvent`s the frontend listens to.
- **JobEvent** — a typed message on the `"job"` Tauri channel. Variants: `Started`, `Progress`, `Log`, `Result`, `Cancelled`. See AD-007.
- **AppError** — single discriminated `thiserror` enum returned from every `#[tauri::command]`. See AD-006.
- **Strategy** — a pluggable trait impl: `AudioCodec` (AD-014), `LanguageProfile` (AD-015), `HeadingStrategy` (AD-016), `IngestSource` (AD-019). The four extension surfaces.
- **Profile** — synonym for `LanguageProfile`. The bundle of per-language behaviour (text normalisation, fuzzy metric, defaults).
- **Spike** — a timeboxed research task whose deliverable is a documented decision, not shippable code. Used for probing undocumented external APIs.

## Upstream tools

- **Calibre** — open-source ebook library manager. The user's source of truth for ebooks: stores EPUBs alongside `metadata.opf` files with title, authors, language, series, `series_index`, tags, cover, ISBN. Treated as an `IngestSource` (AD-019). See `docs/specs/calibre-opf.md`.
- **Libation** — open-source tool that decrypts purchased Audible audiobooks into `.m4b` / `.m4a` files plus a JSON sidecar with chapters, narrators, ASIN, series position. The user's source of truth for audio. Treated as an `IngestSource` (AD-019). See `docs/specs/libation-sidecar.md`.
- **OPF** (Open Packaging Format) — the XML metadata file format Calibre uses per book (`metadata.opf`). Subset of the EPUB spec.
- **ASIN** (Amazon Standard Identification Number) — Audible's per-title identifier. Surfaced by Libation; useful for cross-source pairing (AD-019).
- **Series** — a Calibre-modelled grouping (`<meta name="calibre:series" …>`) with a `series_index` for sort order. The Library UI groups books by series first.

## Domain (extended)

- **Candidate** — a normalised "could be imported" record produced by an `IngestSource`. Contains title, authors, language, series, cover, text source, audio source, chapter manifest, extras. Pre-confirmation; the user picks which become projects.
- **Reconciliation** — the act of matching Calibre candidates to Libation candidates (or any two ingest sources) for the same logical book. Lives in `core::library::reconcile`. Key priority: ASIN → normalised (title, author) → fuzzy ratio.
- **Ingest source** — a pluggable provider of `Candidate`s. Built-in implementations: `ManualSource`, `CalibreLibrarySource`, `LibationFolderSource`. See AD-019.
- **Library index** — `library.index.json` at `~/.lingq-importer/`. Denormalised catalog cache; rebuildable from `project.json` files. See AD-020.
- **Selective import** — the default workflow: open a known book in the Library, tick the chapters you want, hit Import. The wizard is reserved for unknown / loose files.

## Build / tooling

- **specta / tauri-specta** — Rust → TypeScript type-codegen used to keep IPC types in sync. AD-004.
- **mockito** — Rust HTTP mock library used to replay LingQ cassettes in CI without hitting the live API.
- **insta** — Rust snapshot-testing library used to gate carved-text byte equality against golden output.
- **Cassette** — a recorded HTTP request/response pair captured against the live LingQ API and replayed by mockito in CI. Tier-1 contract-drift detection.
