//! Kindle / generic heading strategy.
//!
//! Most non-Kobo EPUBs ship an NCX (`toc.ncx`) and/or an EPUB3 nav document.
//! The strategy walks the OPF spine in order, resolves each chapter's title
//! from the merged nav + NCX map, falling back to per-doc `<h1>/<h2>/<h3>`
//! and then `"Chapter N"`. Spine entries between TOC anchors are folded
//! into the preceding anchor — see [`super::toc::fold_into_toc_groups`].

use super::body::clean_chapter_body;
use super::toc::{fold_into_toc_groups, join_zip_path, parse_opf_refs, read_toc_titles, Entry};
use super::{
    find_case_insensitive, read_container_opf_path, read_to_string_from_zip, Chapter, ChapterId,
    EpubError,
};

/// Marker type for the Kindle strategy. The runtime entry point is
/// [`parse_from_zip`]; the marker exists so callers can name the strategy
/// without instantiating the enum.
pub struct KindleStrategy;

impl KindleStrategy {
    pub const NAME: &'static str = "kindle";
}

pub fn parse_from_zip<R: std::io::Read + std::io::Seek>(
    zip: &mut zip::ZipArchive<R>,
) -> Result<Vec<Chapter>, EpubError> {
    let opf_path = read_container_opf_path(zip)?;
    let opf_xml = read_to_string_from_zip(zip, &opf_path)?;
    let refs = parse_opf_refs(&opf_xml, &opf_path)?;

    // Many trade EPUBs style chapter titles as `<p>` (esp. Japanese vertical
    // layout); nav / NCX is the only reliable title source for those.
    let toc_titles = read_toc_titles(zip, refs.nav_path.as_deref(), refs.ncx_path.as_deref());

    let mut parsed: Vec<Entry> = Vec::with_capacity(refs.spine.len());
    for (i, href) in refs.spine.iter().enumerate() {
        let full = match join_zip_path(&refs.base_dir, href) {
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
        let (title, is_toc_hit) = match toc_titles.get(&full).cloned() {
            Some(t) => (t, true),
            None => (
                extract_first_heading(&raw).unwrap_or_else(|| format!("Chapter {}", i + 1)),
                false,
            ),
        };
        parsed.push(Entry {
            spine_href: href.clone(),
            title,
            body,
            is_toc_hit,
        });
    }

    let groups = fold_into_toc_groups(parsed);

    let mut chapters = Vec::with_capacity(groups.len());
    for (i, g) in groups.into_iter().enumerate() {
        let id = ChapterId::from_chapter_parts(KindleStrategy::NAME, &g.spine_href, &g.title);
        chapters.push(Chapter {
            order: i,
            title: g.title,
            body: g.body,
            id,
            spine_href: g.spine_href,
            ..Default::default()
        });
    }
    Ok(chapters)
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
            let txt = super::body::clean_chapter_body(inner).trim().to_string();
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
            let term = bytes.get(i + 3).copied().unwrap_or(b' ');
            if matches!(term, b' ' | b'\t' | b'\n' | b'\r' | b'>' | b'/') {
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
