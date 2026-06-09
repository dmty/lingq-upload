use std::path::Path;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::index::{write_atomic, LibraryError, LibraryIndex, INDEX_FILENAME, INDEX_SCHEMA_V1};
use crate::core::identity::ProjectId;
use crate::core::project::{Project, ProjectSettings, ProjectSources, SCHEMA_V1};
use crate::core::store::ProjectStore;
use crate::ingest::{Candidate, IngestRegistry};

const FUZZY_CONFLICT_THRESHOLD: f32 = 0.85;

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct ReconcileReport {
    pub created: Vec<ProjectId>,
    pub merged: Vec<ProjectId>,
    pub conflicts: Vec<(ProjectId, ProjectId)>,
}

/// Reconcile candidates from all sources against the store.
///
/// Strong-key match (asin / isbn13 / calibre_uuid) wins. `content_hash`
/// is the fallback. Suspiciously similar titles without a strong/hash
/// match surface as `conflicts` so the caller can prompt the user.
///
/// Newly grouped candidates that don't exist in the store are persisted
/// as minimal Projects. Existing matches are left alone and reported
/// under `merged`. After reconcile, writes the library index to
/// `app_data_root`.
pub async fn reconcile(
    sources: &IngestRegistry,
    store: &dyn ProjectStore,
    app_data_root: &Path,
) -> Result<ReconcileReport, LibraryError> {
    let mut all_candidates: Vec<Candidate> = Vec::new();
    for src in sources.iter() {
        if let Ok(mut cs) = src.scan(app_data_root).await {
            all_candidates.append(&mut cs);
        }
    }

    let mut report = ReconcileReport {
        created: Vec::new(),
        merged: Vec::new(),
        conflicts: Vec::new(),
    };

    // Group candidates whose IDs match (strong-key OR content_hash).
    // Each group represents one logical book stitched across sources.
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (i, c) in all_candidates.iter().enumerate() {
        let id = candidate_to_id(c);
        let mut placed = false;
        for grp in groups.iter_mut() {
            let head = candidate_to_id(&all_candidates[grp[0]]);
            if head.matches(&id) {
                grp.push(i);
                placed = true;
                break;
            }
        }
        if !placed {
            groups.push(vec![i]);
        }
    }

    let existing_summaries = store.list()?;

    for group in &groups {
        let leader_cand = &all_candidates[group[0]];
        let leader_id = candidate_to_id(leader_cand);

        if store.get(&leader_id)?.is_some() {
            report.merged.push(leader_id);
            continue;
        }

        // No strong/hash match in store. Look for a fuzzy title clash to
        // flag before silently creating a duplicate.
        if let Some(near) = nearest_existing(&leader_cand.title, &existing_summaries) {
            report.conflicts.push((leader_id.clone(), near));
            continue;
        }

        let project = candidate_to_project(leader_cand);
        store.put(&project).map_err(LibraryError::from)?;
        report.created.push(leader_id);
    }

    report.merged.sort_by_key(|id| id.join_key());
    report.created.sort_by_key(|id| id.join_key());
    report.conflicts.sort_by_key(|a| a.0.join_key());

    let idx = LibraryIndex {
        schema_version: INDEX_SCHEMA_V1,
        generated_at: chrono::Utc::now(),
        entries: store
            .list()?
            .into_iter()
            .map(|s| super::index::summary_to_entry(s, super::index::LibraryStatus::Idle, None))
            .collect(),
    };
    write_atomic(&idx, &app_data_root.join(INDEX_FILENAME))?;
    Ok(report)
}

pub fn candidate_to_project(c: &Candidate) -> Project {
    Project {
        schema_version: SCHEMA_V1,
        id: candidate_to_id(c),
        sources: ProjectSources {
            text: c.text_source.clone(),
            audio: c.audio_source.clone(),
            chapter_manifest: c.chapter_manifest.clone(),
        },
        settings: ProjectSettings {
            language: c.language.clone().unwrap_or_default(),
            collection_title: c.title.clone(),
            level: 1,
            tags: vec![],
        },
        receipts: vec![],
        queue_cursor: 0,
        completed_lesson_ids: vec![],
        matcher_decision: None,
        cover_path: c.cover_path.clone(),
        authors: c.authors.clone(),
        series: c.series.clone(),
        lingq_collection_id: None,
        last_activity_at: None,
        stage: Default::default(),
        last_transition_at: None,
    skipped_chapters: vec![],
    }
}

fn nearest_existing(
    title: &str,
    existing: &[crate::core::project::ProjectSummary],
) -> Option<ProjectId> {
    let mut best: Option<(&crate::core::project::ProjectSummary, f32)> = None;
    for s in existing {
        let r = title_similarity(title, &s.title);
        if r >= FUZZY_CONFLICT_THRESHOLD && best.is_none_or(|(_, br)| r > br) {
            best = Some((s, r));
        }
    }
    best.map(|(s, _)| s.id.clone())
}

/// Similarity in `[0.0, 1.0]` based on edit distance over the longer
/// string. Cheap and good-enough for surfacing duplicate-looking titles.
fn title_similarity(a: &str, b: &str) -> f32 {
    let a = a.to_lowercase();
    let b = b.to_lowercase();
    if a == b {
        return 1.0;
    }
    let len = a.chars().count().max(b.chars().count()).max(1) as f32;
    let dist = levenshtein(&a, &b) as f32;
    1.0 - (dist / len)
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        curr[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
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
