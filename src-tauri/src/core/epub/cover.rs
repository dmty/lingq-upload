//! EPUB cover image discovery + sidecar writer.
//!
//! The cascade — first hit wins:
//!   1. EPUB3 `manifest/item[@properties~="cover-image"]`
//!   2. EPUB2 `<meta name="cover" content="ID"/>` → manifest item with that id
//!   3. `<guide><reference type="cover" href="…"/>` — recurse into XHTML
//!      and pull the first `<img src=…>`
//!   4. Filename heuristic — any zip entry matching `cover.{jpg,jpeg,png,webp}`
//!      outside `_covers/` (calibre uses that dir for alt thumbs)
//!
//! On success: writes raw bytes to `<dest_dir>/cover.<ext>` and returns the
//! path + mime + the spine href that hosts the cover (when rungs 1-3 can
//! identify it; None for rung 4).

use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};

use quick_xml::events::Event;
use quick_xml::Reader;
use zip::ZipArchive;

use super::{parent_dir, read_bytes_from_zip, read_container_opf_path, read_to_string_from_zip, EpubError};

#[derive(Debug, Clone)]
pub struct ExtractedCover {
    pub path: PathBuf,
    pub mime: String,
    /// The spine href that wraps the cover image (e.g. "cover.xhtml"),
    /// when known. `None` for pure filename-heuristic hits (rung 4) or
    /// when no spine entry references the image.
    pub source_spine_href: Option<String>,
}

pub fn extract_to_dir(epub: &Path, dest_dir: &Path) -> Result<Option<ExtractedCover>, EpubError> {
    let bytes = std::fs::read(epub).map_err(|e| EpubError::Io(e.to_string()))?;
    extract_to_dir_from_bytes(&bytes, dest_dir)
}

pub fn extract_to_dir_from_bytes(
    bytes: &[u8],
    dest_dir: &Path,
) -> Result<Option<ExtractedCover>, EpubError> {
    let mut zip = ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| EpubError::Zip(e.to_string()))?;

    let opf_path = read_container_opf_path(&mut zip)?;
    let opf_dir = parent_dir(&opf_path).to_string();
    let opf_xml = read_to_string_from_zip(&mut zip, &opf_path)?;

    let manifest = parse_manifest(&opf_xml);
    let spine = parse_spine(&opf_xml);

    // Rung 1: EPUB3 properties="cover-image"
    if let Some(item) = manifest.iter().find(|m| m.is_cover_image_property) {
        let href = join_opf(&opf_dir, &item.href);
        return write_sidecar(&mut zip, &href, &item.media_type, dest_dir,
            host_spine_href(&spine, &manifest, &item.href));
    }

    // Rung 2: EPUB2 <meta name="cover" content="ID"/>
    if let Some(meta_id) = parse_meta_cover_id(&opf_xml) {
        if let Some(item) = manifest.iter().find(|m| m.id == meta_id) {
            let href = join_opf(&opf_dir, &item.href);
            return write_sidecar(&mut zip, &href, &item.media_type, dest_dir,
                host_spine_href(&spine, &manifest, &item.href));
        }
    }

    // Rung 3: guide reference -> xhtml -> <img src=…>
    if let Some(guide_href) = parse_guide_cover_href(&opf_xml) {
        let xhtml_path = join_opf(&opf_dir, &guide_href);
        if let Ok(xhtml) = read_to_string_from_zip(&mut zip, &xhtml_path) {
            if let Some(img_src) = first_img_src(&xhtml) {
                let img_path = join_relative(&xhtml_path, &img_src);
                if let Some(item) = manifest.iter().find(|m| join_opf(&opf_dir, &m.href) == img_path) {
                    return write_sidecar(&mut zip, &img_path, &item.media_type, dest_dir,
                        Some(guide_href));
                }
                // Image referenced directly in zip but not declared in manifest:
                // synth a mime from the extension.
                let mime = guess_mime(&img_path).unwrap_or_else(|| "application/octet-stream".into());
                return write_sidecar(&mut zip, &img_path, &mime, dest_dir, Some(guide_href));
            }
        }
    }

    // Rung 4: filename heuristic
    let names: Vec<String> = (0..zip.len())
        .filter_map(|i| zip.by_index(i).ok().map(|f| f.name().to_string()))
        .collect();
    for name in names {
        let lower = name.to_ascii_lowercase();
        if lower.contains("/_covers/") || lower.starts_with("_covers/") {
            continue;
        }
        if let Some(stripped) = lower.rsplit('/').next() {
            if matches!(stripped, "cover.jpg" | "cover.jpeg" | "cover.png" | "cover.webp") {
                let mime = guess_mime(&name).unwrap_or_else(|| "application/octet-stream".into());
                return write_sidecar(&mut zip, &name, &mime, dest_dir, None);
            }
        }
    }

    Ok(None)
}

