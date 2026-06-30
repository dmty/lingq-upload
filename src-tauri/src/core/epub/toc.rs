//! Shared OPF / TOC plumbing for the EPUB strategies.
//!
//! Chapter titles live in one of three places: the EPUB3 nav document
//! (`nav.xhtml`, picked from the manifest by `properties="nav"`), the EPUB2
//! NCX (`toc.ncx`, picked from the spine's `toc` attribute or from a manifest
//! item with the dtbncx media type), or — as a last resort — per-spine
//! headings. The first two are the strategy-agnostic source of truth; only
//! the heading fallback differs between Kindle and Kobo.

use std::collections::HashMap;
use std::io::{Read, Seek};

use quick_xml::events::Event;
use quick_xml::Reader;

use super::{parent_dir, read_to_string_from_zip, EpubError};

/// Spine + TOC pointers pulled out of a single OPF pass.
///
/// `spine` carries the original manifest hrefs (URL-encoded, base-relative),
/// matching the on-disk OPF; resolution against the zip happens at call
/// sites via [`join_zip_path`]. `nav_path` / `ncx_path` are already resolved
/// to zip-literal paths.
pub(crate) struct OpfRefs {
    pub spine: Vec<String>,
    pub base_dir: String,
    pub nav_path: Option<String>,
    pub ncx_path: Option<String>,
}

pub(crate) fn parse_opf_refs(opf_xml: &str, opf_path: &str) -> Result<OpfRefs, EpubError> {
    let base_dir = parent_dir(opf_path).to_string();
    let mut reader = Reader::from_str(opf_xml);
    let mut buf = Vec::new();
    let mut manifest_href: HashMap<String, String> = HashMap::new();
    let mut manifest_media: HashMap<String, String> = HashMap::new();
    let mut nav_id: Option<String> = None;
    let mut nav_href_direct: Option<String> = None;
    let mut ncx_id_from_spine: Option<String> = None;
    let mut spine_ids: Vec<String> = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(e)) | Ok(Event::Start(e)) => {
                let name = e.name();
                if local_name(name.as_ref()) == b"item" {
                    let mut id = None;
                    let mut href = None;
                    let mut media: Option<String> = None;
                    let mut props: Option<String> = None;
                    for attr in e.attributes().flatten() {
                        match attr.key.as_ref() {
                            b"id" => id = attr.unescape_value().map(|v| v.into_owned()).ok(),
                            b"href" => href = attr.unescape_value().map(|v| v.into_owned()).ok(),
                            b"media-type" => {
                                media = attr.unescape_value().map(|v| v.into_owned()).ok()
                            }
                            b"properties" => {
                                props = attr.unescape_value().map(|v| v.into_owned()).ok()
                            }
                            _ => {}
                        }
                    }
                    if let (Some(id), Some(href)) = (id.clone(), href.clone()) {
                        manifest_href.insert(id.clone(), href);
                        if let Some(m) = media.clone() {
                            manifest_media.insert(id, m);
                        }
                    }
                    if let Some(p) = props {
                        if p.split_whitespace().any(|t| t == "nav") {
                            nav_id = id;
                            nav_href_direct = href;
                        }
                    }
                } else if local_name(name.as_ref()) == b"spine" {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"toc" {
                            ncx_id_from_spine = attr.unescape_value().map(|v| v.into_owned()).ok();
                        }
                    }
                } else if local_name(name.as_ref()) == b"itemref" {
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

    let nav_href =
        nav_href_direct.or_else(|| nav_id.and_then(|id| manifest_href.get(&id).cloned()));
    let nav_path = nav_href.and_then(|h| join_zip_path(&base_dir, &h).ok());

    let ncx_href = ncx_id_from_spine
        .and_then(|id| manifest_href.get(&id).cloned())
        .or_else(|| {
            manifest_media
                .iter()
                .find(|(_, m)| m.eq_ignore_ascii_case("application/x-dtbncx+xml"))
                .and_then(|(id, _)| manifest_href.get(id).cloned())
        });
    let ncx_path = ncx_href.and_then(|h| join_zip_path(&base_dir, &h).ok());

    Ok(OpfRefs {
        spine,
        base_dir,
        nav_path,
        ncx_path,
    })
}

