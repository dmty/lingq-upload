# EPUB subset we parse

> Status: **draft**. Captures the supported-EPUB contract HeadingStrategy (AD-016) is committing to. Update as new publisher quirks are absorbed.

## What we parse

- **EPUB 2** and **EPUB 3** zip containers. We use the `epub` Rust crate for spine + metadata extraction.
- **Spine-ordered XHTML files only.** Resources outside the spine (e.g. front-matter promo pages) are ignored.
- **`<ruby>` annotations** for furigana — stripped per AD-015 `JapaneseProfile::normalise_text`.
- **CSS-class-based headings** per HeadingStrategy:
  - `KindleStrategy`: heading is a `<h1>` / `<h2>` in its own XHTML file.
  - `KoboStrategy`: heading by class — `font-160per` (L1), `font-120per` / `font-140per` (L2), `bold` (L3). `koboSpan` text-node wrappers tolerated transparently.
  - `GenericH1Strategy` (fallback): only `<h1>` / `<h2>` taken as headings; no levels.
- **Paragraph extraction** from `<p>`, `<div>` (if direct text children), and `<span>` runs inside paragraph containers.

## What we ignore

- DRM-encrypted EPUBs (`META-INF/encryption.xml`) — fail fast with a clear error.
- Images, SVG, MathML — no text extraction from these.
- Footnotes / endnotes outside the main spine — ignored unless inline in the chapter XHTML.
- CSS positioning. Reading order is spine order, period.
- EPUB navigation (`nav.xhtml`, NCX) — we do not rely on TOC for heading detection; HeadingStrategy works on the XHTML directly. TOC is consulted only as a tiebreaker when two strategies score equally during autodetect.
- Right-to-left layout flags — text extraction is direction-agnostic; the LanguageProfile decides paragraph rules.

## Heading-detection strategy contract

Every `HeadingStrategy` impl honours:

- **Total function over the XHTML body.** Never panics on malformed markup; returns `None` for "this node is not a heading."
- **Side-effect-free.** No DOM mutation; we walk and emit a flat `Vec<Heading>`.
- **Confidence in `[0, 1]`.** Used by `epub::autodetect_strategy` to pick the best fit on a sample of the spine.

## Reference fixtures

Used as regression books. Pre-computed golden carved text lives in `src-tauri/tests/fixtures/` and gates `KindleStrategy` / `KoboStrategy` snapshot tests via `insta`.

| Book | Style | Notes |
|---|---|---|
| 海辺のカフカ（下） | Kindle | 27 chapters, one-XHTML-per-chapter. Tests the simple case + furigana. |
| 運命を創る | Kobo | 67 tracks across 21 XHTML files. Multiple sections per XHTML, `koboSpan` wrappers, chapter-divider absorption to verify. |

## Known unknowns

- Apple iBooks-published EPUBs (different ruby markup variant) — not yet tested.
- Calibre re-converts of Audible audiobook companion EPUBs — usually fine but TOC fragments seen in some.
- Comic / fixed-layout EPUBs — out of scope; fail fast with "fixed-layout EPUBs not supported" error.

## Open questions

- Should TOC (`nav.xhtml` / NCX) ever override heading detection when the XHTML body is ambiguous? Decide once the second heading strategy lands.
- Do we need a "preserve original whitespace" toggle for non-CJK languages where ASCII space is significant? Likely yes; revisit once a non-Japanese book is processed.
