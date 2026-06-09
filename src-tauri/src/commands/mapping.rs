use std::sync::Arc;

use crate::core::identity::ProjectId;
use crate::core::matcher::{apply_mapping_op, MappingError, MappingOp, MappingState};
use crate::core::store::ProjectStore;
use crate::error::AppError;

impl From<MappingError> for AppError {
    fn from(e: MappingError) -> Self {
        AppError::Other(e.to_string())
    }
}

/// Apply a single mapping-editor op to a project and persist the new state.
///
/// `expected_op_id` is the op_id the client believes the server currently
/// holds — i.e. one less than the op_id the new state will carry. If the
/// server's persisted op_id differs we reject. This lets the UI replay an
/// in-flight op on page reload without double-applying it: the client tracks
/// the last-acknowledged op_id locally, and on reload re-sends with the same
/// `expected_op_id` it sent originally. A retry against an already-applied
/// op finds `current_op_id == expected_op_id`, so it is rejected cleanly
/// rather than mutating the state a second time.
#[tauri::command]
#[specta::specta]
pub async fn cmd_apply_mapping_op(
    store: tauri::State<'_, Arc<dyn ProjectStore>>,
    project_id: ProjectId,
    op: MappingOp,
    expected_op_id: u64,
) -> Result<MappingState, AppError> {
    let mut project = store
        .get(&project_id)
        .map_err(|e| AppError::Other(format!("store.get: {e}")))?
        .ok_or_else(|| AppError::Other("project not found".into()))?;

    let current = project.mapping.clone().unwrap_or_default();
    if current.op_id != expected_op_id {
        return Err(AppError::Other(format!(
            "mapping op_id stale: server={} expected={}",
            current.op_id, expected_op_id
        )));
    }
    let next = apply_mapping_op(&current, op)?;
    project.mapping = Some(next.clone());
    store
        .put(&project)
        .map_err(|e| AppError::Other(format!("store.put: {e}")))?;
    Ok(next)
}