/// Read nav + NCX (whichever exist) and merge into a single `resolved zip
/// path → title` map. Nav entries overwrite NCX entries on collision — EPUB3
/// nav is the more recent declaration when both ship.
///
/// Fail-soft: missing or malformed TOC files yield an empty / partial map
/// instead of an error. The strategy layer falls back to per-spine heading
/// scrapes when a spine entry has no TOC title; surfacing a parse error
/// would force a whole-book failure on a single broken NCX entry.
pub(crate) fn read_toc_titles<R: Read + Seek>(
    zip: &mut zip::ZipArchive<R>,
    nav_path: Option<&str>,
    ncx_path: Option<&str>,
) -> HashMap<String, String> {
    let mut out: HashMap<String, String> = HashMap::new();

    if let Some(p) = ncx_path {
        if let Ok(xml) = read_to_string_from_zip(zip, p) {
            let base = parent_dir(p).to_string();
            for (k, v) in parse_ncx_titles(&xml, &base) {
                out.insert(k, v);
            }
        }
    }

    if let Some(p) = nav_path {
        if let Ok(xml) = read_to_string_from_zip(zip, p) {
            let base = parent_dir(p).to_string();
            for (k, v) in parse_nav_titles(&xml, &base) {
                out.insert(k, v);
            }
        }
    }

    out
}

/// Parsed spine entry tagged with its TOC anchor status. Both strategies
/// build a `Vec<Entry>` during the spine walk and then run it through
/// [`fold_into_toc_groups`].
pub(crate) struct Entry {
    pub spine_href: String,
    pub title: String,
    pub body: String,
    /// `true` when the title came from the NCX / nav TOC. Anchors open new
    /// chapter groups; non-anchor entries are merged into the most recent
    /// anchor.
    pub is_toc_hit: bool,
}

/// Collapse consecutive non-TOC-hit entries into the preceding TOC-hit
/// group. The NCX/nav is the canonical chapter list (the EPUB's 目次);
/// spine splits between anchors are pagination artefacts that the reader's
/// chapter list should hide.
///
/// Leading entries before any TOC hit stay as their own chapters — they
/// have no parent to merge into and are typically front matter (cover /
/// copyright / TOC pages).
pub(crate) fn fold_into_toc_groups(entries: Vec<Entry>) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::with_capacity(entries.len());
    let mut in_toc_group = false;
    for e in entries {
        if e.is_toc_hit {
            out.push(e);
            in_toc_group = true;
        } else if in_toc_group {
            let last = out.last_mut().expect("in_toc_group implies a group exists");
            if !last.body.is_empty() {
                last.body.push_str("\n\n");
            }
            last.body.push_str(&e.body);
        } else {
            out.push(e);
        }
    }
    out
}

/// Walk `<nav epub:type="toc">` and pull every `<a href>` / inner text pair.
/// Hrefs are resolved against `nav_base` so the returned keys match spine
/// resolution.
///
/// Sub-entries from a nested `<ol>` are flattened; the hash is href-keyed so
/// nested duplicates collapse onto the first hit. If no nav declares
/// `epub:type="toc"` but the document contains exactly one `<nav>`, that
/// sole nav's entries are kept (some producers drop `epub:type` when there
/// is no ambiguity).
pub(crate) fn parse_nav_titles(xml: &str, nav_base: &str) -> HashMap<String, String> {
    let mut toc_out: HashMap<String, String> = HashMap::new();
    let mut sole_candidate: HashMap<String, String> = HashMap::new();
    let mut saw_toc = false;
    let mut nav_count: usize = 0;

    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();

    let mut nav_depth = 0i32;
    let mut current_is_toc = false;
    let mut pending_href: Option<String> = None;
    let mut text_buf = String::new();
    let mut in_a = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = e.name();
                if name.as_ref() == b"nav" {
                    if nav_depth == 0 {
                        nav_count += 1;
                        current_is_toc = nav_is_toc(&e);
                        if current_is_toc {
                            saw_toc = true;
                        }
                    }
                    nav_depth += 1;
                } else if nav_depth > 0 && name.as_ref() == b"a" {
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
            // Self-closing `<nav .../>` carries no children but still counts
            // toward the "sole nav" fallback. A self-closing toc-typed nav
            // also flips `saw_toc`.
            Ok(Event::Empty(e)) => {
                if e.name().as_ref() == b"nav" && nav_depth == 0 {
                    nav_count += 1;
                    if nav_is_toc(&e) {
                        saw_toc = true;
                    }
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
                            let resolved =
                                join_zip_path(nav_base, path_part(&href)).unwrap_or_default();
                            if !resolved.is_empty() {
                                if current_is_toc {
                                    toc_out.entry(resolved).or_insert(title);
                                } else {
                                    sole_candidate.entry(resolved).or_insert(title);
                                }
                            }
                        }
                    }
                    text_buf.clear();
                    in_a = false;
                } else if name.as_ref() == b"nav" && nav_depth > 0 {
                    nav_depth -= 1;
                    if nav_depth == 0 {
                        current_is_toc = false;
                    }
                }
            }
            Ok(Event::Eof) => break,
            // Fail-soft: stop on the first XML error and return whatever was
            // collected. Partial TOC beats no TOC; spine entries we missed
            // fall through to the per-doc heading scrape.
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    if saw_toc {
        toc_out
    } else if nav_count == 1 {
        sole_candidate
    } else {
        HashMap::new()
    }
}

