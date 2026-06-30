mod body;
pub mod cover;
pub mod detect;
pub mod kindle;
pub mod kobo;
pub mod parse;
mod toc;

use std::fmt;
use std::io::Read;
use std::path::Path;

use quick_xml::events::Event;
use quick_xml::Reader;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;
use thiserror::Error;
use unicode_normalization::UnicodeNormalization;

pub use detect::{detect_vendor, EpubVendor, VendorDetection};
pub use kobo::KoboStrategy;
pub use parse::{parse_epub, parse_epub_bytes, parse_epub_with_strategy, HeadingStrategy};

/// Open the EPUB at `path` and run vendor detection. Convenience wrapper used
/// by the orchestrator so the chosen vendor can be logged on `JobEvent::Started`
/// before parsing kicks off.
pub fn autodetect_vendor(path: &Path) -> Result<VendorDetection, EpubError> {
    let bytes = std::fs::read(path)?;
    autodetect_vendor_bytes(&bytes)
}

/// In-memory variant. The orchestrator slurps the file once and feeds both
/// detection and parse through this so we don't reopen + re-build the zip
/// twice on the hot path.
pub fn autodetect_vendor_bytes(bytes: &[u8]) -> Result<VendorDetection, EpubError> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| EpubError::Zip(e.to_string()))?;
    detect_vendor(&mut zip)
}

/// Position of a chapter within a project's text.
///
/// Tagged by the heading strategy at parse time. `Body` is the default;
/// `FrontMatter` / `BackMatter` flag preface / epilogue chapters so the UI
/// can preselect them as skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChapterKind {
    #[default]
    Body,
    FrontMatter,
    BackMatter,
}

/// Stable identity for a parsed chapter.
///
/// EPUB parsers build the id via [`ChapterId::from_chapter_parts`] so the same
/// EPUB bytes produce the same id set across runs. Non-EPUB ingest paths
/// (loose files, manifests) fall back to [`ChapterId::from_order`] — those
/// chapter sets are anchored by file system layout, not a heading strategy.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type, Default)]
pub struct ChapterId(pub String);

impl fmt::Display for ChapterId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl ChapterId {
    /// Order-anchored id for ingest sources without a heading strategy
    /// (loose-files, chapter manifests).
    pub fn from_order(order: usize) -> Self {
        Self(format!("idx:{order}"))
    }

    /// Inverse of [`from_order`]: returns the embedded index for order-shaped
    /// ids (`idx:{n}`), or `None` for any other form.
    pub(crate) fn order_index(&self) -> Option<usize> {
        self.0.strip_prefix("idx:").and_then(|s| s.parse().ok())
    }

    /// Deterministic id derived from `(strategy, spine_key, title_norm)`.
    /// `spine_key` is a stable per-chapter anchor — typically the spine href
    /// — so dropping an empty chapter does not shift the ids of later ones.
    /// The same EPUB bytes parsed by the same strategy always yield the same
    /// id, so persisted selections survive a re-parse.
    pub fn from_chapter_parts(strategy: &str, spine_key: &str, title: &str) -> Self {
        let title_norm = normalize_title(title);
        let mut h = Sha256::new();
        h.update(strategy.as_bytes());
        h.update(b"|");
        h.update(spine_key.as_bytes());
        h.update(b"|");
        h.update(title_norm.as_bytes());
        let digest = h.finalize();
        let hex16 = hex::encode(digest)[..16].to_string();
        Self(format!("{strategy}:{hex16}"))
    }
}

