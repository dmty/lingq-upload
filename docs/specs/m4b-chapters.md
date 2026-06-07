# m4b embedded chapter atoms

> Status: **planned**. Captures the runtime contract that AD-023 commits to. Probe behaviour, filter rules, packer algorithm, and edge cases are frozen here so the implementation can be checked against a single source of truth.

## What this spec covers

When the user supplies a single `.m4b` (or `.m4a`, `.mp3`) file as the audio input, the file *may* carry embedded chapter atoms — the publisher's own per-chapter time markers. This spec defines:

1. How we discover those atoms (the **probe**).
2. How we filter junk atoms (the **filter rules**).
3. How we present the result to the matcher (the **track contract**).
4. How we split text across audio when the counts do not align (the **proportional packer**).

This spec does **not** cover folder-based audio sources (Libation default delivery, raw folders) — those continue to flow through `IngestSource` per AD-019, one track per file. Probe + atom expansion is only ever applied to a single-file audio input.

## Audio populations in the wild

Four shapes observed in user material. Implementation must handle all four without branching on source.

| Population | Examples | `nb_chapters` | `time_base` | Atom title style | Routing |
|---|---|---|---|---|---|
| **A. Per-track delivery** | Libation default, raw folders | n/a (multiple files) | n/a | n/a | One track per file; probe never runs |
| **B. Single-file, narrative atoms** | Audible merge-mode, audiobookjp non-Audible | 9–27 | `1/44100` (Audible) / `1/1000` (Lavf-encoded) | Narrative (`第　２４　章`, `第一章 初めての友達`) | Probe → atom expansion → N virtual tracks |
| **C. Single-file, generic-indexed atoms** | Kotonoha standalone, drama CDs | 5–7 | `1/1000` | Generic (`Track1`, `<book> - 6`) | Probe → atom expansion → N virtual tracks |
| **D. Single-file, no atoms** | Honzuki Drama CD7/8/9, short single-file books | 0 | n/a | n/a | One track (whole file) |

Populations B and C are indistinguishable from the matcher's point of view — atom title style is informational only. Order is the contract.

## Probe

```rust
// src-tauri/src/core/audio/probe.rs
pub fn probe_chapters(path: &Path) -> Result<Vec<ChapterAtom>, AudioError>;

pub struct ChapterAtom {
    pub start: f64,           // seconds
    pub end: f64,             // seconds
    pub title: Option<String>,
}
```

Implementation shells out to `ffprobe -v error -show_chapters -show_format -print_format json <path>` and parses the JSON.

**Contract surface:** the parser reads `chapters[].start_time` and `chapters[].end_time` (both float seconds) and `chapters[].tags.title` (optional). The `chapters[].start`, `chapters[].end`, and `chapters[].time_base` integer triple is **discarded** — different sources use `1/1000` (Lavf-encoded) or `1/44100` (Audible delivery), and the float seconds fields are emitted correctly regardless of which one ffprobe chose.

**Empty result is normal.** Population D (no atoms) returns `Ok(vec![])`. The caller treats this as "whole file is one track."

**Errors are fatal to ingest, not silent.** If `ffprobe` exits non-zero, missing, or returns unparseable JSON, return `AudioError::ProbeFailed` with the path + stderr. Do not fall back to whole-file silently — the user must see the probe failure.

## Filter rules

After parsing, filter the raw atom list before exposing to the matcher.

1. **Duration threshold.** Drop atoms whose duration is below `max(6.0, total_duration / atom_count / 10)` seconds. The dynamic term catches tiny intro / branding atoms (observed: a 41 s "第1章" preamble in a 21,961 s file where every real chapter is > 2,000 s; dynamic threshold ≈ 244 s drops it). On real-world audiobooks (≥ 6 atoms × ≥ 600 s) the dynamic term is always > 60 s and the floor is a no-op. The floor only bites on very short files (synthetic fixtures, short Drama-CDs where some real atoms are ~200 s) and is set to 6.0 s so it filters obvious sub-10 s branding atoms without consuming legitimate short atoms.
2. **Contiguity check, epsilon-tolerant.** The last surviving atom's `end` must be within `0.05 s` of `format.duration`. Float drift up to 0.014 s observed in a 56,402 s file. If the gap exceeds the epsilon, **do not drop atoms** — emit a `AudioWarning::AtomCoverageGap` event so the user sees it. The atoms are still usable; the gap is just informational.
3. **No re-ordering, no merging.** Atoms come out of ffprobe in file order; that order is the contract. Adjacent atoms with the same title are not merged (they may represent intentional sub-sections).

