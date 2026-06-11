//! Kobo heading strategy.
//!
//! Kobo EPUBs are EPUB3 and ship an HTML navigation document (`nav.xhtml`).
//! The strategy walks the OPF spine in order and resolves each chapter's
//! title from the nav `<ol>`, falling back to NCX (if present) and then to
//! the spine file's `<title>`. Heuristic front/back-matter tagging keys off
//! normalised title prefixes.

use std::collections::HashSet;

use super::body::clean_chapter_body;
use super::toc::{fold_into_toc_groups, join_zip_path, parse_opf_refs, read_toc_titles, Entry};
use super::{
    find_case_insensitive, normalize_title, read_container_opf_path, read_to_string_from_zip,
    Chapter, ChapterId, ChapterKind, EpubError,
};

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
const MATTER_TITLES: &[(&str, ChapterKind)] = &[
    ("cover", ChapterKind::FrontMatter),
    ("title", ChapterKind::FrontMatter),
    ("title page", ChapterKind::FrontMatter),
    ("copyright", ChapterKind::FrontMatter),
    ("imprint", ChapterKind::FrontMatter),
    ("dedication", ChapterKind::FrontMatter),
    ("epigraph", ChapterKind::FrontMatter),
    ("preface", ChapterKind::FrontMatter),
    ("foreword", ChapterKind::FrontMatter),
    ("prologue", ChapterKind::FrontMatter),
    ("contents", ChapterKind::FrontMatter),
    ("table of contents", ChapterKind::FrontMatter),
    ("目次", ChapterKind::FrontMatter),
    ("まえがき", ChapterKind::FrontMatter),
    ("はじめに", ChapterKind::FrontMatter),
    ("序文", ChapterKind::FrontMatter),
    ("序章", ChapterKind::FrontMatter),
    ("acknowledgments", ChapterKind::BackMatter),
    ("acknowledgements", ChapterKind::BackMatter),
    ("about the author", ChapterKind::BackMatter),
    ("about the publisher", ChapterKind::BackMatter),
    ("afterword", ChapterKind::BackMatter),
    ("epilogue", ChapterKind::BackMatter),
    ("appendix", ChapterKind::BackMatter),
    ("bibliography", ChapterKind::BackMatter),
    ("glossary", ChapterKind::BackMatter),
    ("index", ChapterKind::BackMatter),
    ("notes", ChapterKind::BackMatter),
    ("colophon", ChapterKind::BackMatter),
    ("奥付", ChapterKind::BackMatter),
    ("あとがき", ChapterKind::BackMatter),
    ("解説", ChapterKind::BackMatter),
    ("謝辞", ChapterKind::BackMatter),
];

pub fn parse_from_zip<R: std::io::Read + std::io::Seek>(
    zip: &mut zip::ZipArchive<R>,
) -> Result<Vec<Chapter>, EpubError> {
    let opf_path = read_container_opf_path(zip)?;
    let opf_xml = read_to_string_from_zip(zip, &opf_path)?;
    let refs = parse_opf_refs(&opf_xml, &opf_path)?;
    let toc_titles = read_toc_titles(zip, refs.nav_path.as_deref(), refs.ncx_path.as_deref());

    // Calibre-converted EPUBs stamp the book title into every spine file's
    // `<head><title>`. Using that as a chapter fallback would paint the
    // whole chapter list with the book name — so head titles that recur
    // across spine entries are treated as the book title and dropped.
    struct Raw {
        spine_href: String,
        spine_index: usize,
        body: String,
        head_title: Option<String>,
        toc_title: Option<String>,
    }
    let mut raws: Vec<Raw> = Vec::with_capacity(refs.spine.len());
    let mut first_skip: Option<EpubError> = None;
    for (i, href) in refs.spine.iter().enumerate() {
        let full = match join_zip_path(&refs.base_dir, href) {
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
        raws.push(Raw {
            spine_href: href.clone(),
            spine_index: i,
            body,
            head_title: extract_html_title(&raw),
            toc_title: toc_titles.get(&full).cloned(),
        });
    }

    // Each repeated title is cloned once (on first detected duplicate), not
    // once per occurrence.
    let mut seen: HashSet<&str> = HashSet::new();
    let mut book_titles: HashSet<String> = HashSet::new();
    for r in &raws {
        if let Some(t) = r.head_title.as_deref() {
            if !seen.insert(t) {
                book_titles.insert(t.to_string());
            }
        }
    }
    drop(seen);

    let parsed: Vec<Entry> = raws
        .into_iter()
        .map(|r| {
            let (title, is_toc_hit) = match r.toc_title {
                Some(t) => (t, true),
                None => {
                    let usable_head = r.head_title.filter(|t| !book_titles.contains(t));
                    (
                        usable_head.unwrap_or_else(|| format!("Chapter {}", r.spine_index + 1)),
                        false,
                    )
                }
            };
            Entry {
                spine_href: r.spine_href,
                title,
                body: r.body,
                is_toc_hit,
            }
        })
        .collect();

    // An all-skips outcome is a broken book (wrong encoding, bad hrefs), not
    // an empty one — surface it instead of returning Ok(vec![]).
    if parsed.is_empty() && !refs.spine.is_empty() {
        let detail = first_skip
            .map(|e| format!("; first skip: {e}"))
            .unwrap_or_else(|| "; all spine bodies empty after cleaning".into());
        return Err(EpubError::Parse(format!(
            "0 of {} spine entries yielded chapters{detail}",
            refs.spine.len()
        )));
    }

    let groups = fold_into_toc_groups(parsed);

    let mut chapters: Vec<Chapter> = Vec::with_capacity(groups.len());
    for (i, g) in groups.into_iter().enumerate() {
        let kind = classify_kind(&g.title);
        let id = ChapterId::from_chapter_parts(KoboStrategy::NAME, &g.spine_href, &g.title);
        chapters.push(Chapter {
            order: i,
            title: g.title,
            body: g.body,
            id,
            kind,
        });
    }
    Ok(chapters)
}

fn classify_kind(title: &str) -> ChapterKind {
    let norm = normalize_title(title);
    for &(needle, kind) in MATTER_TITLES {
        if needle == norm {
            return kind;
        }
    }
    ChapterKind::Body
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
        assert_eq!(classify_kind("About the Author"), ChapterKind::BackMatter);
        assert_eq!(classify_kind("Acknowledgments"), ChapterKind::BackMatter);
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
    fn classify_kind_cover_notes_stays_body() {
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
}
