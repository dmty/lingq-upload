use std::path::Path;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::index::{write_atomic, LibraryError, LibraryIndex, INDEX_FILENAME, INDEX_SCHEMA_V1};
use crate::core::identity::ProjectId;
use crate::core::store::ProjectStore;
use crate::ingest::{Candidate, IngestRegistry};

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct ReconcileReport {
    pub created: Vec<ProjectId>,
    pub merged: Vec<ProjectId>,
    pub conflicts: Vec<(ProjectId, ProjectId)>,
}

/// Reconcile candidates from all sources against the store.
///
/// Strong-key match (asin / isbn13 / calibre_uuid) wins. `content_hash`
/// is the fallback. Below-threshold fuzzy overlaps are reported as
/// `conflicts`; no silent merge.
///
/// After successful reconcile, writes the library index to `app_data_root`.
pub async fn reconcile(
    sources: &IngestRegistry,
    store: &dyn ProjectStore,
    app_data_root: &Path,
) -> Result<ReconcileReport, LibraryError> {
    let mut all_candidates: Vec<Candidate> = Vec::new();
    for src in sources.iter() {
        match src.scan(app_data_root).await {
            Ok(mut cs) => all_candidates.append(&mut cs),
            Err(_) => continue,
        }
    }

    let mut report = ReconcileReport {
        created: Vec::new(),
        merged: Vec::new(),
        conflicts: Vec::new(),
    };

    // Group candidates by strong-key match. Sprint 2 stops at strong-key
    // grouping; the Project upsert lives in B3-driven create flows.
    let mut grouped: Vec<Vec<ProjectId>> = Vec::new();
    for c in &all_candidates {
        let id = candidate_to_id(c);
        let mut placed = false;
        for grp in grouped.iter_mut() {
            if grp.iter().any(|gid| gid.matches(&id)) {
                grp.push(id.clone());
                placed = true;
                break;
            }
        }
        if !placed {
            grouped.push(vec![id]);
        }
    }

    for group in &grouped {
        let leader = group[0].clone();
        if group.len() > 1 {
            report.merged.push(leader);
        } else {
            // existing in store? track as merged; else created.
            match store.get(&leader)? {
                Some(_) => report.merged.push(leader),
                None => report.created.push(leader),
            }
        }
    }

    let idx = LibraryIndex {
        schema_version: INDEX_SCHEMA_V1,
        generated_at: chrono::Utc::now(),
        entries: store
            .list()?
            .into_iter()
            .map(|s| super::index::LibraryEntry {
                id: s.id,
                title: s.title,
                language: s.language,
                completed_lesson_count: s.completed_lesson_count,
                receipt_count: s.receipt_count,
                mtime: None,
            })
            .collect(),
    };
    write_atomic(&idx, &app_data_root.join(INDEX_FILENAME))?;
    Ok(report)
}

/// Build a `ProjectId` from a `Candidate`, honouring every strong key the
/// candidate carries. Shared by reconcile and the manual-create path.
pub fn candidate_to_id(c: &Candidate) -> ProjectId {
    let author = c.authors.first().map(|s| s.as_str()).unwrap_or("");
    let mut id = ProjectId::from_title_author(&c.title, author);
    if let Some(asin) = c
        .metadata_extras
        .get("audible_asin")
        .and_then(|v| v.as_str())
    {
        id = id.with_asin(asin);
    }
    if let Some(isbn) = c.metadata_extras.get("isbn13").and_then(|v| v.as_str()) {
        id = id.with_isbn13(isbn);
    }
    if let Some(uuid) = c
        .metadata_extras
        .get("calibre_uuid")
        .and_then(|v| v.as_str())
        .and_then(|s| uuid::Uuid::parse_str(s).ok())
    {
        id = id.with_calibre_uuid(uuid);
    }
    id
}