## Track expansion

The atom probe + fanout happens inside the job orchestrator's `resolve_audio_tracks` step (`core::job::resolve_audio_tracks`), the single place that converts `AudioSource` variants into `AudioTrack` records. `AudioTrack` grows one new field:

```rust
// src-tauri/src/core/audio/mod.rs
pub struct AudioTrack {
    pub order: usize,
    pub path: PathBuf,
    pub duration_sec: f64,
    pub title: Option<String>,
    pub window: Option<(f64, f64)>, // NEW: None = whole file, Some = atom slice in seconds
}
```

`resolve_audio_tracks` behaviour per `AudioSource` variant:

- **`AudioSource::Folder`** — one `AudioTrack` per file, `window: None` for every track. **No probe.** (Folder delivery is already per-track per population A; probing each file for atoms is wasted work.)
- **`AudioSource::SingleFile { path }` / `AudioSource::LibationManifest { path }`** — run `probe_chapters(&path)`:
  - Zero surviving atoms → one `AudioTrack` with `window: None`. (Population D, current behaviour preserved.)
  - One surviving atom → one `AudioTrack` with `window: None`. (Degenerate atom-list, treat as no atoms.)
  - N ≥ 2 surviving atoms → N `AudioTrack` records with `window: Some((atom.start, atom.end))`. `duration_sec` per track is `atom.end - atom.start`. `title` per track is `atom.title`. `order` is the atom index.

This boundary is the only place that knows about atoms. The matcher, transcoder, and uploader work on `&[AudioTrack]` uniformly — `window: None` always means whole file (existing path), `window: Some` means slice.

## Matcher integration

Per AD-023, the matcher gains `MismatchCondition::ManyToFew` and `MismatchResponse::SplitProportional`. The `classify` precedence is:

```
equal-nonzero                          → None (clean pair, no mismatch)
either side == 0                       → Unalignable
chapters == 1, tracks >= 3             → OneToMany
tracks == 1, chapters >= 3             → ManyToOne
|chapters - tracks| <= 2               → CountOff       (small-delta near-miss)
chapters > tracks, both >= 2,
  2 * chapters > 3 * tracks,
  chapters <= 30 * tracks              → ManyToFew
otherwise                              → Unalignable
```

CountOff is checked **before** ManyToFew so that small-delta near-misses (e.g. `(22, 20)` — ratio 1.1, drop 2 minor text sections is the right call) stay with the existing `PairAccept`/`PairDrop` flow. ManyToFew triggers only when the audio granularity is *meaningfully* coarser than the text — the `2 * chapters > 3 * tracks` predicate is the integer form of "ratio > 1.5", which captures `(4, 2)`, `(5, 3)`, `(6, 3)`, `(85, 6)` while excluding `(22, 20)`, `(10, 8)`, `(5, 4)`. The upper bound `chapters <= 30 * tracks` is a sanity guard — beyond that the proportional pack quality degrades enough that `SingleLesson` is a better fallback. The observed worst-case (85 text chapters, 6 audio atoms, ratio ≈ 14×) is well within the bound.

### Forward-compat: `Unknown` variants

Both enums carry a `#[serde(other)] Unknown` variant per AD-023. The classifier never emits `Unknown` — it only appears when an older build reads a `project.json` written by a newer build with a tag the older build does not recognise. The matcher treats `Unknown` as if it were `Unalignable` with response set `[Cancel]` and preselect `Cancel`, forcing the user to redo the match step on a build that understands the new tag.

## Proportional packer

When the user picks `SplitProportional`, run the packer:

```rust
// src-tauri/src/core/matcher/pack.rs
pub struct Bucket {
    pub audio: ChapterAtom,
    pub text_range: std::ops::Range<usize>, // indices into the text chapter list
}

pub fn proportional_pack(atoms: &[ChapterAtom], text_chars: &[usize]) -> Vec<Bucket>;
```

### Algorithm

