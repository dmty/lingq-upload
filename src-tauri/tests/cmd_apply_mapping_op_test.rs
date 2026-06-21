//! Real-IPC cover for `cmd_apply_mapping_op`: load → gate → apply → persist
//! against a `JsonProjectStore`, plus a serial-ordering proof under
//! concurrent calls (per-project lock holds the RMW window).

use std::sync::Arc;
use std::thread;

use lingq_upload_lib::core::epub::ChapterId;
use lingq_upload_lib::core::identity::ProjectId;
use lingq_upload_lib::core::matcher::{MappingOp, MappingPair, MappingState};
use lingq_upload_lib::core::project::{Project, ProjectSettings, ProjectSources, SCHEMA_V1};
use lingq_upload_lib::core::store::{JsonProjectStore, ProjectStore, StoreError};
use lingq_upload_lib::ingest::TextSource;
use tempfile::TempDir;

fn cid(s: &str) -> ChapterId {
    ChapterId(s.to_string())
}

fn make_project_with_mapping(state: MappingState) -> Project {
    Project {
        schema_version: SCHEMA_V1,
        id: ProjectId::from_title_author("Mapping Fixture", "Author"),
        sources: ProjectSources {
            text: TextSource::LooseFiles { paths: vec![] },
            audio: None,
            chapter_manifest: None,
        },
        settings: ProjectSettings {
            language: "ja".into(),
            collection_title: "Mapping Fixture".into(),
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
        stage: Default::default(),
        last_transition_at: None,
        skipped_chapters: vec![],
        absorb_policy: Default::default(),
        mapping: Some(state),
    }
}

fn seeded_state() -> MappingState {
    MappingState {
        pairs: vec![
            MappingPair {
                chapter_id: cid("ch1"),
                track_id: Some("t1".into()),
                confidence: 0.4,
                touched: false,
                original_confidence: 0.4,
            },
            MappingPair {
                chapter_id: cid("ch2"),
                track_id: Some("t2".into()),
                confidence: 0.9,
                touched: false,
                original_confidence: 0.9,
            },
        ],
        parking_lot: vec![],
        op_id: 0,
        ..Default::default()
    }
}

#[test]
fn apply_swap_persists_mutation() {
    let dir = TempDir::new().unwrap();
    let store: Arc<dyn ProjectStore> = Arc::new(JsonProjectStore::new(dir.path()));
    let project = make_project_with_mapping(seeded_state());
    let id = project.id.clone();
    store.put(&project).unwrap();

    // Swap t2 into ch1; expect t1 displaced to parking lot.
    let next = store
        .apply_mapping_op(
            &id,
            MappingOp::Swap {
                chapter_id: cid("ch1"),
                track_id: "t2".into(),
            },
            1,
        )
        .expect("swap accepted");

    assert_eq!(next.op_id, 1);
    assert_eq!(next.pairs[0].track_id.as_deref(), Some("t2"));
    assert!(next.pairs[0].touched);
    assert!(next.parking_lot.contains(&"t1".to_string()));

    // Reload from disk through a fresh handle: the mutation survived.
    drop(store);
    let reopened: Arc<dyn ProjectStore> = Arc::new(JsonProjectStore::new(dir.path()));
    let loaded = reopened.get(&id).unwrap().unwrap();
    let persisted = loaded.mapping.expect("mapping persisted");
    assert_eq!(persisted.op_id, 1);
    assert_eq!(persisted.pairs[0].track_id.as_deref(), Some("t2"));
}

#[test]
fn stale_expected_op_id_rejected_with_typed_error() {
    let dir = TempDir::new().unwrap();
    let store: Arc<dyn ProjectStore> = Arc::new(JsonProjectStore::new(dir.path()));
    let project = make_project_with_mapping(seeded_state());
    let id = project.id.clone();
    store.put(&project).unwrap();

    // op_id is 0; legal next is 1. 0 is a replay of an already-applied op.
    let err = store
        .apply_mapping_op(
            &id,
            MappingOp::Park {
                track_id: "t1".into(),
            },
            0,
        )
        .unwrap_err();
    match err {
        StoreError::MappingStaleOp { server, expected } => {
            assert_eq!(server, 0);
            assert_eq!(expected, 0);
        }
        other => panic!("expected MappingStaleOp, got {other:?}"),
    }
}

#[test]
fn concurrent_calls_serialise_one_wins_one_stale() {
    // Two threads race apply_mapping_op against the same project with the
    // same expected_op_id. The per-project lock must serialise the RMW so
    // exactly one transition succeeds; the loser's expectation is stale.
    let dir = TempDir::new().unwrap();
    let store: Arc<dyn ProjectStore> = Arc::new(JsonProjectStore::new(dir.path()));
    let project = make_project_with_mapping(seeded_state());
    let id = project.id.clone();
    store.put(&project).unwrap();

    let s1 = Arc::clone(&store);
    let s2 = Arc::clone(&store);
    let id1 = id.clone();
    let id2 = id.clone();

    let h1 = thread::spawn(move || {
        s1.apply_mapping_op(
            &id1,
            MappingOp::Park {
                track_id: "t1".into(),
            },
            1,
        )
    });
    let h2 = thread::spawn(move || {
        s2.apply_mapping_op(
            &id2,
            MappingOp::Park {
                track_id: "t2".into(),
            },
            1,
        )
    });

    let r1 = h1.join().unwrap();
    let r2 = h2.join().unwrap();

    let (winners, losers): (Vec<_>, Vec<_>) = [r1, r2].into_iter().partition(|r| r.is_ok());
    assert_eq!(winners.len(), 1, "expected exactly one winner");
    assert_eq!(losers.len(), 1, "expected exactly one stale-op rejection");

    match losers.into_iter().next().unwrap().unwrap_err() {
        StoreError::MappingStaleOp { server, expected } => {
            assert_eq!(server, 1, "server advanced past 0 before loser arrived");
            assert_eq!(expected, 1);
        }
        other => panic!("loser saw unexpected error {other:?}"),
    }

    // Final op_id is 1 (one successful step).
    let final_state = store.get(&id).unwrap().unwrap().mapping.unwrap();
    assert_eq!(final_state.op_id, 1);
}
