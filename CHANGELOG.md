# Changelog

## [0.2.0](https://github.com/dmty/lingq-upload/compare/v0.1.4...v0.2.0) (2026-06-29)


### Features

* custom app icon (book-upload speech bubble) ([3ac6a5f](https://github.com/dmty/lingq-upload/commit/3ac6a5ff65a630abe0dc816dfd7859deb7034e08))
* mp3 encoder + deterministic regression golden ([014ff7d](https://github.com/dmty/lingq-upload/commit/014ff7d5e2a589601f86cbc1d530daa8e409e6e1))
* mp4 chapter atom reader (nero chpl + qt fallback stub) ([e81acc9](https://github.com/dmty/lingq-upload/commit/e81acc98bb66d3da724c1a4064335aaaf644e28b))
* pure-rust windowed-rms silence detector ([56154cc](https://github.com/dmty/lingq-upload/commit/56154cc13183595e6ea533ce0440d879f70b9374))
* scaffold codecs module with AudioDecoder/AudioMetadata traits ([6c261ed](https://github.com/dmty/lingq-upload/commit/6c261ed665848aa974df691d567bac1c1c5380e7))
* symphonia decoder + duration probe ([b1f7487](https://github.com/dmty/lingq-upload/commit/b1f748739d12991b8b76ee47ad5e25a3a7b462c2))
* wire SymphoniaMetadata::probe_chapters through mp4 reader ([b0da128](https://github.com/dmty/lingq-upload/commit/b0da128089ce39e702b7c25b5d43d03a4dcc4dbd))


### Bug Fixes

* **audio:** probe AAC spec via first-packet decode when stsd lacks channels ([f7f7cda](https://github.com/dmty/lingq-upload/commit/f7f7cdaae544b633f5c3052819f7bc4626482369))
* **audio:** read QuickTime chapter tracks for m4b (Audible-style) ([428b881](https://github.com/dmty/lingq-upload/commit/428b881587da02a9f3f5c032be15c861efd13def))
* **audio:** replace prod unwrap() with expect() for infallible paths ([aca9760](https://github.com/dmty/lingq-upload/commit/aca97606e9ee476711e411dfcfbe8dffe46b868e))
* **deny:** scope LGPL exception to mp3lame-encoder + mp3lame-sys ([3102918](https://github.com/dmty/lingq-upload/commit/310291811ff87e24b9a4b76b79c232c1e5656fd1))
* **mapping:** alias .m4b to .m4a symlink instead of windowed transcode ([d0a4386](https://github.com/dmty/lingq-upload/commit/d0a438677c181afff5250a2ed8039a6a1f904655))
* **mapping:** give inspector &lt;audio&gt; a MIME hint so .m4b plays ([05ba029](https://github.com/dmty/lingq-upload/commit/05ba029063b1a05a608c140d75d8dc7bdc606196))
* **mapping:** keep orphan buckets in audio order, allow tail-band moves ([5069026](https://github.com/dmty/lingq-upload/commit/50690263997a6cf557440ef1a90c1f9c95d619fd))
* **mapping:** seek inspector audio before play() to avoid AbortError ([735d18e](https://github.com/dmty/lingq-upload/commit/735d18eeabf538e9631d8d02f9f63e1461725e04))
* **mapping:** serve inspector audio via custom audio:// URI scheme ([76bbd76](https://github.com/dmty/lingq-upload/commit/76bbd76e34bdb2981631aac1eaa2170800788f7d))
* **mapping:** transcode inspector preview to MP3, bypass asset:// MIME ([7957dfd](https://github.com/dmty/lingq-upload/commit/7957dfd8c8c7efd2d50d4524b588ae1905706cb5))
* **orchestrator:** exclude tracks paired to skipped chapters from audio-only ([3add521](https://github.com/dmty/lingq-upload/commit/3add52194dcfb5802a6131c837f79e0325e1b574))
* round app icon corners (rx=225) ([e3ef278](https://github.com/dmty/lingq-upload/commit/e3ef27823e227738519e02d2057e9f849a16efa9))
* **tests:** point mp4 chapter fixtures at src-tauri/tests not stray top-level ([28ac93c](https://github.com/dmty/lingq-upload/commit/28ac93c8225bedaf85883ffb8894b6fb91a1668f))


### Performance Improvements

* **epub:** fix O(n²) UTF-8 revalidation in chapter body cleaning ([61a3f90](https://github.com/dmty/lingq-upload/commit/61a3f906a075607663c699b0f9acede1487e5ba7))

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
