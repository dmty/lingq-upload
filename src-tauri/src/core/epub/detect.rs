//! EPUB vendor discriminator.
//!
//! Decides between Kindle / Kobo / Generic before the parse pass so the right
//! [`super::HeadingStrategy`] can be picked centrally. Heuristic is intentionally
//! cluster-based: a single stray `font-160per` span buried in an otherwise-
//! Kindle book must not flip the classification.
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

/// Cluster floor: koboSpan or font-1[246]0per hits must reach this many
/// distinct occurrences before Kobo classification fires. Lower numbers
/// produced false positives in adversarial fixtures where a stylesheet
/// happened to carry one stray Kobo-styled span.
const KOBO_CLUSTER_FLOOR: usize = 3;

/// How many spine entries (best-effort: just the first N XHTML-looking entries
/// in zip order) get scanned. Real Kobo books spread markers across many files
/// so even a small window is enough to clear the cluster floor without
/// touching every chapter.
const MAX_FILES_SCANNED: usize = 12;

/// Read budget per file. Kobo markers cluster near the top of each chapter;
/// avoiding multi-megabyte reads keeps detection cheap for picture books.
const MAX_BYTES_PER_FILE: usize = 64 * 1024;

pub fn detect_vendor<R: std::io::Read + std::io::Seek>(
    zip: &mut zip::ZipArchive<R>,
) -> Result<VendorDetection, EpubError> {
    let candidates = collect_candidate_files(zip);

    let mut kobo_span_hits = 0usize;
    let mut font_per_hits = 0usize;
    let mut kindle_marker_hits = 0usize;
    let mut has_ncx = false;

    for name in &candidates {
        let body = match read_capped(zip, name) {
            Ok(b) => b,
            Err(_) => continue,
        };
        kobo_span_hits = kobo_span_hits.saturating_add(count_kobo_span(&body));
        font_per_hits = font_per_hits.saturating_add(count_font_per(&body));
        kindle_marker_hits = kindle_marker_hits.saturating_add(count_kindle_markers(&body));
        if name.ends_with(".ncx") {
            has_ncx = true;
        }
    }

    let mut signals: Vec<String> = Vec::new();
    if kobo_span_hits > 0 {
        signals.push(label_count("kobo_span", kobo_span_hits).to_string());
    }
    if font_per_hits > 0 {
        signals.push(label_count("font_per", font_per_hits).to_string());
    }
    if kindle_marker_hits > 0 {
        signals.push(label_count("kindle_marker", kindle_marker_hits).to_string());
    }
    if has_ncx {
        signals.push("toc_ncx".to_string());
    }

    let kobo_cluster = kobo_span_hits.max(font_per_hits);
    let kobo_total = kobo_span_hits + font_per_hits;

    let (vendor, confidence) = if kobo_cluster >= KOBO_CLUSTER_FLOOR {
        // Saturate at ~12 hits for full confidence; more hits don't add signal.
        let conf = 0.6 + (kobo_total.min(12) as f32) / 30.0;
        (EpubVendor::Kobo, conf.min(0.99))
    } else if kindle_marker_hits >= 2 || (has_ncx && kindle_marker_hits >= 1) {
        let conf = 0.6 + (kindle_marker_hits.min(8) as f32) / 20.0;
        (EpubVendor::Kindle, conf.min(0.99))
    } else if has_ncx {
        // toc.ncx alone is a weak Kindle/EPUB2 hint — strong enough to beat
        // Generic, weak enough that it doesn't dwarf a Kobo cluster.
        (EpubVendor::Kindle, 0.6)
    } else {
        // Empty, encrypted, or simply unknown. One stray marker stays here.
        let weak = kobo_total + kindle_marker_hits;
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
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for i in 0..zip.len() {
        let Ok(f) = zip.by_index(i) else { continue };
        let name = f.name().to_string();
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".xhtml")
            || lower.ends_with(".html")
            || lower.ends_with(".htm")
            || lower.ends_with(".ncx")
            || lower.ends_with(".opf")
        {
            out.push(name);
            if out.len() >= MAX_FILES_SCANNED {
                break;
            }
        }
    }
    out
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
}
