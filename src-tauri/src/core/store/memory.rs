use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

use super::{ProjectStore, StoreError};
use crate::core::identity::ProjectId;
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
}
