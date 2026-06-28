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

/// Return a path the frontend can hand to `convertFileSrc` for playback.
///
/// Tauri's asset protocol derives Content-Type via `mime_guess`. `.m4b` is
/// not in its table — WebKit on macOS refuses `<audio>` over `asset://`
/// when Content-Type ends up `application/octet-stream`. We sidestep that
/// by exposing the original bytes under an `.m4a` symlink in the app
/// cache dir; `.m4a` maps to `audio/mp4`, which WebKit accepts. The
/// browser plays the full file in place and we seek into the window on
/// the client.
#[tauri::command]
#[specta::specta]
pub async fn cmd_prepare_audio_preview(
    app: tauri::AppHandle,
    audio_path: String,
) -> Result<String, AppError> {
    let src = PathBuf::from(&audio_path);
    if !src.exists() {
        return Err(AppError::Other(format!("missing audio: {audio_path}")));
    }
    let ext = src
        .extension()
        .and_then(|s| s.to_str())
        .map(str::to_ascii_lowercase);
    let needs_alias = !matches!(
        ext.as_deref(),
        Some("mp3" | "m4a" | "wav" | "flac" | "ogg" | "oga" | "opus")
    );
    if !needs_alias {
        return Ok(audio_path);
    }
    let mut h = Sha256::new();
    h.update(audio_path.as_bytes());
    let stem = format!("{:x}", h.finalize());
    let cache_root = app
        .path()
        .app_cache_dir()
        .map_err(|e| AppError::Other(format!("cache dir: {e}")))?
        .join("audio_alias");
    std::fs::create_dir_all(&cache_root)
        .map_err(|e| AppError::Other(format!("create cache dir: {e}")))?;
    let dst = cache_root.join(format!("{}.m4a", &stem[..16]));
    if !dst.exists() {
        #[cfg(unix)]
        std::os::unix::fs::symlink(&src, &dst)
            .map_err(|e| AppError::Other(format!("symlink: {e}")))?;
        #[cfg(windows)]
        std::fs::hard_link(&src, &dst)
            .map_err(|e| AppError::Other(format!("hard_link: {e}")))?;
    }
    Ok(dst.to_string_lossy().into_owned())
}
