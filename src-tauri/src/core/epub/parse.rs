use std::path::Path;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{Chapter, EpubError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum HeadingStrategy {
    Kindle,
    NavDoc,
    GenericH1,
    Kobo,
}

/// Parse an EPUB file into a Vec<Chapter>. The heading strategy is derived
/// internally via [`super::autodetect_vendor`] — callers cannot bypass
/// detection. Empty/whitespace-only chapters are dropped. Body text is
/// `strip_ruby`-clean.
pub fn parse_epub(path: &Path) -> Result<Vec<Chapter>, EpubError> {
    let bytes = std::fs::read(path)?;
    parse_epub_bytes(&bytes)
}

/// In-memory variant of [`parse_epub`]. Used by the orchestrator after the
/// file has already been slurped + decoded for vendor detection — avoids a
/// second `open + ZipArchive::new` round-trip.
pub fn parse_epub_bytes(bytes: &[u8]) -> Result<Vec<Chapter>, EpubError> {
    let strategy = strategy_from_bytes(bytes);
    parse_epub_with_strategy(bytes, strategy)
}

fn strategy_from_bytes(bytes: &[u8]) -> HeadingStrategy {
    match super::autodetect_vendor_bytes(bytes) {
        Ok(d) if d.vendor == super::EpubVendor::Kobo => HeadingStrategy::Kobo,
        _ => HeadingStrategy::Kindle,
    }
}

/// Explicit-strategy entrypoint. Production code routes through
/// [`parse_epub_bytes`] / [`parse_epub`] so detection always runs;
/// this form exists for tests and snapshots that pin a strategy
/// regardless of detection.
pub fn parse_epub_with_strategy(
    bytes: &[u8],
    strategy: HeadingStrategy,
) -> Result<Vec<Chapter>, EpubError> {
    let cursor = std::io::Cursor::new(bytes);
    let mut zip = zip::ZipArchive::new(cursor).map_err(|e| EpubError::Zip(e.to_string()))?;
    match strategy {
        HeadingStrategy::Kobo => super::kobo::parse_from_zip(&mut zip),
        HeadingStrategy::Kindle | HeadingStrategy::NavDoc | HeadingStrategy::GenericH1 => {
            kindle::parse_from_zip(&mut zip)
        }
    }
}

mod kindle {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    use super::super::{
        find_case_insensitive, parent_dir, read_container_opf_path, read_to_string_from_zip,
        ChapterId,
    };
    use super::{Chapter, EpubError};
    use crate::core::text::strip_ruby;

    pub fn parse_from_zip<R: std::io::Read + std::io::Seek>(
        zip: &mut zip::ZipArchive<R>,
    ) -> Result<Vec<Chapter>, EpubError> {
        let opf_path = read_container_opf_path(zip)?;
        let opf_xml = read_to_string_from_zip(zip, &opf_path)?;
        let (spine_hrefs, base_dir) = parse_opf_spine(&opf_xml, &opf_path)?;

        // (spine_href, title, body) for surviving chapters. spine_href is the
        // stable hash anchor — dropping empties never shifts later ids.
        let mut parsed: Vec<(String, String, String)> = Vec::with_capacity(spine_hrefs.len());
        for (i, href) in spine_hrefs.iter().enumerate() {
            let full = match join_zip_path(&base_dir, href) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let raw = match read_to_string_from_zip(zip, &full) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let body = clean_chapter_body(&raw);
            if body.trim().is_empty() {
                continue;
            }
            let title = extract_first_heading(&raw).unwrap_or_else(|| format!("Chapter {}", i + 1));
            parsed.push((href.clone(), title, body));
        }

        let mut chapters = Vec::with_capacity(parsed.len());
        for (i, (spine_href, title, body)) in parsed.into_iter().enumerate() {
            let id = ChapterId::from_chapter_parts("kindle", &spine_href, &title);
            chapters.push(Chapter {
                order: i,
                title,
                body,
                id,
                ..Default::default()
            });
        }
        Ok(chapters)
    }

    fn parse_opf_spine(opf_xml: &str, opf_path: &str) -> Result<(Vec<String>, String), EpubError> {
        let base_dir = parent_dir(opf_path).to_string();
        let mut reader = Reader::from_str(opf_xml);
        let mut buf = Vec::new();
        let mut manifest: std::collections::HashMap<String, String> = Default::default();
        let mut spine: Vec<String> = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Empty(e)) | Ok(Event::Start(e)) => {
                    let name = e.name();
                    if name.as_ref() == b"item" {
                        let mut id = None;
                        let mut href = None;
                        for attr in e.attributes().flatten() {
                            match attr.key.as_ref() {
                                b"id" => {
                                    id = attr.unescape_value().map(|v| v.into_owned()).ok();
                                }
                                b"href" => {
                                    href = attr.unescape_value().map(|v| v.into_owned()).ok();
                                }
                                _ => {}
                            }
                        }
                        if let (Some(id), Some(href)) = (id, href) {
                            manifest.insert(id, href);
                        }
                    } else if name.as_ref() == b"itemref" {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"idref" {
                                if let Ok(v) = attr.unescape_value() {
                                    spine.push(v.into_owned());
                                }
                            }
                        }
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(EpubError::Parse(e.to_string())),
                _ => {}
            }
            buf.clear();
        }
        let hrefs: Vec<String> = spine
            .into_iter()
            .filter_map(|idref| manifest.get(&idref).cloned())
            .collect();
        Ok((hrefs, base_dir))
    }

    /// Resolve a manifest href against the OPF base directory.
    ///
    /// Hrefs in OPF manifests are URL-encoded (e.g. `Chapter%201.xhtml`) and
    /// zip lookups are byte-literal, so we percent-decode before joining.
    /// Rejects traversal (`..`) and absolute (`/foo`) hrefs.
    fn join_zip_path(base: &str, rel: &str) -> Result<String, EpubError> {
        if rel.is_empty() {
            return Err(EpubError::Parse("empty manifest href".into()));
        }
        if rel.starts_with('/') {
            return Err(EpubError::Parse(format!("absolute href rejected: {rel}")));
        }
        // Strip any fragment.
        let path_part = rel.split('#').next().unwrap_or(rel);
        let decoded = percent_decode_utf8(path_part).ok_or_else(|| {
            EpubError::Parse(format!("href is not valid utf-8 after decode: {rel}"))
        })?;
        if decoded.split('/').any(|seg| seg == "..") {
            return Err(EpubError::Parse(format!("traversal href rejected: {rel}")));
        }
        if base.is_empty() {
            Ok(decoded)
        } else {
            Ok(format!("{base}/{decoded}"))
        }
    }

    fn percent_decode_utf8(s: &str) -> Option<String> {
        let bytes = s.as_bytes();
        let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                let hi = hex_val(bytes[i + 1])?;
                let lo = hex_val(bytes[i + 2])?;
                out.push((hi << 4) | lo);
                i += 3;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        String::from_utf8(out).ok()
    }

    fn hex_val(b: u8) -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(10 + b - b'a'),
            b'A'..=b'F' => Some(10 + b - b'A'),
            _ => None,
        }
    }

    fn clean_chapter_body(html: &str) -> String {
        let stripped = strip_ruby(html);
        let text = strip_html_tags(&stripped);
        collapse_whitespace(&text)
    }

    fn strip_html_tags(html: &str) -> String {
        let mut out = String::with_capacity(html.len());
        let bytes = html.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'<' {
                match bytes[i + 1..].iter().position(|&b| b == b'>') {
                    Some(p) => {
                        i = i + 1 + p + 1;
                    }
                    None => {
                        // Stray '<' (single ASCII byte). Emit it and step by
                        // one — '<' is always 1 byte in UTF-8 so we never land
                        // mid-codepoint.
                        out.push('<');
                        i += 1;
                    }
                }
                continue;
            }
            let ch = std::str::from_utf8(&bytes[i..])
                .ok()
                .and_then(|s| s.chars().next())
                .unwrap_or('\u{FFFD}');
            out.push(ch);
            i += ch.len_utf8().max(1);
        }
        decode_basic_entities(&out)
    }

    // `&amp;` must decode last or `&amp;lt;` double-decodes to `<`.
    fn decode_basic_entities(s: &str) -> String {
        s.replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&#x3000;", "\u{3000}")
            .replace("&amp;", "&")
    }

    fn collapse_whitespace(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut prev_blank = false;
        for line in s.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                if !prev_blank {
                    out.push('\n');
                    prev_blank = true;
                }
            } else {
                out.push_str(trimmed);
                out.push('\n');
                prev_blank = false;
            }
        }
        out.trim().to_string()
    }

    /// Locate the first `<hN>...</hN>` (N ∈ {1,2,3}) and return its stripped
    /// text. Case-insensitive without lower-casing the whole document — slicing
    /// by lower-cased byte offsets breaks for CJK / ß / İ where lower-case
    /// length differs from the original.
    fn extract_first_heading(html: &str) -> Option<String> {
        for tag_lc in ['1', '2', '3'] {
            if let Some(start) = find_open_tag(html, tag_lc) {
                let after_open = start + html[start..].find('>')? + 1;
                let close_pat_lower = format!("</h{tag_lc}");
                let close_rel = find_case_insensitive(&html[after_open..], &close_pat_lower)?;
                let inner = &html[after_open..after_open + close_rel];
                let txt = strip_html_tags(inner).trim().to_string();
                if !txt.is_empty() {
                    return Some(txt);
                }
            }
        }
        None
    }

    /// Find `<hN` (any case of `h`) starting somewhere in `html`. Returns the
    /// byte offset of the `<`.
    fn find_open_tag(html: &str, digit: char) -> Option<usize> {
        let bytes = html.as_bytes();
        let want_digit = digit as u8;
        let mut i = 0;
        while i + 2 < bytes.len() {
            if bytes[i] == b'<'
                && (bytes[i + 1] == b'h' || bytes[i + 1] == b'H')
                && bytes[i + 2] == want_digit
            {
                // Next byte must terminate the tag name: whitespace, '>', or '/'.
                let term = bytes.get(i + 3).copied().unwrap_or(b' ');
                if term == b' '
                    || term == b'\t'
                    || term == b'\n'
                    || term == b'\r'
                    || term == b'>'
                    || term == b'/'
                {
                    return Some(i);
                }
            }
            i += 1;
        }
        None
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn decode_basic_entities_amp_decodes_last() {
            assert_eq!(decode_basic_entities("&amp;lt;"), "&lt;");
            assert_eq!(decode_basic_entities("&amp;amp;"), "&amp;");
        }

        #[test]
        fn join_zip_path_percent_decodes() {
            assert_eq!(
                join_zip_path("OEBPS", "Chapter%201.xhtml").unwrap(),
                "OEBPS/Chapter 1.xhtml"
            );
        }

        #[test]
        fn join_zip_path_rejects_traversal() {
            assert!(join_zip_path("OEBPS", "../etc/passwd").is_err());
            assert!(join_zip_path("OEBPS", "foo/../../bar").is_err());
        }

        #[test]
        fn join_zip_path_rejects_absolute() {
            assert!(join_zip_path("OEBPS", "/etc/passwd").is_err());
        }

        #[test]
        fn join_zip_path_strips_fragment() {
            assert_eq!(
                join_zip_path("OEBPS", "chap.xhtml#section").unwrap(),
                "OEBPS/chap.xhtml"
            );
        }

        #[test]
        fn join_zip_path_empty_base() {
            assert_eq!(join_zip_path("", "file.xhtml").unwrap(), "file.xhtml");
        }

        #[test]
        fn extract_first_heading_cjk_inside_h1() {
            let html = "<html><body><h1>海辺のカフカ</h1><p>本文</p></body></html>";
            assert_eq!(extract_first_heading(html), Some("海辺のカフカ".into()));
        }

        #[test]
        fn extract_first_heading_uppercase_tag() {
            let html = "<HTML><BODY><H1>Title</H1></BODY></HTML>";
            assert_eq!(extract_first_heading(html), Some("Title".into()));
        }

        #[test]
        fn extract_first_heading_mixed_case_with_attrs() {
            let html = r#"<h1 class="chapter">İstanbul ßtraße</h1>"#;
            assert_eq!(extract_first_heading(html), Some("İstanbul ßtraße".into()));
        }

        #[test]
        fn extract_first_heading_h2_fallback() {
            let html = "<body><h2>Second-level</h2></body>";
            assert_eq!(extract_first_heading(html), Some("Second-level".into()));
        }

    }
}
