//! `plan_preview` must return the same step set the job would upload,
//! before any upload has happened.

mod support;

use lingq_upload_lib::commands::project::project_chapters_impl;
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

#[tokio::test]
async fn plan_preview_leftover_index_does_not_collide_with_a_skipped_chapter() {
    let text_dir = TempDir::new().unwrap();
    let audio_dir = TempDir::new().unwrap();
    let store = InMemoryProjectStore::default();
    let mut project = make_project(text_dir.path(), audio_dir.path());
    let project_id = project.id.clone();

    // Chapter 0 is skipped and not yet uploaded, so `plan_from_mapping` only
    // walks chapters 1 and 2 (eligible len 2). `a_01` is left unclaimed
    // (chapter 0's pair carries no track) so it ships as a leftover
    // audio-only step. Before `leftover_base` was threaded through,
    // `plan_from_mapping` based the leftover index on the eligible count
    // (2) instead of the full chapter count (3), so this leftover's index
    // collided with chapter 2's own real order.
    let track_id = |name: &str| audio_dir.path().join(format!("{name}.m4a")).display().to_string();
    project.mapping = Some(MappingState {
        pairs: vec![
            MappingPair {
                chapter_id: ChapterId::from_order(0),
                track_id: None,
                confidence: 1.0,
                touched: false,
                original_confidence: 1.0,
            },
            MappingPair {
                chapter_id: ChapterId::from_order(1),
                track_id: Some(track_id("a_02")),
                confidence: 1.0,
                touched: false,
                original_confidence: 1.0,
            },
            MappingPair {
                chapter_id: ChapterId::from_order(2),
                track_id: Some(track_id("a_03")),
                confidence: 1.0,
                touched: false,
                original_confidence: 1.0,
            },
        ],
        parking_lot: vec![],
        op_id: 0,
        buckets: vec![],
    });
    project.skipped_chapters = vec![ChapterId::from_order(0)];
    store.put(&project).unwrap();

    let steps = plan_preview(&store, &project_id).await.unwrap();

    let indices: Vec<usize> = steps.iter().map(|s| s.chapter_index).collect();
    let unique: std::collections::HashSet<usize> = indices.iter().copied().collect();
    assert_eq!(
        unique.len(),
        indices.len(),
        "every chapter_index in a seeded plan must be distinct: {indices:?}"
    );

    let leftover = steps
        .iter()
        .find(|s| s.degraded)
        .expect("the unclaimed a_01 track must produce one leftover step");
    assert_eq!(
        leftover.chapter_index, 3,
        "leftover index must start past the full chapter count (3), not the eligible count (2)"
    );
}

/// `guide-xhtml-img.epub` has `cover.xhtml` then `chapter1.xhtml` in the
/// spine. With the cover suppressed, the surviving chapter keeps `order` 1
/// while the chapter list is one entry long — chapter orders are no longer
/// contiguous, so a leftover index derived from the chapter *count* lands on
/// a real chapter's order.
#[tokio::test]
async fn plan_preview_leftover_index_survives_a_cover_filtered_chapter_set() {
    let audio_dir = TempDir::new().unwrap();
    for name in ["a_01", "a_02"] {
        write_silence_m4a_like(&audio_dir.path().join(format!("{name}.m4a")), 2);
    }
    let store = InMemoryProjectStore::default();
    let mut project = Project::new_test(
        ProjectId::from_title_author("PlanPreviewCover", "Author"),
        "PlanPreviewCover",
    );
    project.sources.text = TextSource::Epub(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/epub-covers/guide-xhtml-img.epub"),
    );
    project.sources.audio = Some(AudioSource::Folder(audio_dir.path().to_path_buf()));
    project.settings.language = "en".into();
    project.cover_source_href = Some("cover.xhtml".into());
    let project_id = project.id.clone();
    store.put(&project).unwrap();

    let chapters = project_chapters_impl(&store, &project_id).unwrap();
    assert_eq!(chapters.len(), 1, "cover.xhtml must be filtered out");
    assert_eq!(
        chapters[0].order, 1,
        "the cover filter must not reindex the surviving chapter"
    );

    // Pair the surviving chapter with the SECOND track, leaving `a_01`
    // (track index 0) unclaimed: its leftover index is `base + 0`, which is
    // exactly the value a count-derived base collides on.
    let track_id = |name: &str| audio_dir.path().join(format!("{name}.m4a")).display().to_string();
    project.mapping = Some(MappingState {
        pairs: vec![MappingPair {
            chapter_id: chapters[0].id.clone(),
            track_id: Some(track_id("a_02")),
            confidence: 1.0,
            touched: false,
            original_confidence: 1.0,
        }],
        parking_lot: vec![],
        op_id: 0,
        buckets: vec![],
    });
    store.put(&project).unwrap();

    let steps = plan_preview(&store, &project_id).await.unwrap();

    let leftover = steps
        .iter()
        .find(|s| s.degraded)
        .expect("the unclaimed a_01 track must produce one leftover step");
    assert_ne!(
        leftover.chapter_index, chapters[0].order,
        "leftover index must not reuse a real chapter's order"
    );
    let indices: Vec<usize> = steps.iter().map(|s| s.chapter_index).collect();
    let unique: std::collections::HashSet<usize> = indices.iter().copied().collect();
    assert_eq!(
        unique.len(),
        indices.len(),
        "every chapter_index in a seeded plan must be distinct: {indices:?}"
    );
}
