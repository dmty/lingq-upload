# Changelog

## [0.4.0](https://github.com/dmty/lingq-upload/compare/v0.3.0...v0.4.0) (2026-08-04)


### Features

* add course detail command and surface missing-key as a typed error ([8c5fc31](https://github.com/dmty/lingq-upload/commit/8c5fc312554b93cda637915e4ecd47991e79adf2))
* add course detail screen with LingQ corpus stats ([e9bc6ce](https://github.com/dmty/lingq-upload/commit/e9bc6ce9451db0b276ed5e564736aa7f39af87a5))
* cache course stats for fifteen minutes with an explicit refresh ([7679adf](https://github.com/dmty/lingq-upload/commit/7679adf1f9301dcadfc45a481511e0a6e2aee3de))
* fetch LingQ collection detail ([3b98f67](https://github.com/dmty/lingq-upload/commit/3b98f67625f4731cdffcc5f92bfd14cf1404bd02))
* open finished courses in the app instead of the browser ([81d6858](https://github.com/dmty/lingq-upload/commit/81d685822fec93a2ef06e512855b429f9c502215))
* project per-lesson stats from the LingQ lesson list ([5c90042](https://github.com/dmty/lingq-upload/commit/5c90042d57b87fd2c00b29a0aacb9cfab5a6f6af))
* show weighted course progress and per-lesson stat rows ([346677c](https://github.com/dmty/lingq-upload/commit/346677c6df4161e12ca1212aa21397f3d573b7c7))


### Bug Fixes

* assert the LingQ URL, correct duration rounding, drop doc-comment slashes ([cfc9693](https://github.com/dmty/lingq-upload/commit/cfc9693debb192db3c9ec813504ae2f5d93320e8))
* distinguish a library read failure from a missing course, tighten error-path tests ([a937a87](https://github.com/dmty/lingq-upload/commit/a937a878206e60925d068436f940e91957bf3cb0))
* keep the course screen usable when LingQ is unreachable ([935e357](https://github.com/dmty/lingq-upload/commit/935e3578e70631c0fe4f3d8f16201fc276235c14))
* weight lessons without a word count instead of dropping them, add lesson row headers ([61dd157](https://github.com/dmty/lingq-upload/commit/61dd1575c9cca57fa74acd39436fe59164e3d256))

## [0.3.0](https://github.com/dmty/lingq-upload/compare/v0.2.0...v0.3.0) (2026-08-02)


### Features

* add plan preview command for the run queue ([ad0a970](https://github.com/dmty/lingq-upload/commit/ad0a97014a19e5be86d573a47295676928a193c7))
* **add:** explain disabled create, retry language load, flag unusable drops, destructive replace styling ([b929684](https://github.com/dmty/lingq-upload/commit/b92968488592bf1a7b456b85be475c0a2741de99))
* **chapters:** suppress cover-host XHTML from chapter list ([e82ffa2](https://github.com/dmty/lingq-upload/commit/e82ffa28acabb60cf28c07a3467db3cadf0313de))
* **commands:** cmd_set_cover copies into project dir; add cmd_set_cover_use ([b05ebcd](https://github.com/dmty/lingq-upload/commit/b05ebcde327b8535e7658122c50d2cdc8c90a78b))
* **epub:** extract cover image from EPUB to project sidecar ([1dd4f24](https://github.com/dmty/lingq-upload/commit/1dd4f24204d0b70a048bf5af4ec08ebbe87fe302))
* **errors:** human-first error copy and a Settings link for missing API key ([938c6fb](https://github.com/dmty/lingq-upload/commit/938c6fbc97b6c1958062323ea86ad3429c2ca72f))
* expose the upload plan's step list for preview ([19231ae](https://github.com/dmty/lingq-upload/commit/19231aeddea09fb486d749a8b54ad3a40cb92c98))
* **ingest:** auto-extract EPUB cover when no sidecar present ([956e93a](https://github.com/dmty/lingq-upload/commit/956e93a4057f05c5e9d47e36b5d7823ccf2e1802))
* **inspector:** empty-state prompt, visible audio errors, drop debug logging ([73b9c61](https://github.com/dmty/lingq-upload/commit/73b9c61360e9f08f0a939b4f50751049a1427317))
* **lingq:** add set_collection_image with three-probe cascade ([fb91b78](https://github.com/dmty/lingq-upload/commit/fb91b78ccc060dd3682db437bc1429cd11a221ae))
* **mapping:** always-visible move arrows, inline continue-gate reason, row hover ([c8f837a](https://github.com/dmty/lingq-upload/commit/c8f837a4dd863c4f51f26d0683f7e9f725fd25ba))
* **mapping:** park tracks via band-header button and show titles in the lot ([f4ef140](https://github.com/dmty/lingq-upload/commit/f4ef140baa84bf931ba29e0094b3d7cc2aa55498))
* **mapping:** restore removed chapters individually ([83daa33](https://github.com/dmty/lingq-upload/commit/83daa33db637ff316f492a1f54bfe1d880f9cc9c))
* **mapping:** transient footer notice when a save is rejected and reverted ([dc51e29](https://github.com/dmty/lingq-upload/commit/dc51e29de1aec2cc964226f130d79a118792393e))
* **match:** explain the mid-run redirect with an upload-paused notice ([29e119e](https://github.com/dmty/lingq-upload/commit/29e119e2759445e0a0fcad0a910bb4b3c803d18a))
* **nav:** promote quick upload to the header, retire the settings burial ([1752b1d](https://github.com/dmty/lingq-upload/commit/1752b1d1cb2280ef717c2568fdcb034856f667a9))
* **nav:** step indicator orients the add-match-upload pipeline ([d5a805a](https://github.com/dmty/lingq-upload/commit/d5a805a64e7f37f23825fdf7050864712276ee40))
* **project:** add cover_use, cover_uploaded_to_lingq, cover_source_href ([41fdcbf](https://github.com/dmty/lingq-upload/commit/41fdcbf5872380a2e0ddf50148ae7b2bdf6c5778))
* **run:** completion banner with LingQ link, chapter counter, pending cancel, back link ([e94800e](https://github.com/dmty/lingq-upload/commit/e94800e534eaa96c4f863e4f2759ecbd81b67d55))
* **settings:** confirm before clearing the API key, longer saved feedback, clipboard failure message ([08fd1dc](https://github.com/dmty/lingq-upload/commit/08fd1dc9c556b746d9aa8e2a510b3ecc286b172d))
* show the chapter in flight and an aggregate run progress bar ([8ad1c69](https://github.com/dmty/lingq-upload/commit/8ad1c69c2d37886128a15c3489a574a2a2c1a5f0))
* **theme:** bundle Literata for reading surfaces, drop phantom Inter ([a9ff105](https://github.com/dmty/lingq-upload/commit/a9ff105d2350b4b4dce565d0bf3efa08dad99b48))
* **theme:** dark palette via prefers-color-scheme token overrides ([9042e55](https://github.com/dmty/lingq-upload/commit/9042e55a261e0e82cf62cdf6618ca89b5083d7ef))
* **theme:** warning-soft token, shadow-card normalization, ProjectSettings palette vars ([d1b2e2f](https://github.com/dmty/lingq-upload/commit/d1b2e2ff1cc961836982a3f77bdd5b18eb3160d5))
* **ui:** add cover toggle and clear button on mapping screen ([e4bb6a1](https://github.com/dmty/lingq-upload/commit/e4bb6a1cdabb3a4888125ead7d2623a866e30d2a))
* **ui:** display-name language filter and sentence-case status badges ([b8735c6](https://github.com/dmty/lingq-upload/commit/b8735c679afbd60f9876f2f069d5b3a1cbe440aa))
* **ui:** shared Alert unifies error/warning/success banners with proper roles ([93c4294](https://github.com/dmty/lingq-upload/commit/93c4294a73f51c491387c9bcc22d8d93e39e82b4))
* **ui:** shared Button with primary/secondary/danger variants across all form actions ([61a78e0](https://github.com/dmty/lingq-upload/commit/61a78e0b345ebb9b9d2753ab527a3508a08b3d6c))
* **ui:** shared Spinner replaces glyph and inline-circle loaders ([2a534cd](https://github.com/dmty/lingq-upload/commit/2a534cdbf6ffb2d6271f9487602fbe27a8a70bff))
* **upload:** aggregate three-stage progress with step position label ([4fdad06](https://github.com/dmty/lingq-upload/commit/4fdad064df3789d19a003d84c4c3b3d974463da1))
* **upload:** cancel button for in-flight one-shot uploads ([9fb9c13](https://github.com/dmty/lingq-upload/commit/9fb9c1381b70aa0f03f40524147f6b5356c0de82))
* **upload:** set LingQ course cover after collection create (soft-fail) ([1282207](https://github.com/dmty/lingq-upload/commit/1282207bff141f818ab90b23281dcd8390d2ebe3))


### Bug Fixes

* address final-review polish (picker/backend ext mismatch, cover_use drift, strict ext rejection) ([243044d](https://github.com/dmty/lingq-upload/commit/243044d07a299bcd5ed406a27c82f864561ca459))
* **clippy:** drop needless Ok(...?) wrapper in add_project copy-name branch ([9f87615](https://github.com/dmty/lingq-upload/commit/9f876159e41279e4f6ce3b30ed4dca49be475194))
* derive leftover receipt index from max chapter order, not chapter count ([13abf9c](https://github.com/dmty/lingq-upload/commit/13abf9c91f4fc4bbc39eec5a898b9ebce45de417))
* **epub:** cover extraction also handles namespaced OPF manifest ([693cbe0](https://github.com/dmty/lingq-upload/commit/693cbe0e77856336023f5183b21517043825cee4))
* **epub:** handle namespace-prefixed OPF manifest elements (Sigil/Calibre) ([378d454](https://github.com/dmty/lingq-upload/commit/378d4544ebbfdbdfed3371bcb7571b33a2f68b49))
* **epub:** strip &lt;head&gt; block so book title doesn't leak into chapter body ([8f4548e](https://github.com/dmty/lingq-upload/commit/8f4548e81b4b0b4239c5bf090ae16af06b8b5d09))
* give the in-flight chapter row an accessible name ([653c8c0](https://github.com/dmty/lingq-upload/commit/653c8c074014c23ac225da8e970750475cd8316b))
* latch plan fetch on attempt not result, align stub fixture naming ([4d0e3b1](https://github.com/dmty/lingq-upload/commit/4d0e3b19668afeac92a23d129fac8cb22c9d5a0f))
* **library:** reset keyboard focus index when search or filter changes ([dbf7f56](https://github.com/dmty/lingq-upload/commit/dbf7f56c3f3bb92c1fee7f2dc412ee9fbaa169ae))
* **match:** cover controls disable on their own busy flag ([a30374e](https://github.com/dmty/lingq-upload/commit/a30374e7fa09e76693b969c838d129b225bcb2c0))
* report the uploading stage so no row claims to be in flight early ([e3f2bc4](https://github.com/dmty/lingq-upload/commit/e3f2bc4d9dbd7feb0983b87ad53a0633a9f654e1))
* reset run cancel without a job stream and scope upload events to the bound job ([2ca96f8](https://github.com/dmty/lingq-upload/commit/2ca96f854c3f00389048aec44c94937a795041fc))
* seed plan preview's mapping and propagate build failures like the job ([8358c0a](https://github.com/dmty/lingq-upload/commit/8358c0a64b452e5b17f4453c10fde0db596ff4ca))
* seed run rows from the upload plan so progress has a real total ([903ee39](https://github.com/dmty/lingq-upload/commit/903ee39adeab5f1240e70e0d6f6e73c095acd09d))
* thread leftover_base through plan_from_mapping to avoid index collisions ([b571210](https://github.com/dmty/lingq-upload/commit/b571210768393b401053f6a79bb0382f8e0019a7))
* track StageChanged progress, fix Alert class conflict, unify status labels ([8f40b6a](https://github.com/dmty/lingq-upload/commit/8f40b6a3809453009d8f5d12b5cc43eeff8b2ebf))
* **ui:** real window title and a back link from the mapping grid ([4dacf54](https://github.com/dmty/lingq-upload/commit/4dacf5483aa458abb68705b3f0f7b555b5ed49cf))
* **ui:** spinner size and tone via props to survive utility-order conflicts ([bf64d1e](https://github.com/dmty/lingq-upload/commit/bf64d1eaf5e160701ac95defaafc9bf7116dcd46))
* **upload:** surface upload errors instead of a stalled progress panel ([d8b4fd8](https://github.com/dmty/lingq-upload/commit/d8b4fd891b24445ebbd554874c394a927d789ca4))

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
