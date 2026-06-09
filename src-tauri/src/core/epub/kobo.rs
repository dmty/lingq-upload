//! Kobo heading strategy.
//!
//! Kobo EPUBs are EPUB3 and ship an HTML navigation document (`nav.xhtml`)
//! instead of an NCX. The strategy walks the OPF spine in order and resolves
//! each chapter's title from the `nav epub:type="toc"` `<ol>` when present;
//! otherwise it falls back to the spine file's `<title>`. Heuristic
//! front/back-matter tagging keys off normalised title prefixes.

use std::collections::HashMap;
use std::io::Read;

use quick_xml::events::Event;
use quick_xml::Reader;

use super::{normalize_title, Chapter, ChapterId, ChapterKind, EpubError};
use crate::core::text::strip_ruby;

/// Marker type for the Kobo strategy. The runtime entry point is
/// [`parse_from_zip`]; the marker exists so callers can ask the strategy for
/// its canonical name without instantiating the enum.
pub struct KoboStrategy;

impl KoboStrategy {
    pub const NAME: &'static str = "kobo";

    pub fn name(&self) -> &'static str {
        Self::NAME
    }
}

const FRONT_MATTER_PREFIXES: &[&str] = &[
    "cover",
    "title",
    "title page",
    "copyright",
    "imprint",
    "dedication",
    "epigraph",
    "preface",
    "foreword",
    "prologue",
    "contents",
    "table of contents",
];

const BACK_MATTER_PREFIXES: &[&str] = &[
    "acknowledg",
    "about the author",
    "about the publisher",
    "afterword",
    "epilogue",
    "appendix",
    "bibliography",
    "glossary",
    "index",
    "notes",
    "colophon",
];

