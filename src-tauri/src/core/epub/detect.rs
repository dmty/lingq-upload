//! EPUB vendor discriminator.
//!
//! Decides between Kindle / Kobo / Generic before the parse pass so the right
//! [`super::HeadingStrategy`] can be picked centrally. Counting is per-file:
//! a single CSS rule with three repetitions of `font-160per` does not flip
//! detection on its own, and only body XHTML files contribute to the cluster
//! decision. Markers buried in `<!-- … -->` comments are excluded.
//!
//! Signals are reported back to the caller so logs and tests can observe which
//! markers fired without re-running the scan.

use std::io::Read;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::EpubError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum EpubVendor {
    Kindle,
    Kobo,
    Generic,
}

impl EpubVendor {
    pub fn as_str(self) -> &'static str {
        match self {
            EpubVendor::Kindle => "kindle",
            EpubVendor::Kobo => "kobo",
            EpubVendor::Generic => "generic",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct VendorDetection {
    pub vendor: EpubVendor,
    pub confidence: f32,
    pub signals: Vec<String>,
}

/// Cluster floor: a single body file must hit this many combined Kobo marker
/// occurrences (koboSpan + font-per accumulate), OR this many distinct body
/// files must each carry at least one hit, before Kobo classification fires.
/// Anything less risks a stray CSS rule flipping a Kindle book.
const KOBO_CLUSTER_FLOOR: usize = 3;

/// Cap on body files scanned. Real Kobo books spread markers across many
/// files; a small window is enough to clear the cluster floor without
/// touching every chapter.
const MAX_BODY_FILES_SCANNED: usize = 12;

/// Read budget per file. Kobo markers cluster near the top of each chapter;
/// avoiding multi-megabyte reads keeps detection cheap for picture books.
const MAX_BYTES_PER_FILE: usize = 64 * 1024;

#[derive(Clone, Copy)]
enum FileKind {
    Body,
    Css,
    Ncx,
}

pub fn detect_vendor<R: std::io::Read + std::io::Seek>(
    zip: &mut zip::ZipArchive<R>,
) -> Result<VendorDetection, EpubError> {
    let candidates = collect_candidate_files(zip)?;

    let mut max_kobo_span_in_body = 0usize;
    let mut max_font_per_in_body = 0usize;
    let mut max_kobo_total_in_body = 0usize;
    let mut total_kobo_in_bodies = 0usize;
    let mut bodies_with_kobo_hit = 0usize;
    let mut kindle_marker_hits = 0usize;
    let mut has_ncx = false;

    for (name, kind) in &candidates {
        // NCX existence is the signal; the content is never inspected, so it
        // is the one kind that skips the read.
        if let FileKind::Ncx = kind {
            has_ncx = true;
            continue;
        }
        let body = match read_capped(zip, name) {
            Ok(b) => b,
            Err(_) => continue,
        };
        match kind {
            FileKind::Body => {
                let stripped = strip_xml_comments(&body);
                let kobo_span = count_kobo_span(&stripped);
                let font_per = count_font_per(&stripped);
                let kobo_total = kobo_span + font_per;
                max_kobo_span_in_body = max_kobo_span_in_body.max(kobo_span);
                max_font_per_in_body = max_font_per_in_body.max(font_per);
                max_kobo_total_in_body = max_kobo_total_in_body.max(kobo_total);
                total_kobo_in_bodies = total_kobo_in_bodies.saturating_add(kobo_total);
                if kobo_total > 0 {
                    bodies_with_kobo_hit += 1;
                }
                kindle_marker_hits =
                    kindle_marker_hits.saturating_add(count_kindle_markers(&stripped));
            }
            FileKind::Css | FileKind::Ncx => {
                // CSS contributes Kindle markers only after comment stripping;
                // it never feeds the Kobo body cluster. (Ncx unreachable here.)
                let stripped = strip_css_comments(&body);
                kindle_marker_hits =
                    kindle_marker_hits.saturating_add(count_kindle_markers(&stripped));
            }
        }
    }

    let mut signals: Vec<String> = Vec::new();
    if max_kobo_span_in_body > 0 {
        signals.push(label_count("kobo_span", max_kobo_span_in_body).to_string());
    }
    if max_font_per_in_body > 0 {
        signals.push(label_count("font_per", max_font_per_in_body).to_string());
    }
    if kindle_marker_hits > 0 {
        signals.push(label_count("kindle_marker", kindle_marker_hits).to_string());
    }
    if has_ncx {
        signals.push("toc_ncx".to_string());
    }

    let is_kobo_cluster = max_kobo_total_in_body >= KOBO_CLUSTER_FLOOR
        || bodies_with_kobo_hit >= KOBO_CLUSTER_FLOOR;

    let (vendor, confidence) = if is_kobo_cluster {
        let conf = 0.6 + (total_kobo_in_bodies.min(12) as f32) / 30.0;
        (EpubVendor::Kobo, conf.min(0.99))
    } else if kindle_marker_hits >= 2 || (has_ncx && kindle_marker_hits >= 1) {
        let conf = 0.6 + (kindle_marker_hits.min(8) as f32) / 20.0;
        (EpubVendor::Kindle, conf.min(0.99))
    } else if has_ncx {
        (EpubVendor::Kindle, 0.6)
    } else {
        let weak = total_kobo_in_bodies + kindle_marker_hits;
        let conf = if weak == 0 { 0.2 } else { 0.35 };
        (EpubVendor::Generic, conf)
    };

    Ok(VendorDetection {
        vendor,
        confidence,
        signals,
    })
}

fn collect_candidate_files<R: std::io::Read + std::io::Seek>(
    zip: &mut zip::ZipArchive<R>,
) -> Result<Vec<(String, FileKind)>, EpubError> {
    let mut out: Vec<(String, FileKind)> = Vec::new();
    let mut body_count = 0usize;
    for i in 0..zip.len() {
        let name = {
            let entry = zip
                .by_index(i)
                .map_err(|e| EpubError::Zip(e.to_string()))?;
            entry.name().to_string()
        };
        let lower = name.to_ascii_lowercase();
        let kind = if lower.ends_with(".xhtml")
            || lower.ends_with(".html")
            || lower.ends_with(".htm")
        {
            FileKind::Body
        } else if lower.ends_with(".css") {
            FileKind::Css
        } else if lower.ends_with(".ncx") {
            FileKind::Ncx
        } else {
            continue;
        };
        if matches!(kind, FileKind::Body) {
            // Cap applies in zip-entry order, not spine order: >12 marker-free
            // front files degrade Kobo→Kindle, which still parses acceptably —
            // bounded scan cost wins over that edge case.
            if body_count >= MAX_BODY_FILES_SCANNED {
                continue;
            }
            body_count += 1;
        }
        out.push((name, kind));
    }
    Ok(out)
}

fn read_capped<R: std::io::Read + std::io::Seek>(
    zip: &mut zip::ZipArchive<R>,
    name: &str,
) -> Result<Vec<u8>, EpubError> {
    let mut f = zip
        .by_name(name)
        .map_err(|e| EpubError::Parse(format!("missing {name}: {e}")))?;
    let mut buf = Vec::with_capacity(MAX_BYTES_PER_FILE.min(8192));
    let mut chunk = [0u8; 8192];
    loop {
        let n = f
            .read(&mut chunk)
            .map_err(|e| EpubError::Io(e.to_string()))?;
        if n == 0 {
            break;
        }
        let room = MAX_BYTES_PER_FILE.saturating_sub(buf.len());
        if room == 0 {
            break;
        }
        let take = n.min(room);
        buf.extend_from_slice(&chunk[..take]);
        if buf.len() >= MAX_BYTES_PER_FILE {
            break;
        }
    }
    Ok(buf)
}

/// Drop `<!-- … -->` blocks in-place. Tolerant of unterminated comments at
/// EOF (the cap can chop one mid-stream): treat the rest as comment.
fn strip_xml_comments(bytes: &[u8]) -> Vec<u8> {
    strip_paired(bytes, b"<!--", b"-->")
}

fn strip_css_comments(bytes: &[u8]) -> Vec<u8> {
    strip_paired(bytes, b"/*", b"*/")
}

fn strip_paired(bytes: &[u8], open: &[u8], close: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if i + open.len() <= bytes.len() && &bytes[i..i + open.len()] == open {
            let rest_start = i + open.len();
            match find_subslice(&bytes[rest_start..], close) {
                Some(off) => i = rest_start + off + close.len(),
                None => break,
            }
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    for i in 0..=haystack.len() - needle.len() {
        if &haystack[i..i + needle.len()] == needle {
            return Some(i);
        }
    }
    None
}

fn count_kobo_span(bytes: &[u8]) -> usize {
    count_subslices_ascii_ci(bytes, b"kobospan")
}

/// Matches Kobo's per-chapter font-size hint classes `font-1[246]0per` such as
/// `font-120per`, `font-140per`, `font-160per`. The middle digit is the only
/// variant seen in production Kobo exports.
fn count_font_per(bytes: &[u8]) -> usize {
    let prefix = b"font-1";
    let suffix = b"0per";
    let mut hits = 0usize;
    let lower = ascii_lower(bytes);
    let mut i = 0;
    while i + prefix.len() + 1 + suffix.len() <= lower.len() {
        if &lower[i..i + prefix.len()] == prefix {
            let mid = lower[i + prefix.len()];
            if (mid == b'2' || mid == b'4' || mid == b'6')
                && &lower[i + prefix.len() + 1..i + prefix.len() + 1 + suffix.len()] == suffix
            {
                hits += 1;
                i += prefix.len() + 1 + suffix.len();
                continue;
            }
        }
        i += 1;
    }
    hits
}

fn count_kindle_markers(bytes: &[u8]) -> usize {
    let mut hits = 0usize;
    hits += count_subslices_ascii_ci(bytes, b"amzn-page-break");
    hits += count_subslices_ascii_ci(bytes, b"kindle:embed");
    hits += count_subslices_ascii_ci(bytes, b"kindle:position");
    hits += count_subslices_ascii_ci(bytes, b"kf8");
    hits += count_subslices_ascii_ci(bytes, b"\"calibre");
    hits
}

fn ascii_lower(bytes: &[u8]) -> Vec<u8> {
    bytes.iter().map(|b| b.to_ascii_lowercase()).collect()
}

fn count_subslices_ascii_ci(haystack: &[u8], needle_lower: &[u8]) -> usize {
    if needle_lower.is_empty() || haystack.len() < needle_lower.len() {
        return 0;
    }
    let mut hits = 0usize;
    let mut i = 0;
    while i + needle_lower.len() <= haystack.len() {
        let mut matched = true;
        for j in 0..needle_lower.len() {
            if haystack[i + j].to_ascii_lowercase() != needle_lower[j] {
                matched = false;
                break;
            }
        }
        if matched {
            hits += 1;
            i += needle_lower.len();
        } else {
            i += 1;
        }
    }
    hits
}

/// Bucket counts into a stable signal label so test assertions don't break on
/// off-by-one fluctuations from minor fixture edits.
fn label_count(prefix: &str, n: usize) -> &'static str {
    macro_rules! pick {
        ($p:literal) => {
            match n {
                0 => concat!($p, "_x0"),
                1 => concat!($p, "_x1"),
                2 => concat!($p, "_x2"),
                3..=5 => concat!($p, "_x3+"),
                6..=11 => concat!($p, "_x6+"),
                _ => concat!($p, "_x12+"),
            }
        };
    }
    match prefix {
        "kobo_span" => pick!("kobo_span"),
        "font_per" => pick!("font_per"),
        "kindle_marker" => pick!("kindle_marker"),
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_per_matches_only_2_4_6() {
        assert_eq!(count_font_per(b"font-120per"), 1);
        assert_eq!(count_font_per(b"font-140per"), 1);
        assert_eq!(count_font_per(b"font-160per"), 1);
        assert_eq!(count_font_per(b"font-130per"), 0);
        assert_eq!(count_font_per(b"font-100per"), 0);
    }

    #[test]
    fn font_per_is_case_insensitive() {
        assert_eq!(count_font_per(b"FONT-160PER"), 1);
        assert_eq!(count_font_per(b"Font-140Per"), 1);
    }

    #[test]
    fn kobo_span_is_case_insensitive() {
        assert_eq!(count_kobo_span(b"<span class=\"koboSpan\""), 1);
        assert_eq!(count_kobo_span(b"KOBOSPAN KOBOSPAN"), 2);
    }

    #[test]
    fn strip_xml_comments_drops_block() {
        let s = b"a<!-- koboSpan koboSpan -->b<!-- x -->c";
        let out = strip_xml_comments(s);
        assert_eq!(out, b"abc");
    }

    #[test]
    fn strip_css_comments_drops_block() {
        let s = b".a { /* font-160per */ } .b { color: red }";
        let out = strip_css_comments(s);
        assert!(!out.windows(11).any(|w| w == b"font-160per"));
    }

    #[test]
    fn as_str_matches_serde_rename() {
        for v in [EpubVendor::Kindle, EpubVendor::Kobo, EpubVendor::Generic] {
            let json = serde_json::to_value(v).unwrap();
            assert_eq!(
                json.as_str().expect("serialized as string"),
                v.as_str(),
                "as_str diverged from serde for {v:?}",
            );
        }
    }
}
