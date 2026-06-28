use std::sync::Arc;

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
