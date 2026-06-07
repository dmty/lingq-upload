# lingq-upload

Desktop app that imports audiobooks (EPUB + audio folder + cover) into [LingQ](https://lingq.com) as language-learning courses. Tauri 2 + Rust + SvelteKit 5 + Bun.

## 30-second orientation

- **What it does:** Parse EPUB → match audio tracks to chapters → strip furigana → carve text per track → transcode to mp3 → upload to LingQ via v3 API. One installable app replacing a pile of one-off Python scripts.
- **Architecture decisions:** [`docs/architecture/decisions.md`](./docs/architecture/decisions.md) — 18 ADs, evergreen.
- **Diagrams:** [`docs/architecture/diagrams/`](./docs/architecture/diagrams/) — component / sequence / state.
- **Specs:** [`docs/specs/`](./docs/specs/) — LingQ API surface, EPUB subset, glossary.

## Dev quickstart

```sh
bun install
bun tauri dev
```

Tauri 2 on macOS / Windows / Linux. Bun-only — no npm / pnpm. Rust toolchain via rustup. ffmpeg located via `FFMPEG_BIN` env var in dev; bundled in release builds.

## Repository map

```
src/                 SvelteKit frontend (Svelte 5 runes, SPA mode)
src-tauri/           Rust backend (crate: lingq_upload_lib, bin: lingq-upload)
docs/                Evergreen architecture + specs
static/              Static frontend assets
```

## End-to-end smoke

Playwright lives at the repo root (`playwright.config.ts`, tests under `e2e/`).
The live LingQ flow is gated behind env vars so CI stays green without
credentials.

```sh
bun run e2e                                                  # skipped (no live key)
LINGQ_LIVE=1 LINGQ_STAGING_KEY=… bun run e2e                 # runs the smoke
```

Playwright boots `bun run dev` via its `webServer` hook; no separate dev
server needed.

## License

MIT.
