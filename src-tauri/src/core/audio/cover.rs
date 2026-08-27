use std::path::{Path, PathBuf};

use super::AudioError;
use crate::codecs::{AudioMetadata, SymphoniaMetadata};

/// File extension for an embedded cover's declared media type. Taggers write
/// this field inconsistently (`image/jpeg`, `jpeg`, `JPG`), so match loosely
/// and default to jpg — what audiobook tooling overwhelmingly embeds.
fn ext_for(media_type: &str) -> &'static str {
    let m = media_type.to_ascii_lowercase();
    if m.contains("png") {
        "png"
    } else if m.contains("webp") {
        "webp"
    } else {
        "jpg"
    }
}

/// Write the audio file's embedded cover into `dest_dir` as `cover.<ext>`,
/// returning the path written. `Ok(None)` when the file carries no art, which
/// includes every `.wav` — the format has no standard place for one.
///
/// Mirrors the EPUB cover sidecar layout so both sources land on the same
/// filename and only one `cover.*` ever exists per project.
pub fn extract_to_dir(audio: &Path, dest_dir: &Path) -> Result<Option<PathBuf>, AudioError> {
    let Some(cover) = SymphoniaMetadata::probe_cover(audio)? else {
        return Ok(None);
    };
    if cover.data.is_empty() {
        return Ok(None);
    }
    std::fs::create_dir_all(dest_dir).map_err(|e| AudioError::Io(e.to_string()))?;
    let out = dest_dir.join(format!("cover.{}", ext_for(&cover.media_type)));
    for prior_ext in ["jpg", "jpeg", "png", "webp"] {
        let candidate = dest_dir.join(format!("cover.{prior_ext}"));
        if candidate != out && candidate.exists() {
            let _ = std::fs::remove_file(&candidate);
        }
    }
    std::fs::write(&out, &cover.data).map_err(|e| AudioError::Io(e.to_string()))?;
    Ok(Some(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ext_follows_media_type_and_defaults_to_jpg() {
        assert_eq!(ext_for("image/png"), "png");
        assert_eq!(ext_for("image/webp"), "webp");
        assert_eq!(ext_for("image/jpeg"), "jpg");
        // Taggers that write a bare or unknown type still get a usable file.
        assert_eq!(ext_for("JPG"), "jpg");
        assert_eq!(ext_for(""), "jpg");
    }

    #[test]
    fn a_file_without_art_yields_no_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let untagged = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/audio/probe_3min.mp3");
        let got = extract_to_dir(&untagged, dir.path()).expect("probe must not fail");
        assert_eq!(got, None, "an untagged file yields no cover");
        assert!(!dir.path().join("cover.jpg").exists());
    }
}
