//! Direct unit tests for `ProjectStage` + `Project::advance` (AD-022).

use lingq_upload_lib::core::audio::AbsorbPolicy;
use std::path::PathBuf;

use chrono::{TimeZone, Utc};
use lingq_upload_lib::core::identity::ProjectId;
use lingq_upload_lib::core::job::next_stage;
use lingq_upload_lib::core::project::{
    Project, ProjectSettings, ProjectSources, ProjectStage, StageError, SCHEMA_V1,
};
use lingq_upload_lib::core::store::{safe_path_segment, JsonProjectStore, ProjectStore};
use lingq_upload_lib::ingest::TextSource;
use tempfile::TempDir;

fn sample(title: &str, stage: ProjectStage) -> Project {
    Project {
        schema_version: SCHEMA_V1,
        id: ProjectId::from_title_author(title, "Author"),
        sources: ProjectSources {
            text: TextSource::Epub(PathBuf::from("/tmp/x.epub")),
            audio: None,
            chapter_manifest: None,
        },
        settings: ProjectSettings {
            language: "ja".into(),
            collection_title: title.into(),
            level: 1,
            tags: vec![],
        },
        receipts: vec![],
        queue_cursor: 0,
        completed_lesson_ids: vec![],
        matcher_decision: None,
        cover_path: None,
        authors: vec![],
        series: None,
        lingq_collection_id: None,
        last_activity_at: None,
        stage,
        last_transition_at: None,
        skipped_chapters: vec![],
        absorb_policy: AbsorbPolicy::default(),
        mapping: None,
    }
}

#[test]
fn advance_forward_succeeds_and_stamps_timestamp() {
    let mut p = sample("Forward", ProjectStage::New);
    assert!(p.last_transition_at.is_none());

    p.advance(ProjectStage::Parsed).unwrap();

    assert_eq!(p.stage(), ProjectStage::Parsed);
    assert!(p.last_transition_at.is_some());
}

#[test]
fn advance_backward_returns_stage_error() {
    let mut p = sample("Backward", ProjectStage::Uploaded);
    let before = p.clone();

    let err = p.advance(ProjectStage::Mapped).unwrap_err();

    assert_eq!(
        err,
        StageError {
            from: ProjectStage::Uploaded,
            to: ProjectStage::Mapped,
        }
    );
    assert_eq!(p.stage(), ProjectStage::Uploaded, "stage untouched");
    assert_eq!(p, before, "no mutation");
}

#[test]
fn advance_same_stage_is_noop_and_does_not_restamp() {
    let stamped = Utc.with_ymd_and_hms(2025, 1, 2, 3, 4, 5).unwrap();
    let mut p = sample("Same", ProjectStage::Parsed);
    p.last_transition_at = Some(stamped);

    p.advance(ProjectStage::Parsed).unwrap();

    assert_eq!(p.stage(), ProjectStage::Parsed);
    assert_eq!(p.last_transition_at, Some(stamped));
}

#[test]
fn project_stage_serialises_lowercase() {
    let p = sample("Snap", ProjectStage::Mapped);
    let v: serde_json::Value = serde_json::to_value(&p).unwrap();
    assert_eq!(v["stage"], "mapped");
}

#[test]
fn project_json_without_stage_defaults_to_new() {
    let raw = serde_json::json!({
        "schema_version": SCHEMA_V1,
        "id": ProjectId::from_title_author("Legacy", "Author"),
        "sources": { "text": { "kind": "epub", "value": "/tmp/x.epub" } },
        "settings": { "language": "ja", "collection_title": "Legacy" }
    });
    let p: Project = serde_json::from_value(raw).unwrap();
    assert_eq!(p.stage(), ProjectStage::New);
    assert!(p.last_transition_at.is_none());
}

#[test]
fn advance_then_put_round_trips_through_json_store() {
    let tmp = TempDir::new().unwrap();
    let store = JsonProjectStore::new(tmp.path());
    let mut p = sample("Persist", ProjectStage::New);
    store.put(&p).unwrap();

    p.advance(ProjectStage::Parsed).unwrap();
    store.put(&p).unwrap();

    let got = store.get(&p.id).unwrap().unwrap();
    assert_eq!(got.stage(), ProjectStage::Parsed);
    assert_eq!(got.last_transition_at, p.last_transition_at);
}

#[test]
fn next_stage_returns_none_for_done() {
    let p = sample("Done", ProjectStage::Done);
    assert_eq!(next_stage(&p), None);
}

#[test]
fn next_stage_returns_successor_for_every_other_stage() {
    let cases = [
        (ProjectStage::New, Some(ProjectStage::Parsed)),
        (ProjectStage::Parsed, Some(ProjectStage::Mapped)),
        (ProjectStage::Mapped, Some(ProjectStage::Transcoded)),
        (ProjectStage::Transcoded, Some(ProjectStage::Uploaded)),
        (ProjectStage::Uploaded, Some(ProjectStage::Done)),
        (ProjectStage::Done, None),
    ];
    for (from, expected) in cases {
        let p = sample("Table", from);
        assert_eq!(next_stage(&p), expected, "from {from:?}");
    }
}

#[test]
fn powercut_between_stage_writes_keeps_prior_stage() {
    let tmp = TempDir::new().unwrap();
    let store = JsonProjectStore::new(tmp.path());
    let p = sample("PowerCut", ProjectStage::Parsed);
    store.put(&p).unwrap();

    let pj = tmp
        .path()
        .join("projects")
        .join(safe_path_segment(&p.id.join_key()))
        .join("project.json");
    let tmp_path = pj.with_extension("json.tmp");
    std::fs::write(&tmp_path, b"{ partial write before rename }").unwrap();

    let got = store.get(&p.id).unwrap().unwrap();
    assert_eq!(got.stage(), ProjectStage::Parsed, "prior stage survives");
}
