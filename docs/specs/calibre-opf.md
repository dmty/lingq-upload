# Calibre `metadata.opf` — observed subset

> Status: **stub**. To be probed against a real Calibre library. The shape below is the documented OPF 2.0 / 3.0 contract Calibre emits; specific quirks (custom columns, identifier formats, language tag variants) need verification.

## Source

Calibre stores each book as a directory under the library root. The per-book directory contains:

```
<Title> (<calibre_id>)/
├── metadata.opf          ← what we read
├── cover.jpg | cover.png ← what we copy if no explicit cover passed
└── <Title>.epub | …      ← the actual ebook file(s)
```

Library-wide there is also `metadata.db` (SQLite) and `metadata_db_prefs_backup.json`. The OPF file is the authoritative per-book metadata; `metadata.db` is an index of OPFs.

## Fields we extract (priority order)

| Field | OPF location | Required | Use |
|---|---|---|---|
| `title` | `<dc:title>` | yes | `Candidate.title`, default course title |
| `authors` | `<dc:creator opf:role="aut">` (may repeat) | yes | `Candidate.authors`, default course description |
| `language` | `<dc:language>` | yes | `Candidate.language` → `LingqClient(lang)` selector |
| `series` | `<meta name="calibre:series" content="…">` | no | `Candidate.series.name` |
| `series_index` | `<meta name="calibre:series_index" content="…">` | no | `Candidate.series.index`, library sort order |
| `tags` | `<dc:subject>` (may repeat) | no | facet filter in Library UI |
| `cover` | `<meta name="cover" content="cover-id">` → `<item id="cover-id" href="…">` | no | `Candidate.cover_path` |
| `identifier` (ISBN / ASIN / OCLC) | `<dc:identifier opf:scheme="ISBN">` etc. | no | `metadata_extras["isbn"]`, cross-source matching |
| `pubdate` | `<dc:date opf:event="publication">` | no | `metadata_extras["pubdate"]` |
| `publisher` | `<dc:publisher>` | no | `metadata_extras["publisher"]` |
| `rating` | `<meta name="calibre:rating" content="…">` | no | facet filter |

## Fields we ignore (for v1)

- Custom columns (`<meta name="calibre:user_metadata:…">`) — too user-specific. Future: opt-in.
- Reading lists / virtual libraries — out of scope.
- Bookmarks, last-read position — LingQ has its own.

## Language tag normalisation

`<dc:language>` is supposed to be IETF BCP-47 (`ja`, `en-US`, `zh-Hans`). In practice Calibre also emits ISO 639-2 codes (`jpn`, `eng`). We normalise:

| Input | Output |
|---|---|
| `ja`, `jpn`, `jap` | `ja` |
| `en`, `eng`, `en-US`, `en-GB`, `en-AU` | `en` (LingQ does not split English variants) |
| `zh`, `zh-Hans`, `chi`, `zho` | `zh` (Simplified default; user can override) |
| `zh-Hant` | future, distinct LingQ variant |
| anything else | passthrough first 2–3 chars, log a warning |

Normalisation lives in `src-tauri/src/ingest/calibre/lang.rs` — testable in isolation against a fixture table.

## Series & sort order

`series_index` is a decimal (Calibre allows `1.5` for novellas). We sort by `(series, series_index)` lexicographic but treat absent series as a singleton group rather than a single bucket. The Library UI groups by series first, then standalone by title.

## Open probes

- Confirm the OPF dialect (2.0 vs 3.0) on the user's library — 3.0 uses `<meta property="…">` instead of `<meta name="…">`.
- Verify cover extraction works for both `manifest`-referenced covers and the convention-based `cover.jpg` fallback.
- Test on books imported into Calibre from Amazon — they sometimes carry ASIN as `<dc:identifier opf:scheme="MOBI-ASIN">`.

## Parser choice

`quick-xml` with a hand-rolled small state machine. The OPF surface we care about is ~10 elements; full XPath via `roxmltree` is overkill. The parser is a pure function `parse_opf(&str) -> Result<OpfMetadata, OpfError>`.
