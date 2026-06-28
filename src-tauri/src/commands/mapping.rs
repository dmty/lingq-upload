use std::path::PathBuf;
use std::sync::Arc;

use sha2::{Digest, Sha256};
use tauri::Manager;

use crate::core::identity::ProjectId;
use crate::core::matcher::{MappingOp, MappingState};
use crate::core::store::{ProjectStore, StoreError};
use crate::error::AppError;

/// Apply a single mapping-editor op to a project and persist the new state.
///
/// The store performs the load → gate → apply → put cycle under its
/// per-project write lock so concurrent callers cannot interleave their RMW
/// windows. Stale-op and `MappingError` discriminants survive intact onto the
/// IPC boundary; UI may match on `MappingError::UnknownChapter` /
/// `MappingError::UnknownTrack` / stale-op without parsing prose.
#[tauri::command]
#[specta::specta]
pub async fn cmd_apply_mapping_op(
    store: tauri::State<'_, Arc<dyn ProjectStore>>,
    project_id: ProjectId,
    op: MappingOp,
    expected_op_id: u64,
) -> Result<MappingState, AppError> {
    match store.apply_mapping_op(&project_id, op, expected_op_id) {
        Ok(next) => Ok(next),
        Err(StoreError::Mapping(e)) => Err(AppError::Mapping(e)),
        Err(StoreError::MappingStaleOp { server, expected }) => {
            Err(AppError::MappingStaleOp { server, expected })
        }
        Err(StoreError::NotFound { key }) => {
            Err(AppError::Other(format!("project not found: {key}")))
        }
        Err(other) => Err(AppError::Other(format!("store: {other}"))),
    }
}

/// Transcode a windowed slice of an audio file to a deterministic temp MP3
/// and return its absolute path. Caches by (audio_path, start_sec, end_sec).
///
/// Why: WebKit on macOS refuses `<audio>` playback over the `asset://`
/// protocol when the source has no canonical audio MIME (notably `.m4b`,
/// which `mime_guess` does not map). Serving a transcoded `.mp3` from the
/// app cache dir sidesteps the MIME / scheme constraint.
#[tauri::command]
#[specta::specta]
pub async fn cmd_prepare_audio_preview(
    app: tauri::AppHandle,
    audio_path: String,
    start_sec: f64,
    end_sec: f64,
) -> Result<String, AppError> {
    let mut h = Sha256::new();
    h.update(audio_path.as_bytes());
    h.update(start_sec.to_le_bytes());
    h.update(end_sec.to_le_bytes());
    let stem = format!("{:x}", h.finalize());
    let cache_root = app
        .path()
        .app_cache_dir()
        .map_err(|e| AppError::Other(format!("cache dir: {e}")))?
        .join("audio_preview");
    std::fs::create_dir_all(&cache_root)
        .map_err(|e| AppError::Other(format!("create cache dir: {e}")))?;
    let dst = cache_root.join(format!("{}.mp3", &stem[..16]));
    if !dst.exists() {
        let src = PathBuf::from(&audio_path);
        let enc = crate::core::audio::EncoderSettings::default();
        crate::core::audio::transcode(&src, &dst, &enc, Some((start_sec, end_sec)))
            .await
            .map_err(|e| AppError::Other(format!("transcode: {e}")))?;
    }
    Ok(dst.to_string_lossy().into_owned())
}