1. **Audio shares.** For each atom, `share_i = (end_i − start_i) / total_duration`. Compute prefix sum `audio_boundaries[i] = Σ share_k for k ≤ i` in `[0, 1]`.
2. **Text shares.** Compute prefix sum `cum_text_j / total_chars` per chapter `j`.
3. **Walk and bucket.** Walk text chapters in order. Push chapter `j` into bucket `i` while `cum_text_j ≤ audio_boundaries[i]`.
4. **Snap rule for boundary-straddling chapters.** If chapter `j` straddles boundary `i` (i.e. `cum_text_{j-1} < audio_boundaries[i] < cum_text_j`), assign it to whichever side absorbs more of it (`> 0.5` overlap → that side). Tie-break: earlier bucket. **A text chapter is never split.**
5. **Oversized-head exception.** If chapter `j` is the *first* chapter that the current bucket would take (`bucket has no chapters yet`) and it overflows immediately, force-keep it in the current bucket even when the majority-overlap rule would send it forward. Without this carve-out a single chapter larger than several atom buckets' combined share would skip past every starved bucket and starve them all of any text; with the carve-out only one bucket (the one that owns the chapter) absorbs it and downstream buckets keep moving.
6. **Final bucket.** The last bucket gets all remaining chapters regardless of cum_text overshoot.

### Output guarantees

- `result.len() == atoms.len()`. Every audio atom yields exactly one bucket, even if the bucket's `text_range` is empty (a degenerate case worth flagging in UI, not in code).
- Bucket text ranges are contiguous and cover `0..text_chars.len()` exactly.
- Pure function. No I/O. Deterministic across re-runs given identical inputs.

### Text length

`text_chars[j]` is the character count of text chapter `j` after the language profile's `normalise_text` step (so furigana ruby annotations, whitespace, and markup are already stripped). We use character count, not word count: see AD-023's rationale.

### Drift warning

A bucket whose computed `chars_per_second = bucket_text_chars / bucket_duration` deviates from the corpus median by more than `±30%` is flagged. The flag is informational — the upload proceeds. Surfaces in the Mismatch UI's bucket preview row.

## Transcode

`core::audio::transcode` gains a fourth argument, `window: Option<(f64, f64)>`:

```rust
pub async fn transcode(
    src: &Path,
    dst: &Path,
    enc: &Encoding,
    window: Option<(f64, f64)>, // NEW
) -> Result<(), AudioError>;
```

Behaviour per `AudioTrack.window`:

- `window: None` → existing path: `ffmpeg -i <src> -c:a libmp3lame -b:a <rate> <dst>`.
- `window: Some((start, end))` → `ffmpeg -ss <start> -to <end> -i <src> -c:a libmp3lame -b:a <rate> <dst>`.

`-ss` precedes `-i` for accurate seek at re-encode time (input-side seek + re-encoded output). Duration verify per slice: `|(end − start) − probed_mp3_duration| < 1.0 s`. Fail the job loudly on mismatch, same as the existing whole-file path.

The signature change ripples to 5 call sites — `core/audio/batch.rs` (2 callers), `core/audio/mod.rs` (drop-cancel test), `core/job/mod.rs` (production caller), `commands/upload.rs` (smoke command). Four pass `None` (preserving current behaviour); the production caller in `core/job/mod.rs` passes `track.window`.

## UI integration

The Mismatch screen gains a new card for `SplitProportional`, sibling to `SingleLesson`. The card layout:

- **Title:** "Split by embedded chapters"
- **Subtitle:** "N audio chapters found — text auto-grouped to match"
- **Preview pane (below the selected card):** one row per bucket showing:
  - Text chapter range — `Ch 1–14`, `Ch 15–28`, …
  - Atom title if narrative (population B), otherwise atom index — `第　２４　章` or `Atom 1`
  - Atom duration — `MM:SS`
  - Computed `chars/sec`
  - Drift warning icon (info `i`) on rows that exceed the ±30% band, with tooltip explaining the narrator may have skipped or added material at that boundary.

The preview pane is read-only in v1. Drag-to-adjust boundaries is a future addition.

## Test fixtures

Synthetic m4b artefacts at `src-tauri/tests/fixtures/audio/`, generated by `scripts/fixtures/synth_m4b_chapters.sh`:

