use std::sync::Arc;

use chrono::Utc;

use crate::core::epub::ChapterId;
use crate::core::identity::ProjectId;
use crate::core::job::{
    inspect_mismatch, seed_mapping_for_response, seed_split_excluding, MismatchInspection,
};
use crate::core::matcher::{allowed, MappingState, MismatchCondition, MismatchResponse};
use crate::core::project::MatcherDecision;
use crate::core::store::ProjectStore;
use crate::error::AppError;
use crate::ingest::{audio_source_paths, AudioSource};

/// Record the user's matcher decision and seed the initial mapping-grid
/// state so the review step has pairs to render.
#[tauri::command]
#[specta::specta]
pub async fn cmd_matcher_resolve(
    store: tauri::State<'_, Arc<dyn ProjectStore>>,
    project_id: ProjectId,
    condition: MismatchCondition,
    response: MismatchResponse,
    chapter_count: usize,
    track_count: usize,
) -> Result<(), AppError> {
    let project = store
        .get(&project_id)
        .map_err(|e| AppError::Other(format!("store.get: {e}")))?
        .ok_or_else(|| AppError::Other("project not found".into()))?;
    let preselect = allowed(condition).1;
    let decision = MatcherDecision {
        condition,
        response,
        chapter_count,
        track_count,
        user_overrode: response != preselect,
        decided_at: Utc::now(),
    };
    // Resolve sources outside the store lock — pure read-only filesystem
    // work; the atomic write below applies decision + mapping together.
    let seeded = seed_mapping_for_response(&project, response).await?;
    store
        .update(&project_id, &mut |p| {
            p.matcher_decision = Some(decision.clone());
            if seeded.is_some() {
                p.mapping = seeded.clone();
            }
        })
        .map_err(|e| AppError::Other(format!("store.update: {e}")))?;
    Ok(())
}

/// Replace the project's audio source before any chapter has been uploaded.
///
/// Rejects when `receipts` is non-empty (the upload pipeline has already
/// written to LingQ — reshaping audio mid-flight is out of scope here) and
/// when the new source resolves to zero usable files. On success, clears
/// `matcher_decision` and `mapping`: both were seeded against the prior
/// track count and would mis-render after the swap.
#[tauri::command]
#[specta::specta]
pub async fn cmd_replace_audio_source(
    store: tauri::State<'_, Arc<dyn ProjectStore>>,
    project_id: ProjectId,
    audio_source: AudioSource,
) -> Result<(), AppError> {
    replace_audio_source_impl(store.inner().as_ref(), &project_id, audio_source)
}

/// Tauri-free body for `cmd_replace_audio_source`. Tested directly without
/// spinning up a `tauri::State` harness.
pub fn replace_audio_source_impl(
    store: &dyn ProjectStore,
    project_id: &ProjectId,
    audio_source: AudioSource,
) -> Result<(), AppError> {
    let project = store
        .get(project_id)
        .map_err(|e| AppError::Other(format!("store.get: {e}")))?
        .ok_or_else(|| AppError::Other("project not found".into()))?;
    if !project.receipts.is_empty() {
        return Err(AppError::Unsupported(
            "cannot replace audio after uploads have begun".into(),
        ));
    }
    let resolved = audio_source_paths(&audio_source)?;
    if resolved.is_empty() {
        return Err(AppError::Unsupported(
            "audio source resolved to zero usable files".into(),
        ));
    }
    store
        .update(project_id, &mut |p| {
            p.sources.audio = Some(audio_source.clone());
            p.matcher_decision = None;
            p.mapping = None;
        })
        .map_err(|e| AppError::Other(format!("store.update: {e}")))?;
    Ok(())
}

/// Re-probe a project parked in `needs_match` and return the resolve payload.
///
/// Used by the Resolve UI when the user enters from the Library (no live job
/// event to consume). Returns `None` when the project pairs cleanly or has
/// already been resolved.
#[tauri::command]
#[specta::specta]
pub async fn cmd_matcher_inspect(
    store: tauri::State<'_, Arc<dyn ProjectStore>>,
    project_id: ProjectId,
) -> Result<Option<MismatchInspection>, AppError> {
    let project = store
        .get(&project_id)
        .map_err(|e| AppError::Other(format!("store.get: {e}")))?
        .ok_or_else(|| AppError::Other("project not found".into()))?;
    inspect_mismatch(&project).await
}

