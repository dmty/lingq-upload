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
/// Accept only when `expected_op_id == state.op_id + 1`; reject otherwise so
/// reloads that replay an already-applied op see a clean conflict signal.
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
    if !check_op_id(current.op_id, expected_op_id) {
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

#[inline]
fn check_op_id(current: u64, expected: u64) -> bool {
    expected == current + 1
}

#[cfg(test)]
mod tests {
    use super::check_op_id;

    #[test]
    fn accepts_next_op_id() {
        assert!(check_op_id(5, 6));
        assert!(check_op_id(0, 1));
    }

    #[test]
    fn rejects_stale_or_future_op_id() {
        // Replay of an already-applied op (state advanced from 5 to 6, client
        // retries with 5).
        assert!(!check_op_id(5, 5));
        // Same-id retry after the server has moved on.
        assert!(!check_op_id(6, 5));
        // Skipped op_id (gap).
        assert!(!check_op_id(5, 7));
    }
}
