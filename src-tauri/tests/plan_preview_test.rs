//! `plan_preview` must return the same step set the job would upload,
//! before any upload has happened.

mod support;

use lingq_upload_lib::core::epub::ChapterId;
use lingq_upload_lib::core::identity::ProjectId;
use lingq_upload_lib::core::job::plan_preview;
use lingq_upload_lib::core::matcher::{MappingPair, MappingState};
use lingq_upload_lib::core::project::Project;
use lingq_upload_lib::core::store::{InMemoryProjectStore, ProjectStore};
use lingq_upload_lib::error::AppError;
use lingq_upload_lib::ingest::{AudioSource, TextSource};
use std::path::{Path, PathBuf};
use support::mk_fixture::write_silence_m4a_like;
use tempfile::TempDir;

/// Three loose text chapters + three audio files in a folder. Equal counts,
/// so `auto_match` pairs them and `build_plan` emits one step per chapter.
fn make_project(text_dir: &Path, audio_dir: &Path) -> Project {
    let mut text_paths: Vec<PathBuf> = Vec::new();
    for (i, name) in ["ch_01", "ch_02", "ch_03"].iter().enumerate() {
        let p = text_dir.join(format!("{name}.txt"));
        std::fs::write(&p, format!("Body of chapter {}.", i + 1)).unwrap();
        text_paths.push(p);
    }
    // `.m4a` extension with WAV content: `has_audio_extension` gates the
    // folder scan on the extension, symphonia reads the real container.
    for name in ["a_01", "a_02", "a_03"] {
        write_silence_m4a_like(&audio_dir.join(format!("{name}.m4a")), 2);
    }
    let mut project = Project::new_test(
        ProjectId::from_title_author("PlanPreview", "Author"),
        "PlanPreview",
    );
    project.sources.text = TextSource::LooseFiles { paths: text_paths };
    project.sources.audio = Some(AudioSource::Folder(audio_dir.to_path_buf()));
    project.settings.language = "en".into();
    project
}

#[tokio::test]
async fn plan_preview_lists_every_step_before_any_upload() {
    let text_dir = TempDir::new().unwrap();
    let audio_dir = TempDir::new().unwrap();
    let store = InMemoryProjectStore::default();
    let project = make_project(text_dir.path(), audio_dir.path());
    let project_id = project.id.clone();
    store.put(&project).unwrap();

    let steps = plan_preview(&store, &project_id).await.unwrap();

    assert_eq!(steps.len(), 3, "one step per paired chapter");
    assert_eq!(
        steps.iter().map(|s| s.chapter_index).collect::<Vec<_>>(),
        vec![0, 1, 2],
    );
    assert_eq!(steps[0].title, "ch_01", "title comes from the plan step");
    assert!(steps.iter().all(|s| !s.degraded));
}

#[tokio::test]
async fn plan_preview_returns_empty_when_project_has_no_audio() {
    let text_dir = TempDir::new().unwrap();
    let audio_dir = TempDir::new().unwrap();
    let store = InMemoryProjectStore::default();
    let mut project = make_project(text_dir.path(), audio_dir.path());
    project.sources.audio = None;
    let project_id = project.id.clone();
    store.put(&project).unwrap();

    let steps = plan_preview(&store, &project_id).await.unwrap();

    assert!(steps.is_empty(), "no audio source means no plan to preview");
}

#[tokio::test]
async fn plan_preview_propagates_a_broken_mapping_as_an_error() {
    let text_dir = TempDir::new().unwrap();
    let audio_dir = TempDir::new().unwrap();
    let store = InMemoryProjectStore::default();
    let mut project = make_project(text_dir.path(), audio_dir.path());
    let project_id = project.id.clone();

    // Same failure `run_project_job` hits when a track the mapping points at
    // has since been moved or renamed: `plan_from_mapping` can't resolve the
    // pair's `track_id` against the current audio folder.
    project.mapping = Some(MappingState {
        pairs: (0..3)
            .map(|i| MappingPair {
                chapter_id: ChapterId::from_order(i),
                track_id: Some("missing-track".to_string()),
                confidence: 1.0,
                touched: false,
                original_confidence: 1.0,
            })
            .collect(),
        parking_lot: vec![],
        op_id: 0,
        buckets: vec![],
    });
    store.put(&project).unwrap();

    let err = plan_preview(&store, &project_id).await.unwrap_err();
    assert!(
        matches!(&err, AppError::Other(msg) if msg.contains("unknown track")),
        "a mapping referencing a moved/renamed track must propagate as an error, not an empty preview: {err:?}",
    );
}
