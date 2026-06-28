use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

use super::{canonicalise_selection, ProjectStore, StoreError};
use crate::core::epub::ChapterId;
use crate::core::identity::ProjectId;
use crate::core::matcher::{apply_mapping_op as apply_pure, MappingOp, MappingState};
use crate::core::project::{ChapterReceipt, Project, ProjectSummary};

pub struct InMemoryProjectStore {
    inner: Mutex<BTreeMap<String, Project>>,
}

impl InMemoryProjectStore {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(BTreeMap::new()),
        }
    }

    /// Single funnel for `.lock().expect(...)`. Single-process store;
    /// a poisoned mutex means real corruption — propagate the panic.
    fn lock(&self) -> MutexGuard<'_, BTreeMap<String, Project>> {
        self.inner.lock().expect("project store mutex poisoned")
    }
}

impl Default for InMemoryProjectStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectStore for InMemoryProjectStore {
    fn put(&self, p: &Project) -> Result<(), StoreError> {
        self.lock().insert(p.id.join_key(), p.clone());
        Ok(())
    }

    fn get(&self, id: &ProjectId) -> Result<Option<Project>, StoreError> {
        Ok(self.lock().get(&id.join_key()).cloned())
    }

    fn update(
        &self,
        id: &ProjectId,
        f: &mut dyn FnMut(&mut Project),
    ) -> Result<Project, StoreError> {
        let key = id.join_key();
        let mut guard = self.lock();
        let project = guard
            .get_mut(&key)
            .ok_or_else(|| StoreError::NotFound { key: key.clone() })?;
        f(project);
        Ok(project.clone())
    }

    fn list(&self) -> Result<Vec<ProjectSummary>, StoreError> {
        let mut out: Vec<ProjectSummary> = self.lock().values().map(ProjectSummary::from).collect();
        out.sort_by(|a: &ProjectSummary, b| a.title.cmp(&b.title));
        Ok(out)
    }

    fn patch_chapter(
        &self,
        id: &ProjectId,
        index: usize,
        receipt: ChapterReceipt,
    ) -> Result<(), StoreError> {
        let key = id.join_key();
        let mut guard = self.lock();
        let project = guard
            .get_mut(&key)
            .ok_or_else(|| StoreError::NotFound { key: key.clone() })?;
        let len = project.receipts.len();
        if index >= len {
            return Err(StoreError::OutOfBounds { index, len });
        }
        project.receipts[index] = receipt;
        Ok(())
    }

    fn set_selection(&self, id: &ProjectId, skipped_ids: &[ChapterId]) -> Result<(), StoreError> {
        let key = id.join_key();
        let mut guard = self.lock();
        let project = guard
            .get_mut(&key)
            .ok_or_else(|| StoreError::NotFound { key: key.clone() })?;
        project.skipped_chapters = canonicalise_selection(skipped_ids);
        Ok(())
    }

    fn apply_mapping_op(
        &self,
        id: &ProjectId,
        op: MappingOp,
        expected_op_id: u64,
    ) -> Result<MappingState, StoreError> {
        let key = id.join_key();
        let mut guard = self.lock();
        let project = guard
            .get_mut(&key)
            .ok_or_else(|| StoreError::NotFound { key: key.clone() })?;
        let current = project.mapping.clone().unwrap_or_default();
        if expected_op_id != current.op_id + 1 {
            return Err(StoreError::MappingStaleOp {
                server: current.op_id,
                expected: expected_op_id,
            });
        }
        let next = apply_pure(&current, op).map_err(StoreError::Mapping)?;
        project.mapping = Some(next.clone());
        Ok(next)
    }
}
