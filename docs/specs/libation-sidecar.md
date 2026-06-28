# Libation JSON sidecar — observed subset

> Status: **stub**. To be probed against real Libation output. Field set below is what earlier scripts consumed; the exact JSON shape Libation emits should be re-verified — Libation versions differ.

## Source

Libation decrypts purchased Audible content. Per audiobook it produces:

```
<Author> - <Title> [<ASIN>]/
├── <Title> [<ASIN>].m4b              ← single-file mode, full audiobook
│   OR
├── <Title> [<ASIN>] - 01 - <chapter title>.m4b  ← split-by-chapter mode
├── <Title> [<ASIN>] - 02 - …
├── …
├── <Title> [<ASIN>].json             ← the sidecar we read
└── <Title> [<ASIN>].jpg              ← cover
```

The user's flow uses **split-by-chapter** mode (one m4b/m4a per chapter), which aligns naturally with LingQ's lesson-per-chapter model.

## Fields we extract

| Field | JSON path | Required | Use |
|---|---|---|---|
| `title` | `.title` or `.product.title` | yes | `Candidate.title`, default course title |
| `authors` | `.authors[].name` | yes | `Candidate.authors` |
| `narrators` | `.narrators[].name` | no | `metadata_extras["narrators"]`, facet filter |
| `asin` | `.asin` or `.product.asin` | no | `metadata_extras["asin"]`, cross-source matching key |
| `language` | `.language` (when present) | no | `Candidate.language` — Audible doesn't always set this; we fall back to Calibre's value if paired |
| `series_name` | `.series[0].name` | no | `Candidate.series.name` |
| `series_position` | `.series[0].position` | no | `Candidate.series.index` |
| `cover_url` | `.product.imageUrl` | no | downloaded once, cached as `cover.jpg` |
| `chapters` | `.chapters[]` | yes for split mode | `Candidate.chapter_manifest` |

### Chapter manifest shape

```jsonc
"chapters": [
  {
    "title": "Chapter 1: A Beginning",
    "start_offset_ms": 0,
    "length_ms": 1234567,
    "file": "<Title> [<ASIN>] - 01 - A Beginning.m4b"  // present in split mode
  },
  …
]
```

We project this into a `ChapterManifest`:

```rust
pub struct ChapterManifest {
    pub chapters: Vec<ChapterEntry>,
}
pub struct ChapterEntry {
    pub title: String,
    pub start_ms: u64,
    pub duration_ms: u64,
    pub file_path: Option<PathBuf>,  // None in single-file mode
}
```

## Filename pattern

Libation's split-mode filename:

```
<book title> [<ASIN>] - <NN> - <chapter title>.m4b
```

…sometimes `.m4a` for individual chapters of the same audiobook. **Always glob both extensions** — a single book may mix them, and missing one silently drops a track.

## Cross-source pairing with Calibre

Reconciliation key (in priority order):

1. `(asin)` if both sources carry it.
2. `(normalised_title, normalised_author)` — Unicode NFC + lower + drop `[bracket]` + drop `: subtitle` after first colon. Used in `core::library::reconcile`.
3. Fuzzy ratio ≥ 0.85 against any candidate's `(title, author)` tuple — Levenshtein on the normalised forms.
4. Below 0.85: flag for user reconciliation in the Library UI.

## Open probes

- Confirm Libation's current JSON schema field names. Older versions (pre-2024) used different paths; the user's install may have either.
- Verify the chapter timing offsets line up with the m4b files Libation produces, not the source Audible runtime (relevant if Libation trims silence).
- Test that the `narrators` list is reliably populated.

## Parser choice

`serde_json` with `#[serde(rename_all = "camelCase")]` and `#[serde(default)]` on every optional field. Forward-compatible by construction. Pure function `parse_libation_sidecar(&str) -> Result<LibationMetadata, LibationError>`.