/// Lowercased, NFKC-normalised, whitespace-collapsed title with
/// `Default_Ignorable_Code_Point` characters dropped. Strips ZWJ/ZWNJ, BOM,
/// soft hyphen, and variation selectors so a re-save that inserts or removes
/// them does not flip the chapter id hash.
pub(crate) fn normalize_title(s: &str) -> String {
    let nfkc: String = s.nfkc().collect();
    let lower = nfkc.to_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut prev_space = false;
    for c in lower.chars() {
        if is_default_ignorable(c) {
            continue;
        }
        if c.is_whitespace() {
            if !prev_space && !out.is_empty() {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

/// Decompressed-size ceiling per zip entry. EPUB bytes are untrusted; a
/// crafted entry can deflate to gigabytes. Real chapter XHTML tops out well
/// under a megabyte.
pub(crate) const MAX_ENTRY_BYTES: u64 = 16 * 1024 * 1024;

pub(crate) fn read_to_string_from_zip<R: std::io::Read + std::io::Seek>(
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

/// Strip an XML qualified-name prefix (`opf:item` → `item`). Sigil/Calibre
/// EPUBs redeclare `xmlns:opf` on `<manifest>` and use `<opf:item>` for every
/// manifest entry; quick_xml returns the fully-qualified bytes, so callers
/// must normalise before comparing.
pub(crate) fn local_name(qname: &[u8]) -> &[u8] {
    match qname.iter().rposition(|&b| b == b':') {
        Some(i) => &qname[i + 1..],
        None => qname,
    }
}

pub(crate) fn read_bytes_from_zip<R: std::io::Read + std::io::Seek>(
    zip: &mut zip::ZipArchive<R>,
    name: &str,
) -> Result<Vec<u8>, EpubError> {
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
    Ok(bytes)
}

pub(crate) fn read_container_opf_path<R: std::io::Read + std::io::Seek>(
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

pub(crate) fn parent_dir(p: &str) -> &str {
    match p.rfind('/') {
        Some(i) => &p[..i],
        None => "",
    }
}

/// Case-insensitive substring search where the needle is already lower-case
/// ASCII. Returns the byte offset of the first match in `haystack`.
pub(crate) fn find_case_insensitive(haystack: &str, needle_lower_ascii: &str) -> Option<usize> {
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
    match std::str::from_utf8(body) {
        Ok(s) => Ok(s.to_string()),
        Err(_) => {
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

fn is_default_ignorable(c: char) -> bool {
    if c.is_control() {
        return true;
    }
    matches!(
        c as u32,
        0x00AD
            | 0x034F
            | 0x061C
            | 0x115F
            | 0x1160
            | 0x17B4
            | 0x17B5
            | 0x180B..=0x180E
            | 0x200B..=0x200F
            | 0x202A..=0x202E
            | 0x2060..=0x206F
            | 0x3164
            | 0xFE00..=0xFE0F
            | 0xFEFF
            | 0xFFA0
            | 0xFFF0..=0xFFF8
            | 0xE0000..=0xE0FFF
    )
}

fn default_chapter_id() -> ChapterId {
    ChapterId(String::new())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
pub struct Chapter {
    pub order: usize,
    pub title: String,
    pub body: String,
    #[serde(default = "default_chapter_id")]
    pub id: ChapterId,
    #[serde(default)]
    pub kind: ChapterKind,
    /// Spine href of the XHTML document that produced this chapter.
    /// Empty string when the parser does not track spine positions
    /// (loose-file and manifest ingest paths).
    #[serde(default)]
    pub spine_href: String,
}

#[derive(Debug, Error, Serialize, Deserialize, Type)]
#[serde(tag = "kind", content = "message")]
pub enum EpubError {
    #[error("io: {0}")]
    Io(String),
    #[error("zip: {0}")]
    Zip(String),
    #[error("parse: {0}")]
    Parse(String),
    #[error("unsupported strategy: {0}")]
    UnsupportedStrategy(String),
}

impl From<std::io::Error> for EpubError {
    fn from(e: std::io::Error) -> Self {
        EpubError::Io(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_title_drops_default_ignorables() {
        // ZWJ between the words must not affect the hash input.
        let plain = "co\u{200D}operate";
        let stripped = "cooperate";
        assert_eq!(normalize_title(plain), normalize_title(stripped));
    }

    #[test]
    fn normalize_title_drops_bom_zwnj_soft_hyphen_variation_selector() {
        let dirty = "\u{FEFF}re\u{200C}sume\u{00AD}\u{FE0F}";
        let clean = "resume";
        assert_eq!(normalize_title(dirty), normalize_title(clean));
    }

    #[test]
    fn chapter_id_hash_stable_across_spine_drift() {
        // Same spine href + title → same id regardless of position in the
        // surviving chapter list.
        let a = ChapterId::from_chapter_parts("kobo", "ch3.xhtml", "The End");
        let b = ChapterId::from_chapter_parts("kobo", "ch3.xhtml", "The End");
        assert_eq!(a, b);
        let c = ChapterId::from_chapter_parts("kobo", "ch2.xhtml", "The End");
        assert_ne!(a, c);
    }

    #[test]
    fn chapter_id_legacy_idx_string_deserialises() {
        // Persisted `skipped_chapters: ["idx:3"]` must still round-trip even
        // after the parser stopped minting `idx:N` ids — older saves drift
        // silently rather than crash-loading.
        let s = r#""idx:3""#;
        let id: ChapterId = serde_json::from_str(s).unwrap();
        assert_eq!(id, ChapterId::from_order(3));
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