| File | nb_chapters | Duration | Purpose |
|---|---|---|---|
| `synth_chapters_generic.m4b` | 3 | 60 s | Generic indexed titles (`Chapter 1`…), models population C |
| `synth_chapters_narrative.m4b` | 3 | 60 s | Narrative Japanese titles (`序章`, `第一章`, `第二章`), models population B atom title quality |
| `synth_chapters_intro.m4b` | 3 | 120 s | First atom is 5 s; filter rule must drop it. Models the tiny-intro case |

ffmpeg's mp4 muxer normalises every chapter atom's `time_base` to `1/1000` on output regardless of input metadata. The synthetic fixtures therefore cannot exercise the `1/44100` time_base path directly. This is acceptable because the parser reads only the float `start_time` / `end_time` fields and ignores `time_base` entirely (per AD-023's contract surface) — the structural path under test is identical. A real-world Audible m4b with `1/44100` atoms remains a useful manual smoke test if locally available.

AAC encoding is non-deterministic across libavcodec builds, so the fixtures do not assert sha256 — tests assert structural shape only.

## Test coverage targets

Unit tests on `proportional_pack` (pure function, no fixtures needed):

| Case | Input | Expected output |
|---|---|---|
| Equal-share uniform text | `atoms_share = [⅓, ⅓, ⅓]`, `text = [1; 6]` | `[0..2, 2..4, 4..6]` |
| Realistic 6 × 85 | `atoms_share = [⅙]×6`, `text = [c; 85]` | `[0..14, 14..28, 28..43, 43..57, 57..71, 71..85]` |
| Straddle assigns to majority side | `atoms_share = [0.5, 0.5]`, `text = [100, 50, 100]` | `[0..2, 2..3]` (chap 2 packs to bucket 0: 150 > boundary 125) |
| Degenerate oversize chapter | `atoms_share = [0.1, 0.1, 0.8]`, `text = [1000, 100]` | `[0..1, 1..1, 1..2]` (bucket 1 empty; surface as warning, do not panic) |
| Empty atom list rejected by caller | n/a | Caller must not invoke packer when atoms.is_empty() |

Integration tests over the synthetic fixtures:

| Fixture | Scenario | Expected `resolve_audio_tracks` result + matcher behaviour |
|---|---|---|
| `synth_chapters_generic.m4b` | Drop file + 6-chapter text | Yields 3 `AudioTrack` with `window: Some`. `classify(6, 3)` — delta=3 fails CountOff, then `2*6 > 3*3` (12 > 9) → `ManyToFew`. Pack yields `[0..2, 2..4, 4..6]`. |
| `synth_chapters_intro.m4b` | Drop file + 4-chapter text | Filter drops the 5 s intro atom; yields 2 `AudioTrack` with `window: Some`. `classify(4, 2)` — delta=2 hits CountOff under the current rule, but `2*4 > 3*2` (8 > 6) is true; the precedence puts CountOff first, so this case **resolves as CountOff**, not ManyToFew. That's intentional — at delta=2 the user gets `PairAccept` (pair-by-index, drop the 2 extras) as the preselect. For a true `ManyToFew` integration test use a 6-chapter text source against this fixture instead: `classify(6, 2)` → delta=4 skips CountOff, `2*6 > 3*2` → `ManyToFew`, pack yields `[0..3, 3..6]`. |
| `synth_chapters_narrative.m4b` | Drop file + 3-chapter text | Probe yields 3 atoms with `window: Some`. `classify(3, 3) == None` → clean pair. No packer involvement. |

Manual smoke (optional, depends on having user-side fixtures): drop `時をかける少女.m4b` (6 atoms) against an 85-chapter EPUB text — verify `SplitProportional` preselected, preview shows 6 buckets approximately `0..14, 14..28, …, 71..85`.

## Out of scope

The following are intentionally deferred:

- Whisper-based forced alignment for closer-than-proportional accuracy.
- Drag-to-adjust bucket boundaries in the UI.
- Fuzzy-matching atom `tags.title` to text chapter headings. Atom titles in the wild are usually generated indices (`Chapter N`, `<book> - N`) or stylised numerals (`第　２４　章` with full-width digits) — both lossy to match against text headings. Order remains the contract.
- Re-slicing audio atoms to match more text chapters. Atoms are the publisher's own boundaries and re-slicing risks mid-sentence cuts.
- Caching probe results by folder. Same-series mixed atom presence (observed in Drama CD sets where some files have atoms and others do not) makes folder-level caching unsafe.