pub fn parse_from_zip<R: std::io::Read + std::io::Seek>(
    zip: &mut zip::ZipArchive<R>,
) -> Result<Vec<Chapter>, EpubError> {
    let opf_path = read_container_opf_path(zip)?;
    let opf_xml = read_to_string_from_zip(zip, &opf_path)?;
    let opf = parse_opf(&opf_xml, &opf_path)?;

    // href→title map sourced from nav.xhtml when present. Hrefs are stored
    // both as the raw nav href (relative to nav's base dir) and the resolved
    // zip path so lookups can match either side.
    let nav_titles: HashMap<String, String> = match opf.nav_href.as_deref() {
        Some(nav_href) => {
            let full = join_zip_path(&opf.base_dir, nav_href).unwrap_or_default();
            match read_to_string_from_zip(zip, &full) {
                Ok(xml) => {
                    let nav_base = parent_dir(&full).to_string();
                    parse_nav_titles(&xml, &nav_base)
                }
                Err(_) => HashMap::new(),
            }
        }
        None => HashMap::new(),
    };

    let mut chapters: Vec<Chapter> = Vec::with_capacity(opf.spine.len());
    for (i, href) in opf.spine.iter().enumerate() {
        let full = match join_zip_path(&opf.base_dir, href) {
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
        let title = nav_titles
            .get(&full)
            .cloned()
            .or_else(|| extract_html_title(&raw))
            .unwrap_or_else(|| format!("Chapter {}", i + 1));
        chapters.push(Chapter {
            order: i,
            title,
            body,
            ..Default::default()
        });
    }

    for (i, c) in chapters.iter_mut().enumerate() {
        c.order = i;
        c.kind = classify_kind(&c.title);
        c.id = ChapterId::from_chapter_parts(KoboStrategy::NAME, i, &c.title);
    }
    Ok(chapters)
}

fn classify_kind(title: &str) -> ChapterKind {
    let norm = normalize_title(title);
    if FRONT_MATTER_PREFIXES.iter().any(|p| norm.starts_with(p)) {
        return ChapterKind::FrontMatter;
    }
    if BACK_MATTER_PREFIXES.iter().any(|p| norm.starts_with(p)) {
        return ChapterKind::BackMatter;
    }
    ChapterKind::Body
}

struct OpfData {
    spine: Vec<String>,
    base_dir: String,
    nav_href: Option<String>,
}

fn parse_opf(opf_xml: &str, opf_path: &str) -> Result<OpfData, EpubError> {
    let base_dir = parent_dir(opf_path).to_string();
    let mut reader = Reader::from_str(opf_xml);
    let mut buf = Vec::new();
    let mut manifest_href: HashMap<String, String> = HashMap::new();
    let mut nav_id: Option<String> = None;
    let mut nav_href_direct: Option<String> = None;
    let mut spine_ids: Vec<String> = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(e)) | Ok(Event::Start(e)) => {
                let name = e.name();
                if name.as_ref() == b"item" {
                    let mut id = None;
                    let mut href = None;
                    let mut props: Option<String> = None;
                    for attr in e.attributes().flatten() {
                        match attr.key.as_ref() {
                            b"id" => {
                                id = attr.unescape_value().map(|v| v.into_owned()).ok();
                            }
                            b"href" => {
                                href = attr.unescape_value().map(|v| v.into_owned()).ok();
                            }
                            b"properties" => {
                                props = attr.unescape_value().map(|v| v.into_owned()).ok();
                            }
                            _ => {}
                        }
                    }
                    if let (Some(id), Some(href)) = (id.clone(), href.clone()) {
                        manifest_href.insert(id, href);
                    }
                    if let Some(p) = props {
                        if p.split_whitespace().any(|t| t == "nav") {
                            nav_id = id;
                            nav_href_direct = href;
                        }
                    }
                } else if name.as_ref() == b"itemref" {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"idref" {
                            if let Ok(v) = attr.unescape_value() {
                                spine_ids.push(v.into_owned());
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
    let spine: Vec<String> = spine_ids
        .into_iter()
        .filter_map(|id| manifest_href.get(&id).cloned())
        .collect();
    let nav_href = nav_href_direct.or_else(|| nav_id.and_then(|id| manifest_href.get(&id).cloned()));
    Ok(OpfData {
        spine,
        base_dir,
        nav_href,
    })
}

/// Walk `<nav epub:type="toc">` and pull every `<a href>` / inner text pair.
/// Hrefs are joined against `nav_base` so the returned keys match the spine
/// resolution.
fn parse_nav_titles(xml: &str, nav_base: &str) -> HashMap<String, String> {
    let mut out: HashMap<String, String> = HashMap::new();
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();

    let mut in_toc_nav = false;
    let mut nav_depth = 0i32;
    let mut pending_href: Option<String> = None;
    let mut text_buf = String::new();
    let mut in_a = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = e.name();
                if name.as_ref() == b"nav" {
                    let is_toc = e.attributes().flatten().any(|a| {
                        let k = a.key.as_ref();
                        (k == b"epub:type" || k.ends_with(b":type") || k == b"type")
                            && a.unescape_value()
                                .map(|v| v.split_whitespace().any(|t| t == "toc"))
                                .unwrap_or(false)
                    });
                    if is_toc {
                        in_toc_nav = true;
                        nav_depth = 1;
                    } else if in_toc_nav {
                        nav_depth += 1;
                    }
                } else if in_toc_nav && name.as_ref() == b"a" {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"href" {
                            if let Ok(v) = attr.unescape_value() {
                                pending_href = Some(v.into_owned());
                            }
                        }
                    }
                    text_buf.clear();
                    in_a = true;
                }
            }
            Ok(Event::Empty(e)) => {
                if in_toc_nav && e.name().as_ref() == b"a" {
                    // self-closing anchor with no inner text — rare; skip.
                }
            }
            Ok(Event::Text(t)) => {
                if in_a {
                    if let Ok(s) = t.unescape() {
                        text_buf.push_str(&s);
                    }
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name();
                if name.as_ref() == b"a" && in_a {
                    if let Some(href) = pending_href.take() {
                        let title = text_buf.trim().to_string();
                        if !title.is_empty() {
                            let path = path_part(&href);
                            let resolved = join_zip_path(nav_base, &path).unwrap_or_default();
                            if !resolved.is_empty() {
                                out.insert(resolved, title);
                            }
                        }
                    }
                    text_buf.clear();
                    in_a = false;
                } else if name.as_ref() == b"nav" && in_toc_nav {
                    nav_depth -= 1;
                    if nav_depth <= 0 {
                        in_toc_nav = false;
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    out
}

fn path_part(href: &str) -> String {
    href.split('#').next().unwrap_or(href).to_string()
}

fn extract_html_title(xml: &str) -> Option<String> {
    let bytes = xml.as_bytes();
    let open = find_case_insensitive(xml, "<title")?;
    let after_open_rel = bytes[open..].iter().position(|&b| b == b'>')?;
    let inner_start = open + after_open_rel + 1;
    let close_rel = find_case_insensitive(&xml[inner_start..], "</title")?;
    let inner = &xml[inner_start..inner_start + close_rel];
    let t = inner.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

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

// --- shared zip + decode helpers (cloned from kindle::* to avoid reaching
// into a sibling module's private surface). Kept small on purpose.

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

fn read_to_string_from_zip<R: std::io::Read + std::io::Seek>(
    zip: &mut zip::ZipArchive<R>,
    name: &str,
) -> Result<String, EpubError> {
    let mut f = zip
        .by_name(name)
        .map_err(|e| EpubError::Parse(format!("missing {name}: {e}")))?;
    let mut bytes = Vec::new();
    f.read_to_end(&mut bytes)
        .map_err(|e| EpubError::Io(e.to_string()))?;
    decode_xml_bytes(&bytes, name)
}

fn decode_xml_bytes(bytes: &[u8], name: &str) -> Result<String, EpubError> {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return decode_utf16(&bytes[2..], true, name);
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return decode_utf16(&bytes[2..], false, name);
    }
    let body: &[u8] = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &bytes[3..]
    } else {
        bytes
    };
    std::str::from_utf8(body)
        .map(|s| s.to_string())
        .map_err(|_| EpubError::Parse(format!("{name}: not valid utf-8 / utf-16")))
}

fn decode_utf16(bytes: &[u8], le: bool, name: &str) -> Result<String, EpubError> {
    if !bytes.len().is_multiple_of(2) {
        return Err(EpubError::Parse(format!("{name}: truncated utf-16")));
    }
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| {
            if le {
                u16::from_le_bytes([c[0], c[1]])
            } else {
                u16::from_be_bytes([c[0], c[1]])
            }
        })
        .collect();
    String::from_utf16(&units).map_err(|_| EpubError::Parse(format!("{name}: invalid utf-16")))
}

fn parent_dir(p: &str) -> &str {
    match p.rfind('/') {
        Some(i) => &p[..i],
        None => "",
    }
}

fn join_zip_path(base: &str, rel: &str) -> Result<String, EpubError> {
    if rel.is_empty() {
        return Err(EpubError::Parse("empty href".into()));
    }
    if rel.starts_with('/') {
        return Err(EpubError::Parse(format!("absolute href rejected: {rel}")));
    }
    let path_part = rel.split('#').next().unwrap_or(rel);
    let decoded = percent_decode_utf8(path_part)
        .ok_or_else(|| EpubError::Parse(format!("href not utf-8: {rel}")))?;
    if decoded.split('/').any(|seg| seg == "..") {
        return Err(EpubError::Parse(format!("traversal rejected: {rel}")));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_kind_cover_is_front_matter() {
        assert_eq!(classify_kind("Cover"), ChapterKind::FrontMatter);
        assert_eq!(classify_kind("  COVER  "), ChapterKind::FrontMatter);
    }

    #[test]
    fn classify_kind_about_the_author_is_back_matter() {
        assert_eq!(
            classify_kind("About the Author"),
            ChapterKind::BackMatter,
        );
        assert_eq!(
            classify_kind("Acknowledgments"),
            ChapterKind::BackMatter,
        );
    }

    #[test]
    fn classify_kind_chapter_is_body() {
        assert_eq!(classify_kind("Chapter 1"), ChapterKind::Body);
        assert_eq!(classify_kind("運命を創る"), ChapterKind::Body);
    }

    #[test]
    fn extract_html_title_basic() {
        let xml = "<html><head><title>Hello World</title></head><body/></html>";
        assert_eq!(extract_html_title(xml).as_deref(), Some("Hello World"));
    }

    #[test]
    fn extract_html_title_missing_returns_none() {
        assert!(extract_html_title("<html><body/></html>").is_none());
    }

    #[test]
    fn parse_nav_titles_basic() {
        let nav = r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<body>
  <nav epub:type="toc">
    <ol>
      <li><a href="ch1.xhtml">Cover</a></li>
      <li><a href="ch2.xhtml#frag">Chapter One</a></li>
    </ol>
  </nav>
</body></html>"#;
        let m = parse_nav_titles(nav, "OEBPS");
        assert_eq!(m.get("OEBPS/ch1.xhtml").map(String::as_str), Some("Cover"));
        assert_eq!(
            m.get("OEBPS/ch2.xhtml").map(String::as_str),
            Some("Chapter One")
        );
    }
}