/// Walk a `toc.ncx`, returning a `resolved zip path → title` map.
///
/// Nested `<navPoint>` trees are flattened. Closing order is child-first,
/// parent-last, and emission uses last-write-wins — so when a child
/// `navPoint` shares the same href as its parent (sub-section anchored to
/// the section's start), the *parent* (outer / section) label is what ends
/// up in the map. ASCII whitespace inside `<text>` is collapsed; U+3000
/// (ideographic space) is preserved as significant content.
pub(crate) fn parse_ncx_titles(xml: &str, ncx_base: &str) -> HashMap<String, String> {
    let mut out: HashMap<String, String> = HashMap::new();

    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();

    // One frame per open <navPoint>. Children close before their parents, so
    // the parent's label/src must not be clobbered by a nested navPoint that
    // happens to share the same href anchor.
    #[derive(Default)]
    struct Frame {
        label: String,
        src: Option<String>,
        in_label_text: bool,
    }
    let mut stack: Vec<Frame> = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                if local == b"navPoint" {
                    stack.push(Frame::default());
                } else if local == b"text" {
                    if let Some(top) = stack.last_mut() {
                        top.label.clear();
                        top.in_label_text = true;
                    }
                }
            }
            // NCX 2005-1 declares `<content>` as `EMPTY` content model — it
            // is always self-closing in conformant NCX. No Start/End handler
            // is needed for `content`.
            Ok(Event::Empty(e)) => {
                let name = e.name();
                if local_name(name.as_ref()) == b"content" {
                    if let Some(top) = stack.last_mut() {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"src" {
                                if let Ok(v) = attr.unescape_value() {
                                    top.src = Some(v.into_owned());
                                }
                            }
                        }
                    }
                }
            }
            Ok(Event::Text(t)) => {
                if let Some(top) = stack.last_mut() {
                    if top.in_label_text {
                        if let Ok(s) = t.unescape() {
                            top.label.push_str(&s);
                        }
                    }
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                if local == b"text" {
                    if let Some(top) = stack.last_mut() {
                        top.in_label_text = false;
                    }
                } else if local == b"navPoint" {
                    if let Some(frame) = stack.pop() {
                        if let Some(src) = frame.src {
                            let title = collapse_ws(&frame.label);
                            if !title.is_empty() {
                                let resolved =
                                    join_zip_path(ncx_base, path_part(&src)).unwrap_or_default();
                                if !resolved.is_empty() {
                                    // Parent closes after its children, so
                                    // last-write-wins lets the outer
                                    // section title overwrite any nested
                                    // subentry that shares the same href.
                                    out.insert(resolved, title);
                                }
                            }
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            // Fail-soft: stop on the first XML error and return whatever was
            // collected. Partial NCX beats no NCX; spine entries we missed
            // fall through to the per-doc heading scrape.
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    out
}

/// Detect `epub:type="toc"` (or any `:type`-suffixed namespaced equivalent)
/// on a `<nav>` element. Used by both the Start and Empty handlers.
fn nav_is_toc(e: &quick_xml::events::BytesStart) -> bool {
    e.attributes().flatten().any(|a| {
        let k = a.key.as_ref();
        (k == b"epub:type" || k.ends_with(b":type") || k == b"type")
            && a.unescape_value()
                .map(|v| v.split_whitespace().any(|t| t == "toc"))
                .unwrap_or(false)
    })
}

fn local_name(qname: &[u8]) -> &[u8] {
    match qname.iter().rposition(|&b| b == b':') {
        Some(i) => &qname[i + 1..],
        None => qname,
    }
}

// Collapse runs of ASCII whitespace inside an NCX label into a single ASCII
// space. U+3000 (ideographic space) and other non-ASCII whitespace are
// preserved as content: Japanese chapter titles like "一章\u{3000}同期"
// use U+3000 as a deliberate visual separator between the chapter number
// and the title, and collapsing it would destroy author intent.
fn collapse_ws(s: &str) -> String {
    fn is_ascii_ws(c: char) -> bool {
        matches!(c, ' ' | '\t' | '\r' | '\n' | '\x0B' | '\x0C')
    }
    let trimmed = s.trim_matches(is_ascii_ws);
    let mut out = String::with_capacity(trimmed.len());
    let mut prev_space = false;
    for c in trimmed.chars() {
        if is_ascii_ws(c) {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    out
}

pub(crate) fn path_part(href: &str) -> &str {
    href.split('#').next().unwrap_or(href)
}

/// Resolve an href against `base`, normalizing `.` / `..` segments — zip
/// lookups are byte-literal, so `OEBPS/./ch1.xhtml` would miss the entry.
/// `..` that climbs above the zip root is rejected.
pub(crate) fn join_zip_path(base: &str, rel: &str) -> Result<String, EpubError> {
    if rel.is_empty() {
        return Err(EpubError::Parse("empty href".into()));
    }
    if rel.starts_with('/') {
        return Err(EpubError::Parse(format!("absolute href rejected: {rel}")));
    }
    let decoded = percent_encoding::percent_decode_str(path_part(rel))
        .decode_utf8()
        .map_err(|_| EpubError::Parse(format!("href not utf-8: {rel}")))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ncx_titles_basic() {
        let xml = r#"<?xml version="1.0"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <navMap>
    <navPoint id="n1" playOrder="1">
      <navLabel><text>Cover</text></navLabel>
      <content src="cover.xhtml"/>
    </navPoint>
    <navPoint id="n2" playOrder="2">
      <navLabel><text>第一章 始まり</text></navLabel>
      <content src="ch1.xhtml#anchor"/>
    </navPoint>
  </navMap>
</ncx>"#;
        let m = parse_ncx_titles(xml, "OEBPS");
        assert_eq!(
            m.get("OEBPS/cover.xhtml").map(String::as_str),
            Some("Cover")
        );
        assert_eq!(
            m.get("OEBPS/ch1.xhtml").map(String::as_str),
            Some("第一章 始まり"),
        );
    }

    /// When an inner `<navPoint>` shares the href of its parent, the parent
    /// (section / outer) label must win — the section title is more useful
    /// than a sub-anchor label. Implementation relies on close-order:
    /// children close first, parent closes last, `out.insert` is
    /// last-write-wins.
    #[test]
    fn ncx_titles_nested_outer_navpoint_label_overrides_inner() {
        let xml = r#"<?xml version="1.0"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <navMap>
    <navPoint id="n1">
      <navLabel><text>Part One</text></navLabel>
      <content src="part1.xhtml"/>
      <navPoint id="n1.1">
        <navLabel><text>Section A</text></navLabel>
        <content src="part1.xhtml#sa"/>
      </navPoint>
    </navPoint>
  </navMap>
</ncx>"#;
        let m = parse_ncx_titles(xml, "OEBPS");
        assert_eq!(
            m.get("OEBPS/part1.xhtml").map(String::as_str),
            Some("Part One")
        );
    }

    #[test]
    fn ncx_titles_preserve_ideographic_space() {
        // U+3000 between the chapter number and title must survive — it is
        // a deliberate visual separator in Japanese chapter titles, not
        // collapsable whitespace.
        let xml = "<ncx><navMap><navPoint><navLabel><text>\u{4e00}\u{7ae0}\u{3000}\
\u{540c}\u{671f}</text></navLabel><content src=\"x.xhtml\"/></navPoint></navMap></ncx>";
        let m = parse_ncx_titles(xml, "");
        assert_eq!(
            m.get("x.xhtml").map(String::as_str),
            Some("\u{4e00}\u{7ae0}\u{3000}\u{540c}\u{671f}"),
        );
    }

    #[test]
    fn ncx_titles_collapses_multiline_label() {
        let xml = r#"<ncx>
  <navMap>
    <navPoint>
      <navLabel><text>
        Multi
        line
        label
      </text></navLabel>
      <content src="x.xhtml"/>
    </navPoint>
  </navMap>
</ncx>"#;
        let m = parse_ncx_titles(xml, "");
        assert_eq!(
            m.get("x.xhtml").map(String::as_str),
            Some("Multi line label")
        );
    }

    #[test]
    fn parse_opf_refs_picks_ncx_via_spine_toc() {
        let opf = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0">
  <manifest>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
    <item id="c1"  href="ch1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine toc="ncx"><itemref idref="c1"/></spine>
</package>"#;
        let refs = parse_opf_refs(opf, "OEBPS/content.opf").unwrap();
        assert_eq!(refs.ncx_path.as_deref(), Some("OEBPS/toc.ncx"));
        assert!(refs.nav_path.is_none());
        assert_eq!(refs.spine, vec!["ch1.xhtml"]);
    }

    #[test]
    fn parse_opf_refs_picks_ncx_via_media_type_only() {
        let opf = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0">
  <manifest>
    <item id="anything" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
    <item id="c1"  href="ch1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="c1"/></spine>
</package>"#;
        let refs = parse_opf_refs(opf, "OEBPS/content.opf").unwrap();
        assert_eq!(refs.ncx_path.as_deref(), Some("OEBPS/toc.ncx"));
    }

    #[test]
    fn nav_titles_basic() {
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
    fn nav_titles_ignores_page_list_sibling_nav() {
        let nav = r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<body>
  <nav epub:type="toc"><ol><li><a href="ch1.xhtml">Real Chapter</a></li></ol></nav>
  <nav epub:type="page-list"><ol><li><a href="ch1.xhtml#p1">1</a></li></ol></nav>
</body></html>"#;
        let m = parse_nav_titles(nav, "OEBPS");
        assert_eq!(m.len(), 1);
        assert_eq!(
            m.get("OEBPS/ch1.xhtml").map(String::as_str),
            Some("Real Chapter")
        );
    }

    #[test]
    fn nav_titles_sole_nav_without_epub_type_accepted() {
        let nav = r#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<body><nav><ol><li><a href="ch1.xhtml">Only Chapter</a></li></ol></nav></body></html>"#;
        let m = parse_nav_titles(nav, "OEBPS");
        assert_eq!(
            m.get("OEBPS/ch1.xhtml").map(String::as_str),
            Some("Only Chapter")
        );
    }

    #[test]
    fn join_zip_path_normalizes_dot_segments() {
        assert_eq!(
            join_zip_path("OEBPS", "./ch1.xhtml").unwrap(),
            "OEBPS/ch1.xhtml"
        );
        assert_eq!(
            join_zip_path("OEBPS", "a/./b.xhtml").unwrap(),
            "OEBPS/a/b.xhtml"
        );
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
    fn join_zip_path_percent_decodes() {
        assert_eq!(
            join_zip_path("OEBPS", "Chapter%201.xhtml").unwrap(),
            "OEBPS/Chapter 1.xhtml"
        );
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

    fn entry(href: &str, title: &str, body: &str, hit: bool) -> Entry {
        Entry {
            spine_href: href.into(),
            title: title.into(),
            body: body.into(),
            is_toc_hit: hit,
        }
    }

    #[test]
    fn fold_collapses_continuations_into_toc_anchor() {
        let groups = fold_into_toc_groups(vec![
            entry("p1.xhtml", "Prologue", "start", true),
            entry("p2.xhtml", "ignored", "more", false),
            entry("p3.xhtml", "ignored", "tail", false),
            entry("c1.xhtml", "Chapter 1", "ch1 start", true),
        ]);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].title, "Prologue");
        assert_eq!(groups[0].body, "start\n\nmore\n\ntail");
        assert_eq!(groups[1].title, "Chapter 1");
        assert_eq!(groups[1].body, "ch1 start");
    }

    #[test]
    fn fold_keeps_leading_non_toc_entries_separate() {
        // Front matter before the first TOC hit has no parent to merge into.
        let groups = fold_into_toc_groups(vec![
            entry("cover.xhtml", "Cover", "cover body", false),
            entry("title.xhtml", "Title", "title body", false),
            entry("p1.xhtml", "Prologue", "prologue body", true),
            entry("p2.xhtml", "ignored", "prologue tail", false),
        ]);
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].title, "Cover");
        assert_eq!(groups[1].title, "Title");
        assert_eq!(groups[2].title, "Prologue");
        assert_eq!(groups[2].body, "prologue body\n\nprologue tail");
    }

    #[test]
    fn fold_passes_through_when_no_toc_hits() {
        // No NCX, no nav: every spine entry stays its own chapter.
        let groups = fold_into_toc_groups(vec![
            entry("a.xhtml", "A", "a", false),
            entry("b.xhtml", "B", "b", false),
            entry("c.xhtml", "C", "c", false),
        ]);
        assert_eq!(groups.len(), 3);
        assert_eq!(
            groups.iter().map(|e| e.title.as_str()).collect::<Vec<_>>(),
            vec!["A", "B", "C"],
        );
    }

    #[test]
    fn parse_opf_refs_picks_nav_via_properties() {
        let opf = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="c1"  href="ch1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="c1"/></spine>
</package>"#;
        let refs = parse_opf_refs(opf, "OEBPS/content.opf").unwrap();
        assert_eq!(refs.nav_path.as_deref(), Some("OEBPS/nav.xhtml"));
        assert!(refs.ncx_path.is_none());
    }
}