/// Re-run the proportional split over the remaining chapters (excluding
/// `excluded_chapter_id`) and persist the new mapping with
/// `partition_locked = false`. Adds the excluded chapter to the project's
/// skip set so the run loop omits it.
#[tauri::command]
#[specta::specta]
pub async fn cmd_recompute_split(
    store: tauri::State<'_, Arc<dyn ProjectStore>>,
    project_id: ProjectId,
    excluded_chapter_id: Option<ChapterId>,
) -> Result<MappingState, AppError> {
    let project = store
        .get(&project_id)
        .map_err(|e| AppError::Other(format!("store.get: {e}")))?
        .ok_or_else(|| AppError::Other("project not found".into()))?;
    let seeded = seed_split_excluding(&project, excluded_chapter_id.as_ref()).await?;
    store
        .update(&project_id, &mut |p| {
            if let Some(cid) = &excluded_chapter_id {
                if !p.skipped_chapters.contains(cid) {
                    p.skipped_chapters.push(cid.clone());
                }
            }
            p.mapping = Some(seeded.clone());
        })
        .map_err(|e| AppError::Other(format!("store.update: {e}")))?;
    Ok(seeded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

    use crate::core::matcher::MappingState;
    use crate::core::project::{ChapterReceipt, Project};
    use crate::core::store::{InMemoryProjectStore, ProjectStore};

    fn write_m4b(dir: &std::path::Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, b"x").unwrap();
        p
    }

    fn seed_project(store: &dyn ProjectStore, audio: Option<AudioSource>) -> ProjectId {
        let id = ProjectId::from_title_author("Test Book", "Author");
        let mut p = Project::new_test(id.clone(), "Test Book");
        p.sources.audio = audio;
        store.put(&p).unwrap();
        id
    }

    #[test]
    fn replaces_single_with_multiple_files() {
        let dir = tempdir().unwrap();
        let a = write_m4b(dir.path(), "01.m4b");
        let b = write_m4b(dir.path(), "02.m4b");
        let c = write_m4b(dir.path(), "03.m4b");

        let store = InMemoryProjectStore::new();
        let id = seed_project(&store, Some(AudioSource::SingleFile(a.clone())));

        let new_source = AudioSource::MultipleFiles(vec![a, b, c]);
        replace_audio_source_impl(&store, &id, new_source.clone()).unwrap();

        let after = store.get(&id).unwrap().unwrap();
        assert_eq!(after.sources.audio.as_ref(), Some(&new_source));
    }

    #[test]
    fn rejects_when_receipts_non_empty() {
        let dir = tempdir().unwrap();
        let a = write_m4b(dir.path(), "old.m4b");
        let b = write_m4b(dir.path(), "new.m4b");

        let store = InMemoryProjectStore::new();
        let id = ProjectId::from_title_author("Test Book", "Author");
        let mut p = Project::new_test(id.clone(), "Test Book");
        p.sources.audio = Some(AudioSource::SingleFile(a.clone()));
        p.receipts.push(ChapterReceipt {
            chapter_index: 0,
            track_index: Some(0),
            lesson_id: Some(42),
            degraded: false,
            uploaded_at: None,
        });
        store.put(&p).unwrap();

        let err = replace_audio_source_impl(&store, &id, AudioSource::SingleFile(b)).unwrap_err();
        assert!(
            matches!(err, AppError::Unsupported(ref m) if m.contains("uploads have begun")),
            "expected Unsupported with uploads-have-begun message, got {err:?}",
        );

        let after = store.get(&id).unwrap().unwrap();
        assert_eq!(after.sources.audio, Some(AudioSource::SingleFile(a)));
        assert_eq!(after.receipts.len(), 1);
    }

    #[test]
    fn rejects_when_new_source_resolves_to_zero_files() {
        let dir = tempdir().unwrap();
        let a = write_m4b(dir.path(), "01.m4b");

        let store = InMemoryProjectStore::new();
        let id = seed_project(&store, Some(AudioSource::SingleFile(a.clone())));

        let ghost = AudioSource::MultipleFiles(vec![PathBuf::from("/definitely/not/a/file.m4b")]);
        let err = replace_audio_source_impl(&store, &id, ghost).unwrap_err();
        assert!(
            matches!(err, AppError::Unsupported(ref m) if m.contains("zero usable files")),
            "expected Unsupported with zero-files message, got {err:?}",
        );

        let after = store.get(&id).unwrap().unwrap();
        assert_eq!(after.sources.audio, Some(AudioSource::SingleFile(a)));
    }

    #[test]
    fn successful_replace_clears_matcher_decision_and_mapping() {
        let dir = tempdir().unwrap();
        let a = write_m4b(dir.path(), "01.m4b");
        let b = write_m4b(dir.path(), "02.m4b");

        let store = InMemoryProjectStore::new();
        let id = ProjectId::from_title_author("Test Book", "Author");
        let mut p = Project::new_test(id.clone(), "Test Book");
        p.sources.audio = Some(AudioSource::SingleFile(a));
        p.matcher_decision = Some(MatcherDecision {
            condition: MismatchCondition::OneToMany,
            response: MismatchResponse::SingleLesson,
            chapter_count: 5,
            track_count: 1,
            user_overrode: false,
            decided_at: Utc::now(),
        });
        p.mapping = Some(MappingState::default());
        store.put(&p).unwrap();

        replace_audio_source_impl(&store, &id, AudioSource::MultipleFiles(vec![b])).unwrap();

        let after = store.get(&id).unwrap().unwrap();
        assert!(after.matcher_decision.is_none());
        assert!(after.mapping.is_none());
    }
}