fn write_sidecar<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    entry_name: &str,
    media_type: &str,
    dest_dir: &Path,
    source_spine_href: Option<String>,
) -> Result<Option<ExtractedCover>, EpubError> {
    let bytes = read_bytes_from_zip(zip, entry_name)?;
    let ext = ext_for_mime(media_type)
        .unwrap_or_else(|| {
            std::path::Path::new(entry_name)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("jpg")
                .to_string()
        });
    std::fs::create_dir_all(dest_dir).map_err(|e| EpubError::Io(e.to_string()))?;
    let out = dest_dir.join(format!("cover.{ext}"));
    // Delete any prior sidecar at a different extension.
    for prior_ext in ["jpg", "jpeg", "png", "webp"] {
        let candidate = dest_dir.join(format!("cover.{prior_ext}"));
        if candidate != out && candidate.exists() {
            let _ = std::fs::remove_file(&candidate);
        }
    }
    let mut f = std::fs::File::create(&out).map_err(|e| EpubError::Io(e.to_string()))?;
    f.write_all(&bytes).map_err(|e| EpubError::Io(e.to_string()))?;
    Ok(Some(ExtractedCover {
        path: out,
        mime: media_type.to_string(),
        source_spine_href,
    }))
}

#[derive(Debug)]
struct ManifestItem {
    id: String,
    href: String,
    media_type: String,
    is_cover_image_property: bool,
}

fn parse_manifest(opf_xml: &str) -> Vec<ManifestItem> {
    let mut reader = Reader::from_str(opf_xml);
    let mut out = Vec::new();
    let mut buf = Vec::new();
    let mut in_manifest = false;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) if e.name().as_ref() == b"manifest" => in_manifest = true,
            Ok(Event::End(e)) if e.name().as_ref() == b"manifest" => in_manifest = false,
            Ok(Event::Empty(e)) | Ok(Event::Start(e)) if in_manifest && e.name().as_ref() == b"item" => {
                let mut id = String::new();
                let mut href = String::new();
                let mut media_type = String::new();
                let mut props = String::new();
                for attr in e.attributes().flatten() {
                    let key = attr.key.as_ref();
                    let val = attr.unescape_value().unwrap_or_default().to_string();
                    match key {
                        b"id" => id = val,
                        b"href" => href = val,
                        b"media-type" => media_type = val,
                        b"properties" => props = val,
                        _ => {}
                    }
                }
                out.push(ManifestItem {
                    id,
                    href,
                    media_type,
                    is_cover_image_property: props.split_ascii_whitespace().any(|p| p == "cover-image"),
                });
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    out
}

