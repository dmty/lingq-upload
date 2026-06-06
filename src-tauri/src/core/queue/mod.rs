use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

use crate::core::identity::ProjectId;
use crate::core::store::{ProjectStore, StoreError};

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct JobId(pub Uuid);

impl JobId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for JobId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct Job {
    pub id: JobId,
    pub project_id: ProjectId,
}

/// In-memory serial queue. Persistent backing for resume lives in
/// `project.json::queue_cursor` (D3); `rebuild_pending` scans the store
/// to populate the in-memory FIFO at startup.
pub struct Queue {
    store: Arc<dyn ProjectStore>,
    inner: Mutex<VecDeque<Job>>,
}

impl Queue {
    pub fn new(store: Arc<dyn ProjectStore>) -> Self {
        Self {
            store,
            inner: Mutex::new(VecDeque::new()),
        }
    }

    pub fn push(&self, project_id: ProjectId) -> JobId {
        let job = Job {
            id: JobId::new(),
            project_id,
        };
        let id = job.id.clone();
        self.inner.lock().unwrap().push_back(job);
        id
    }

    pub fn current(&self) -> Option<Job> {
        self.inner.lock().unwrap().front().cloned()
    }

    pub fn advance(&self) -> Option<JobId> {
        self.inner.lock().unwrap().pop_front().map(|j| j.id)
    }

    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.lock().unwrap().is_empty()
    }

    /// Scan the store for projects with outstanding work and re-enqueue
    /// them in title order. A project counts as pending when:
    /// - any receipt is missing a `lesson_id` (started but not uploaded), OR
    /// - `queue_cursor` lags behind `receipts.len()` (cursor not advanced), OR
    /// - the project has a `matcher_decision` recorded but no receipts and
    ///   no completed lessons yet (decided but never started).
    pub fn rebuild_pending(&self) -> Result<usize, StoreError> {
        let summaries = self.store.list()?;
        let mut added = 0;
        for s in summaries {
            if let Some(p) = self.store.get(&s.id)? {
                let any_unposted = p.receipts.iter().any(|r| r.lesson_id.is_none());
                let cursor_lag = p.queue_cursor < p.receipts.len();
                let decided_but_unstarted = p.receipts.is_empty()
                    && p.completed_lesson_ids.is_empty()
                    && p.matcher_decision.is_some();
                if any_unposted || cursor_lag || decided_but_unstarted {
                    self.push(p.id);
                    added += 1;
                }
            }
        }
        Ok(added)
    }
}
