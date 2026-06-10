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
    use std::io::Read;

    use quick_xml::events::Event;
    use quick_xml::Reader;

    use super::super::ChapterId;
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

    /// Decompressed-size ceiling per zip entry. EPUB bytes are untrusted; a
    /// crafted entry can deflate to gigabytes.
    const MAX_ENTRY_BYTES: u64 = 16 * 1024 * 1024;

    /// Read a zip entry as a `String`, sniffing UTF-8 / UTF-16 (LE/BE) BOMs.
    /// EPUBs in the wild are mostly UTF-8 with the occasional UTF-16 (Kindle).
    /// Other encodings return a parse error rather than mojibake.
    fn read_to_string_from_zip<R: std::io::Read + std::io::Seek>(
        zip: &mut zip::ZipArchive<R>,
        name: &str,
    ) -> Result<String, EpubError> {
        let f = zip
            .by_name(name)
            .map_err(|e| EpubError::Parse(format!("missing {name}: {e}")))?;
        let mut bytes = Vec::new();
        f.take(MAX_ENTRY_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| EpubError::Io(e.to_string()))?;
        if bytes.len() as u64 > MAX_ENTRY_BYTES {
            return Err(EpubError::Parse(format!(
                "{name}: decompressed entry exceeds {MAX_ENTRY_BYTES} byte cap"
            )));
        }
        decode_xml_bytes(&bytes, name)
    }

    fn decode_xml_bytes(bytes: &[u8], name: &str) -> Result<String, EpubError> {
        // UTF-16 LE BOM.
        if bytes.starts_with(&[0xFF, 0xFE]) {
            return decode_utf16(&bytes[2..], true, name);
        }
        // UTF-16 BE BOM.
        if bytes.starts_with(&[0xFE, 0xFF]) {
            return decode_utf16(&bytes[2..], false, name);
        }
        // UTF-8 BOM.
        let body: &[u8] = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            &bytes[3..]
        } else {
            bytes
        };
        match std::str::from_utf8(body) {
            Ok(s) => Ok(s.to_string()),
            Err(_) => {
                // Detect declared encoding for a helpful error; only utf-8 and
                // utf-16 are supported for now.
                let declared = sniff_xml_encoding(body).unwrap_or_else(|| "unknown".into());
                Err(EpubError::Parse(format!(
                    "{name}: unsupported text encoding '{declared}' (only utf-8 and utf-16 are supported)"
                )))
            }
        }
    }

    fn decode_utf16(bytes: &[u8], little_endian: bool, name: &str) -> Result<String, EpubError> {
        if !bytes.len().is_multiple_of(2) {
            return Err(EpubError::Parse(format!("{name}: truncated utf-16 stream")));
        }
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| {
                if little_endian {
                    u16::from_le_bytes([c[0], c[1]])
                } else {
                    u16::from_be_bytes([c[0], c[1]])
                }
            })
            .collect();
        String::from_utf16(&units)
            .map_err(|_| EpubError::Parse(format!("{name}: invalid utf-16 sequence")))
    }

    fn sniff_xml_encoding(bytes: &[u8]) -> Option<String> {
        let head = &bytes[..bytes.len().min(256)];
        let lc: Vec<u8> = head.iter().map(|b| b.to_ascii_lowercase()).collect();
        let key = b"encoding=";
        let idx = lc.windows(key.len()).position(|w| w == key)?;
        let after = &head[idx + key.len()..];
        let quote = *after.first()?;
        if quote != b'"' && quote != b'\'' {
            return None;
        }
        let rest = &after[1..];
        let end = rest.iter().position(|&b| b == quote)?;
        std::str::from_utf8(&rest[..end])
            .ok()
            .map(|s| s.to_string())
    }

    fn read_container_opf_path<R: std::io::Read + std::io::Seek>(
        zip: &mut zip::ZipArchive<R>,
    ) -> Result<String, EpubError> {
        let xml = read_to_string_from_zip(zip, "META-INF/container.xml")?;
        let mut reader = Reader::from_str(&xml);
        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Empty(e)) | Ok(Event::Start(e)) => {
                    if e.name().as_ref() == b"rootfile" {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"full-path" {
                                let v = attr
                                    .unescape_value()
                                    .map_err(|err| EpubError::Parse(err.to_string()))?;
                                return Ok(v.into_owned());
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
        Err(EpubError::Parse("no rootfile in container.xml".into()))
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

    fn parent_dir(p: &str) -> &str {
        match p.rfind('/') {
            Some(i) => &p[..i],
            None => "",
        }
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

    /// Case-insensitive substring search where the needle is already lower-case
    /// ASCII. Returns the byte offset of the first match in `haystack`.
    fn find_case_insensitive(haystack: &str, needle_lower_ascii: &str) -> Option<usize> {
        let hb = haystack.as_bytes();
        let nb = needle_lower_ascii.as_bytes();
        if nb.is_empty() || hb.len() < nb.len() {
            return None;
        }
        'outer: for i in 0..=hb.len() - nb.len() {
            for j in 0..nb.len() {
                if hb[i + j].to_ascii_lowercase() != nb[j] {
                    continue 'outer;
                }
            }
            return Some(i);
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

        #[test]
        fn decode_xml_bytes_utf8_passthrough() {
            let s = decode_xml_bytes(b"<?xml version=\"1.0\"?><a/>", "x.xml").unwrap();
            assert!(s.starts_with("<?xml"));
        }

        #[test]
        fn decode_xml_bytes_utf8_bom_stripped() {
            let mut b = vec![0xEF, 0xBB, 0xBF];
            b.extend_from_slice(b"<a/>");
            let s = decode_xml_bytes(&b, "x.xml").unwrap();
            assert_eq!(s, "<a/>");
        }

        #[test]
        fn decode_xml_bytes_utf16_le_with_bom() {
            // <?xml version="1.0" encoding="utf-16"?><a/> in UTF-16 LE with BOM.
            let s_orig = "<?xml version=\"1.0\"?><a/>";
            let mut bytes = vec![0xFF, 0xFE];
            for u in s_orig.encode_utf16() {
                bytes.extend_from_slice(&u.to_le_bytes());
            }
            let s = decode_xml_bytes(&bytes, "x.xml").unwrap();
            assert_eq!(s, s_orig);
        }

        #[test]
        fn decode_xml_bytes_utf16_be_with_bom() {
            let s_orig = "<a>漢</a>";
            let mut bytes = vec![0xFE, 0xFF];
            for u in s_orig.encode_utf16() {
                bytes.extend_from_slice(&u.to_be_bytes());
            }
            let s = decode_xml_bytes(&bytes, "x.xml").unwrap();
            assert_eq!(s, s_orig);
        }

        #[test]
        fn decode_xml_bytes_rejects_unknown_encoding() {
            // Latin-1-ish bytes that are not valid utf-8.
            let bytes = b"<?xml version=\"1.0\" encoding=\"latin-1\"?>\xE9".to_vec();
            let err = decode_xml_bytes(&bytes, "x.xml").unwrap_err();
            match err {
                EpubError::Parse(msg) => {
                    assert!(msg.contains("latin-1"), "got {msg}");
                }
                _ => panic!("expected Parse"),
            }
        }
    }
}