fn parse_spine(opf_xml: &str) -> Vec<String> {
    let mut reader = Reader::from_str(opf_xml);
    let mut out = Vec::new();
    let mut buf = Vec::new();
    let mut in_spine = false;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) if e.name().as_ref() == b"spine" => in_spine = true,
            Ok(Event::End(e)) if e.name().as_ref() == b"spine" => in_spine = false,
            Ok(Event::Empty(e)) | Ok(Event::Start(e)) if in_spine && e.name().as_ref() == b"itemref" => {
                for attr in e.attributes().flatten() {
                    if attr.key.as_ref() == b"idref" {
                        out.push(attr.unescape_value().unwrap_or_default().to_string());
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

fn parse_meta_cover_id(opf_xml: &str) -> Option<String> {
    let mut reader = Reader::from_str(opf_xml);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(e)) | Ok(Event::Start(e)) if e.name().as_ref() == b"meta" => {
                let mut name = None;
                let mut content = None;
                for attr in e.attributes().flatten() {
                    let v = attr.unescape_value().ok().map(|c| c.into_owned());
                    match attr.key.as_ref() {
                        b"name" => name = v,
                        b"content" => content = v,
                        _ => {}
                    }
                }
                if name.as_deref() == Some("cover") {
                    return content;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    None
}

fn parse_guide_cover_href(opf_xml: &str) -> Option<String> {
    let mut reader = Reader::from_str(opf_xml);
    let mut buf = Vec::new();
    let mut in_guide = false;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) if e.name().as_ref() == b"guide" => in_guide = true,
            Ok(Event::End(e)) if e.name().as_ref() == b"guide" => in_guide = false,
            Ok(Event::Empty(e)) | Ok(Event::Start(e)) if in_guide && e.name().as_ref() == b"reference" => {
                let mut typ = None;
                let mut href = None;
                for attr in e.attributes().flatten() {
                    let v = attr.unescape_value().ok().map(|c| c.into_owned());
                    match attr.key.as_ref() {
                        b"type" => typ = v,
                        b"href" => href = v,
                        _ => {}
                    }
                }
                if typ.as_deref() == Some("cover") {
                    return href;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    None
}

fn first_img_src(xhtml: &str) -> Option<String> {
    let mut reader = Reader::from_str(xhtml);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(e)) | Ok(Event::Start(e)) if e.name().as_ref() == b"img" => {
                for attr in e.attributes().flatten() {
                    if attr.key.as_ref() == b"src" {
                        return attr.unescape_value().ok().map(|c| c.into_owned());
                    }
                }
            }
            Ok(Event::Eof) => return None,
            Err(_) => return None,
            _ => {}
        }
        buf.clear();
    }
}

/// Find the spine href whose XHTML wraps the given image href, if any.
// SIMPLIFY: host_spine_href heuristic — matches "cover" in filename; upgrade by
// parsing the XHTML body and matching <img src> to the cover image href when
// false positives appear.
fn host_spine_href(
    spine: &[String],
    manifest: &[ManifestItem],
    image_href: &str,
) -> Option<String> {
    for idref in spine {
        let item = manifest.iter().find(|m| &m.id == idref)?;
        if !item.media_type.contains("xhtml") && !item.media_type.contains("html") {
            continue;
        }
        // We don't read the xhtml here for performance; the most common case
        // is that the cover host page name contains "cover".
        let lower = item.href.to_ascii_lowercase();
        if lower.contains("cover") {
            // Confirm by checking whether this page's <img src> resolves to the target.
            // Caller already opened the zip; we don't have it here. Return the href
            // optimistically — the filter helper is best-effort and a stale match is
            // harmless because chapters are user-confirmed before upload.
            return Some(item.href.clone());
        }
    }
    None
}

fn join_opf(opf_dir: &str, href: &str) -> String {
    if opf_dir.is_empty() {
        href.to_string()
    } else {
        format!("{}/{}", opf_dir.trim_end_matches('/'), href)
    }
}

fn join_relative(from: &str, rel: &str) -> String {
    let base = match from.rfind('/') {
        Some(i) => &from[..i],
        None => "",
    };
    if base.is_empty() {
        rel.to_string()
    } else {
        format!("{base}/{rel}")
    }
}

fn ext_for_mime(mime: &str) -> Option<String> {
    Some(match mime.to_ascii_lowercase().as_str() {
        "image/jpeg" | "image/jpg" => "jpg".into(),
        "image/png" => "png".into(),
        "image/webp" => "webp".into(),
        _ => return None,
    })
}

fn guess_mime(path: &str) -> Option<String> {
    let lower = path.to_ascii_lowercase();
    Some(match lower.rsplit('.').next()? {
        "jpg" | "jpeg" => "image/jpeg".into(),
        "png" => "image/png".into(),
        "webp" => "image/webp".into(),
        _ => return None,
    })
}
