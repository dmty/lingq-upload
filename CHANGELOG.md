# Changelog

## [0.1.4](https://github.com/dmty/lingq-upload/compare/v0.1.3...v0.1.4) (2026-06-28)


### Bug Fixes

* **ci:** drop empty APPLE_* env to skip signing path ([c68e9bd](https://github.com/dmty/lingq-upload/commit/c68e9bd06052c2a63da5983a5126019c14ba6f56))

## [0.1.3](https://github.com/dmty/lingq-upload/compare/v0.1.2...v0.1.3) (2026-06-28)


### Bug Fixes

* **build:** move gen_bindings out of src/bin so tauri bundler skips it ([960090b](https://github.com/dmty/lingq-upload/commit/960090b3d3b3975ba1a61aa2c6f68a973e421bb8))
* **ci:** keep parent ref when amending release-please commit ([a7cbe1c](https://github.com/dmty/lingq-upload/commit/a7cbe1cbe572c8d61b12dfecfb4d1ba134462eed))

## [0.1.2](https://github.com/dmty/lingq-upload/compare/v0.1.1...v0.1.2) (2026-06-28)


### Bug Fixes

* **ci:** defer pr-json parse until sync step runs ([76b8bfd](https://github.com/dmty/lingq-upload/commit/76b8bfd34cb8d4fc8c218cc14a585cf6b0893da7))

## 0.1.1 (2026-06-27)

First release. End-to-end: EPUB + audio → LingQ courses.

### Features

- **EPUB ingest** — Kindle, Kobo, and generic vendors; furigana strip; NCX/nav chapter grouping.
- **Audio ingest** — single `.m4b` (embedded chapter atoms) or per-chapter folder of files.
- **Mapping screen** — chapter ↔ audio visualisation; reassign / move / remove; confirm gate before upload.
- **Carve + transcode** — `silencedetect`-driven per-chapter carving, MP3 transcode for LingQ.
- **LingQ upload** — v3 API, lesson `private`/`public` flip, `books` tag on collections.
- **Library** — list / trash / restore / purge projects.
- **In-app updater** — auto-check on launch, native prompt, install + relaunch.

### Notes

- macOS universal `.dmg` only. Windows and Linux planned.
- Unsigned build — strip quarantine on first launch: `xattr -d com.apple.quarantine /Applications/lingq-upload.app`.
- Requires `ffmpeg` on PATH (`brew install ffmpeg`) until bundling lands.
