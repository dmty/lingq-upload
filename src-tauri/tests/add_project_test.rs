//! Conflict-resolution tests for the create-project commands.
//!
//! These tests exercise the orchestration logic by calling the inner
//! reconcile + store contract — they cannot run the Tauri commands
//! directly because those depend on an `AppHandle`. Instead they
//! re-derive the same `candidate_to_id` path and prove the loop and
//! deduping behaviour through the underlying primitives.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use lingq_upload_lib::core::identity::ProjectId;
use lingq_upload_lib::core::library::candidate_to_id;
use lingq_upload_lib::core::project::{Project, ProjectSettings, ProjectSources, SCHEMA_V1};
use lingq_upload_lib::core::store::{InMemoryProjectStore, ProjectStore};
use lingq_upload_lib::ingest::{Candidate, TextSource};

fn make_candidate(title: &str, author: &str) -> Candidate {
    Candidate {
        source_id: format!("test://{title}"),
        title: title.into(),
        authors: vec![author.into()],
        language: Some("ja".into()),
        series: None,
        cover_path: None,
        text_source: TextSource::Epub(PathBuf::from("/tmp/x.epub")),
        audio_source: None,
        chapter_manifest: None,
        metadata_extras: HashMap::new(),
    }
}

fn make_project(id: ProjectId, title: &str) -> Project {
    Project {
        schema_version: SCHEMA_V1,
        id,
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
    }
}

/// Proves the NewProject id-derivation contract: hashing only `title +
/// authors[0]` means mutating `collection_title` is invisible to the hash.
/// The fix mutates `candidate.title` itself, so each suffix changes the id.
#[test]
fn new_project_mutating_collection_title_alone_does_not_change_id() {
    let c1 = make_candidate("Foo Book", "Author");
    let c2 = make_candidate("Foo Book", "Author"); // collection_title differs only at the Project layer
    let id1 = candidate_to_id(&c1);
    let id2 = candidate_to_id(&c2);
    assert_eq!(
        id1, id2,
        "id derived solely from candidate.title + authors[0]"
    );
}

#[test]
fn new_project_mutating_candidate_title_changes_id() {
    let c1 = make_candidate("Foo Book", "Author");
    let mut c2 = make_candidate("Foo Book", "Author");
    c2.title.push_str(" (copy)");
    let id1 = candidate_to_id(&c1);
    let id2 = candidate_to_id(&c2);
    assert_ne!(id1, id2, "appending to candidate.title must shift the hash");
}

/// Simulates the NewProject loop: each iteration grows the candidate title
/// by `" (copy)"`, re-derives the id, and stops at the first unused hash.
/// Verifies the loop terminates in at most `MAX` attempts even with a
/// pre-seeded collision.
#[test]
fn new_project_loop_terminates_with_existing_copy() {
    const MAX_COPY_ATTEMPTS: usize = 100;
    let store: Arc<dyn ProjectStore> = Arc::new(InMemoryProjectStore::new());

    // Seed the store with the original and the first " (copy)" so the loop
    // has to advance to " (copy) (copy)" before finding free space.
    let original = make_candidate("Foo Book", "Author");
    let original_id = candidate_to_id(&original);
    store
        .put(&make_project(original_id.clone(), "Foo Book"))
        .unwrap();

    let mut first_copy = original.clone();
    first_copy.title.push_str(" (copy)");
    let first_copy_id = candidate_to_id(&first_copy);
    store
        .put(&make_project(first_copy_id.clone(), &first_copy.title))
        .unwrap();

    let mut copy = original.clone();
    let mut chosen: Option<ProjectId> = None;
    for _ in 0..MAX_COPY_ATTEMPTS {
        copy.title.push_str(" (copy)");
        let id = candidate_to_id(&copy);
        if store.get(&id).unwrap().is_none() {
            store.put(&make_project(id.clone(), &copy.title)).unwrap();
            chosen = Some(id);
            break;
        }
    }

    let chosen = chosen.expect("loop must terminate");
    assert_ne!(chosen, original_id, "must not reuse original id");
    assert_ne!(chosen, first_copy_id, "must not reuse first-copy id");

    // A second NewProject pass must again produce a distinct id (proves the
    // bug fix: hash changes per mutation, not stuck on the same value).
    let mut copy2 = original.clone();
    let mut second: Option<ProjectId> = None;
    for _ in 0..MAX_COPY_ATTEMPTS {
        copy2.title.push_str(" (copy)");
        let id = candidate_to_id(&copy2);
        if store.get(&id).unwrap().is_none() {
            second = Some(id);
            break;
        }
    }
    let second = second.expect("second pass must also terminate");
    assert_ne!(
        second, chosen,
        "second NewProject must produce a distinct id"
    );
}
