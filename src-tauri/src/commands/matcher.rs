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
    recompute_split_impl(store.inner().as_ref(), &project_id, excluded_chapter_id).await
}

/// Tauri-free body for `cmd_recompute_split`. Tested directly without
/// spinning up a `tauri::State` harness.
pub async fn recompute_split_impl(
    store: &dyn ProjectStore,
    project_id: &ProjectId,
    excluded_chapter_id: Option<ChapterId>,
) -> Result<MappingState, AppError> {
    let project = store
        .get(project_id)
        .map_err(|e| AppError::Other(format!("store.get: {e}")))?
        .ok_or_else(|| AppError::Other("project not found".into()))?;
    let seeded = seed_split_excluding(&project, excluded_chapter_id.as_ref()).await?;
    store
        .update(project_id, &mut |p| {
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

    fn ffprobe_available() -> bool {
        std::process::Command::new("which")
            .arg("ffprobe")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .is_some()
            && std::process::Command::new("which")
                .arg("ffmpeg")
                .output()
                .ok()
                .filter(|o| o.status.success())
                .is_some()
    }

    fn make_silent_m4a(dir: &std::path::Path, name: &str, seconds: u32) -> PathBuf {
        let p = dir.join(name);
        let status = std::process::Command::new("ffmpeg")
            .args([
                "-y", "-hide_banner", "-v", "error",
                "-f", "lavfi", "-i", "anullsrc=r=22050:cl=stereo",
                "-t", &seconds.to_string(),
                "-c:a", "aac", "-b:a", "32k",
            ])
            .arg(&p)
            .status()
            .expect("spawn ffmpeg");
        assert!(status.success(), "ffmpeg failed for {}", p.display());
        p
    }

    #[tokio::test]
    async fn recompute_split_persists_skip_and_mapping() {
        use crate::ingest::TextSource;

        if !ffprobe_available() {
            eprintln!("ffmpeg/ffprobe missing — skipping");
            return;
        }
        let dir = tempdir().unwrap();
        let _a = make_silent_m4a(dir.path(), "a.m4a", 30);
        let _b = make_silent_m4a(dir.path(), "b.m4a", 60);

        let t0 = dir.path().join("00_ch0.txt");
        let t1 = dir.path().join("01_ch1.txt");
        let t2 = dir.path().join("02_ch2.txt");
        std::fs::write(&t0, "a".repeat(100)).unwrap();
        std::fs::write(&t1, "a".repeat(100)).unwrap();
        std::fs::write(&t2, "a".repeat(200)).unwrap();

        let store = InMemoryProjectStore::new();
        let id = ProjectId::from_title_author("Split Book", "Author");
        let mut p = Project::new_test(id.clone(), "Split Book");
        p.sources.audio = Some(AudioSource::Folder(dir.path().to_path_buf()));
        p.sources.text = TextSource::LooseFiles { paths: vec![t0, t1, t2] };
        store.put(&p).unwrap();

        let excluded = ChapterId::from_order(1);
        let result = super::recompute_split_impl(&store, &id, Some(excluded.clone()))
            .await
            .unwrap();

        assert!(!result.partition_locked);
        assert!(result.pairs.iter().all(|pair| pair.chapter_id != excluded));

        let persisted = store.get(&id).unwrap().unwrap();
        assert!(persisted.skipped_chapters.contains(&excluded));
        let m = persisted.mapping.unwrap();
        assert!(!m.partition_locked);
        assert!(m.pairs.iter().all(|pair| pair.chapter_id != excluded));
    }
}
