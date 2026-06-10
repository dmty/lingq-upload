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
/// [`parse_from_zip`]; the marker exists so callers can name the strategy
/// without instantiating the enum.
pub struct KoboStrategy;

impl KoboStrategy {
    pub const NAME: &'static str = "kobo";
}

// Exact normalised-title match (not a prefix). A chapter literally titled
// "Cover Notes" therefore stays Body — false positives are worse than a
// missed pre-select, since the user can always re-mark it from the UI.
const FRONT_MATTER_TITLES: &[&str] = &[
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
    "目次",
    "まえがき",
    "はじめに",
    "序文",
    "序章",
];

const BACK_MATTER_TITLES: &[&str] = &[
    "acknowledgments",
    "acknowledgements",
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
    "奥付",
    "あとがき",
    "解説",
    "謝辞",
];

pub fn parse_from_zip<R: std::io::Read + std::io::Seek>(
    zip: &mut zip::ZipArchive<R>,
) -> Result<Vec<Chapter>, EpubError> {
    let opf_path = read_container_opf_path(zip)?;
    let opf_xml = read_to_string_from_zip(zip, &opf_path)?;
    let opf = parse_opf(&opf_xml, &opf_path)?;

    // href→title map sourced from nav.xhtml when present. Keys are resolved
    // zip paths so spine lookups match directly.
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

    // (spine_href, title, body) tuples for surviving chapters. spine_href is
    // the original manifest href and acts as the stable hash anchor — that
    // way dropping an empty chapter does not shift later ids.
    let mut parsed: Vec<(String, String, String)> = Vec::with_capacity(opf.spine.len());
    let mut first_skip: Option<EpubError> = None;
    for (i, href) in opf.spine.iter().enumerate() {
        let full = match join_zip_path(&opf.base_dir, href) {
            Ok(p) => p,
            Err(e) => {
                first_skip.get_or_insert(e);
                continue;
            }
        };
        let raw = match read_to_string_from_zip(zip, &full) {
            Ok(s) => s,
            Err(e) => {
                first_skip.get_or_insert(e);
                continue;
            }
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
        parsed.push((href.clone(), title, body));
    }

    // An all-skips outcome is a broken book (wrong encoding, bad hrefs), not
    // an empty one — surface it instead of returning Ok(vec![]).
    if parsed.is_empty() && !opf.spine.is_empty() {
        let detail = first_skip
            .map(|e| format!("; first skip: {e}"))
            .unwrap_or_else(|| "; all spine bodies empty after cleaning".into());
        return Err(EpubError::Parse(format!(
            "0 of {} spine entries yielded chapters{detail}",
            opf.spine.len()
        )));
    }

    let mut chapters: Vec<Chapter> = Vec::with_capacity(parsed.len());
    for (i, (spine_href, title, body)) in parsed.into_iter().enumerate() {
        let kind = classify_kind(&title);
        let id = ChapterId::from_chapter_parts(KoboStrategy::NAME, &spine_href, &title);
        chapters.push(Chapter {
            order: i,
            title,
            body,
            id,
            kind,
        });
    }
    Ok(chapters)
}

fn classify_kind(title: &str) -> ChapterKind {
    let norm = normalize_title(title);
    if FRONT_MATTER_TITLES.iter().any(|t| *t == norm) {
        return ChapterKind::FrontMatter;
    }
    if BACK_MATTER_TITLES.iter().any(|t| *t == norm) {
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
///
/// If the document declares no `epub:type="toc"` on any `<nav>` but contains
/// exactly one `<nav>` overall, that nav is accepted as the toc. Producers
/// (some Sigil exports) omit the epub:type attribute when there is only one
/// nav and the relationship is unambiguous.
///
/// Sub-entries from a nested `<ol>` are flattened into the same flat map; the
/// hash is href-keyed so nested duplicates collapse onto the same chapter.
fn parse_nav_titles(xml: &str, nav_base: &str) -> HashMap<String, String> {
    let nav_count = count_nav_elements(xml);
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
                    // Sole-nav relaxation: if the doc has exactly one <nav>
                    // and that one lacks epub:type, treat it as the toc.
                    let accept = is_toc || (!in_toc_nav && nav_count == 1);
                    if accept && !in_toc_nav {
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
                                out.entry(resolved).or_insert(title);
                            }
                        }
                    }
                    text_buf.clear();
                    in_a = false;
                } else if name.as_ref() == b"nav" && in_toc_nav {
                    nav_depth -= 1;
                    if nav_depth <= 0 {
                        in_toc_nav = false;
                        nav_depth = 0;
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

fn count_nav_elements(xml: &str) -> usize {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut count = 0usize;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                if e.name().as_ref() == b"nav" {
                    count += 1;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    count
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
    // Reject the generic stub titles boilerplate templates leave behind
    // ("page", "untitled"). Callers fall back to nav lookup or spine href.
    let lc = t.to_ascii_lowercase();
    if t.is_empty() || lc == "page" || lc == "untitled" {
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

/// Decompressed-size ceiling per zip entry. EPUB bytes are untrusted; a
/// crafted entry can deflate to gigabytes. Real chapter XHTML tops out well
/// under a megabyte.
const MAX_ENTRY_BYTES: u64 = 16 * 1024 * 1024;

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

/// Resolve an href against `base`, normalizing `.` / `..` segments — zip
/// lookups are byte-literal, so `OEBPS/./ch1.xhtml` would miss the entry.
/// `..` that climbs above the zip root is rejected.
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
    let mut segs: Vec<&str> = if base.is_empty() {
        Vec::new()
    } else {
        base.split('/').collect()
    };
    for seg in decoded.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if segs.pop().is_none() {
                    return Err(EpubError::Parse(format!("traversal rejected: {rel}")));
                }
            }
            s => segs.push(s),
        }
    }
    if segs.is_empty() {
        return Err(EpubError::Parse(format!("href resolves to nothing: {rel}")));
    }
    Ok(segs.join("/"))
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
            // <style>/<script> payloads are not prose — drop them whole.
            if let Some(end) = raw_element_end(html, i) {
                i = end;
                continue;
            }
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

/// If `html[start..]` opens a `<style>` or `<script>` element, return the byte
/// offset just past its closing tag (or EOF when unterminated) so the caller
/// skips content and tags in one hop.
fn raw_element_end(html: &str, start: usize) -> Option<usize> {
    let rest = &html[start + 1..];
    let name = ["style", "script"].into_iter().find(|n| {
        rest.len() > n.len()
            && rest.as_bytes()[..n.len()].eq_ignore_ascii_case(n.as_bytes())
            && matches!(rest.as_bytes()[n.len()], b'>' | b'/' | b' ' | b'\t' | b'\r' | b'\n')
    })?;
    // Self-closing form has no payload; skip just the tag.
    if let Some(gt) = rest.find('>') {
        if rest.as_bytes()[..gt].ends_with(b"/") {
            return Some(start + 1 + gt + 1);
        }
    }
    let close = format!("</{name}");
    let close_rel = match find_case_insensitive(rest, &close) {
        Some(p) => p,
        None => return Some(html.len()),
    };
    let after_close = start + 1 + close_rel;
    match html[after_close..].find('>') {
        Some(p) => Some(after_close + p + 1),
        None => Some(html.len()),
    }
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

    #[test]
    fn parse_nav_titles_ignores_page_list_sibling_nav() {
        // Two <nav> siblings: only entries from `epub:type="toc"` survive.
        let nav = r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<body>
  <nav epub:type="toc">
    <ol>
      <li><a href="ch1.xhtml">Real Chapter</a></li>
    </ol>
  </nav>
  <nav epub:type="page-list">
    <ol>
      <li><a href="ch1.xhtml#p1">1</a></li>
      <li><a href="ch1.xhtml#p2">2</a></li>
    </ol>
  </nav>
</body></html>"#;
        let m = parse_nav_titles(nav, "OEBPS");
        assert_eq!(m.len(), 1);
        assert_eq!(m.get("OEBPS/ch1.xhtml").map(String::as_str), Some("Real Chapter"));
    }

    #[test]
    fn parse_nav_titles_nested_ol_flattens_top_level_only_first_wins() {
        // Nested <ol> sub-entries are flattened; the outer (top-level)
        // anchor lands first so the href→title map keeps the section title.
        let nav = r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<body>
  <nav epub:type="toc">
    <ol>
      <li>
        <a href="part1.xhtml">Part One</a>
        <ol>
          <li><a href="part1.xhtml#s1">Section 1</a></li>
          <li><a href="part1.xhtml#s2">Section 2</a></li>
        </ol>
      </li>
      <li><a href="part2.xhtml">Part Two</a></li>
    </ol>
  </nav>
</body></html>"#;
        let m = parse_nav_titles(nav, "OEBPS");
        assert_eq!(
            m.get("OEBPS/part1.xhtml").map(String::as_str),
            Some("Part One"),
            "outer anchor wins; subentries collapse onto same href",
        );
        assert_eq!(
            m.get("OEBPS/part2.xhtml").map(String::as_str),
            Some("Part Two"),
        );
    }

    #[test]
    fn parse_nav_titles_sole_nav_without_epub_type_accepted() {
        // Producers sometimes drop epub:type when there's only one nav.
        let nav = r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<body>
  <nav>
    <ol>
      <li><a href="ch1.xhtml">Only Chapter</a></li>
    </ol>
  </nav>
</body></html>"#;
        let m = parse_nav_titles(nav, "OEBPS");
        assert_eq!(
            m.get("OEBPS/ch1.xhtml").map(String::as_str),
            Some("Only Chapter"),
        );
    }

    #[test]
    fn classify_kind_cover_notes_stays_body() {
        // Entire-title match: a chapter literally called "Cover Notes" is
        // not front matter.
        assert_eq!(classify_kind("Cover Notes"), ChapterKind::Body);
        assert_eq!(classify_kind("Cover Operations"), ChapterKind::Body);
    }

    #[test]
    fn extract_html_title_stub_page_returns_none() {
        assert!(extract_html_title("<html><head><title>page</title></head></html>").is_none());
        assert!(extract_html_title("<html><head><title>Untitled</title></head></html>").is_none());
        assert!(extract_html_title("<html><head><title>  </title></head></html>").is_none());
    }

    #[test]
    fn decode_basic_entities_amp_decodes_last() {
        assert_eq!(decode_basic_entities("&amp;lt;"), "&lt;");
        assert_eq!(decode_basic_entities("&amp;amp;"), "&amp;");
        assert_eq!(decode_basic_entities("&lt;b&gt; &amp; &quot;q&quot;"), "<b> & \"q\"");
    }

    #[test]
    fn classify_kind_japanese_front_matter() {
        assert_eq!(classify_kind("目次"), ChapterKind::FrontMatter);
        assert_eq!(classify_kind("まえがき"), ChapterKind::FrontMatter);
        assert_eq!(classify_kind("はじめに"), ChapterKind::FrontMatter);
    }

    #[test]
    fn classify_kind_japanese_back_matter() {
        assert_eq!(classify_kind("奥付"), ChapterKind::BackMatter);
        assert_eq!(classify_kind("あとがき"), ChapterKind::BackMatter);
        assert_eq!(classify_kind("解説"), ChapterKind::BackMatter);
    }

    #[test]
    fn join_zip_path_normalizes_dot_segments() {
        assert_eq!(join_zip_path("OEBPS", "./ch1.xhtml").unwrap(), "OEBPS/ch1.xhtml");
        assert_eq!(join_zip_path("OEBPS", "a/./b.xhtml").unwrap(), "OEBPS/a/b.xhtml");
        assert_eq!(join_zip_path("", "./ch1.xhtml").unwrap(), "ch1.xhtml");
    }

    #[test]
    fn join_zip_path_resolves_parent_within_zip() {
        assert_eq!(
            join_zip_path("OEBPS/text", "../images/x.xhtml").unwrap(),
            "OEBPS/images/x.xhtml"
        );
    }

    #[test]
    fn join_zip_path_rejects_escape_above_zip_root() {
        assert!(join_zip_path("OEBPS", "../../etc/passwd").is_err());
        assert!(join_zip_path("", "../x.xhtml").is_err());
    }

    #[test]
    fn strip_html_tags_drops_style_and_script_content() {
        let html = "<head><style>.a { color: red }</style></head>\
<body><p>Keep</p><script>var x = 1;</script><p>Also keep</p></body>";
        let out = strip_html_tags(html);
        assert!(!out.contains("color"), "css leaked: {out}");
        assert!(!out.contains("var x"), "js leaked: {out}");
        assert!(out.contains("Keep") && out.contains("Also keep"));
    }

    #[test]
    fn strip_html_tags_self_closing_style_keeps_following_text() {
        let out = strip_html_tags("<style type=\"text/css\"/><p>Body text</p>");
        assert_eq!(out.trim(), "Body text");
    }
}
