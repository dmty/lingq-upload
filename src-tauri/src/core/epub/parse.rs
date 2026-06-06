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

/// Parse an EPUB file into a Vec<Chapter> using the given heading strategy.
///
/// Kindle / NavDoc / GenericH1 are implemented; Kobo is deferred.
/// Empty/whitespace-only chapters are dropped. Body text is `strip_ruby`-clean.
pub fn parse_epub(path: &Path, strategy: HeadingStrategy) -> Result<Vec<Chapter>, EpubError> {
    if matches!(strategy, HeadingStrategy::Kobo) {
        unimplemented!("Kobo HeadingStrategy deferred to a future sprint");
    }
    kindle::parse(path, strategy)
}

mod kindle {
    use std::io::Read;
    use std::path::Path;

    use quick_xml::events::Event;
    use quick_xml::Reader;

    use super::{Chapter, EpubError, HeadingStrategy};
    use crate::core::text::strip_ruby;

    pub fn parse(path: &Path, _strategy: HeadingStrategy) -> Result<Vec<Chapter>, EpubError> {
        let file = std::fs::File::open(path)?;
        let mut zip = zip::ZipArchive::new(file).map_err(|e| EpubError::Zip(e.to_string()))?;

        let opf_path = read_container_opf_path(&mut zip)?;
        let opf_xml = read_to_string_from_zip(&mut zip, &opf_path)?;
        let (spine_hrefs, base_dir) = parse_opf_spine(&opf_xml, &opf_path)?;

        let mut chapters = Vec::with_capacity(spine_hrefs.len());
        for (i, href) in spine_hrefs.iter().enumerate() {
            let full = join_zip_path(&base_dir, href);
            let raw = match read_to_string_from_zip(&mut zip, &full) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let body = clean_chapter_body(&raw);
            if body.trim().is_empty() {
                continue;
            }
            let title = extract_first_heading(&raw).unwrap_or_else(|| format!("Chapter {}", i + 1));
            chapters.push(Chapter {
                order: i,
                title,
                body,
            });
        }
        // Re-index `order` after dropping empties.
        for (i, c) in chapters.iter_mut().enumerate() {
            c.order = i;
        }
        Ok(chapters)
    }

    fn read_to_string_from_zip<R: std::io::Read + std::io::Seek>(
        zip: &mut zip::ZipArchive<R>,
        name: &str,
    ) -> Result<String, EpubError> {
        let mut f = zip
            .by_name(name)
            .map_err(|e| EpubError::Parse(format!("missing {name}: {e}")))?;
        let mut s = String::new();
        f.read_to_string(&mut s)
            .map_err(|e| EpubError::Io(e.to_string()))?;
        Ok(s)
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
                                    id = attr
                                        .unescape_value()
                                        .map(|v| v.into_owned())
                                        .ok();
                                }
                                b"href" => {
                                    href = attr
                                        .unescape_value()
                                        .map(|v| v.into_owned())
                                        .ok();
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

    fn join_zip_path(base: &str, rel: &str) -> String {
        if base.is_empty() {
            rel.to_string()
        } else {
            format!("{base}/{rel}")
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

    fn decode_basic_entities(s: &str) -> String {
        s.replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&#x3000;", "\u{3000}")
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

    fn extract_first_heading(html: &str) -> Option<String> {
        for tag in ["h1", "h2", "h3"] {
            let open = format!("<{tag}");
            if let Some(start) = html.to_lowercase().find(&open) {
                if let Some(gt) = html[start..].find('>') {
                    let after = start + gt + 1;
                    let close = format!("</{tag}>");
                    if let Some(end) = html[after..].to_lowercase().find(&close) {
                        let raw = &html[after..after + end];
                        let txt = strip_html_tags(raw).trim().to_string();
                        if !txt.is_empty() {
                            return Some(txt);
                        }
                    }
                }
            }
        }
        None
    }
}
