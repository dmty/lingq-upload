//! Mapping-state contract across both stores.
//!
//! Covers the `apply_mapping_op` stale-op gate, the persistence round-trip,
//! and the seeding path the orchestrator uses to materialise the initial
//! `MappingState` (guarded `update` so user edits are never clobbered).

use std::path::PathBuf;

use lingq_upload_lib::core::audio::AudioTrack;
use lingq_upload_lib::core::epub::{Chapter, ChapterId};
use lingq_upload_lib::core::identity::ProjectId;
use lingq_upload_lib::core::matcher::{
    seed_mapping_state, track_id_for, MappingOp, MappingPair, MappingState,
};
use lingq_upload_lib::core::project::Project;
use lingq_upload_lib::core::store::{
    InMemoryProjectStore, JsonProjectStore, ProjectStore, StoreError,
};
use tempfile::TempDir;

fn cid(order: usize) -> ChapterId {
    ChapterId::from_order(order)
}

fn chapter(order: usize) -> Chapter {
    Chapter {
        order,
        title: format!("c{order}"),
        body: format!("body {order}"),
        id: cid(order),
        ..Default::default()
    }
}

fn track(order: usize) -> AudioTrack {
    AudioTrack {
        order,
        path: PathBuf::from(format!("/audio/t{order}.mp3")),
        duration_sec: Some(60.0),
        title: None,
        window: None,
    }
}

fn seeded_project(store: &dyn ProjectStore, title: &str) -> (ProjectId, MappingState) {
    let id = ProjectId::from_title_author(title, "Author");
    let mut p = Project::new_test(id.clone(), title);
    let chapters = [chapter(0), chapter(1)];
    let tracks = [track(0), track(1)];
    let state = seed_mapping_state(&[(0, 0), (1, 1)], &chapters, &tracks);
    p.mapping = Some(state.clone());
    store.put(&p).unwrap();
    (id, state)
}

fn both_stores() -> (TempDir, Vec<Box<dyn ProjectStore>>) {
    let tmp = TempDir::new().unwrap();
    let stores: Vec<Box<dyn ProjectStore>> = vec![
        Box::new(JsonProjectStore::new(tmp.path())),
        Box::new(InMemoryProjectStore::new()),
    ];
    (tmp, stores)
}

#[test]
fn apply_mapping_op_rejects_stale_op_id_without_mutating() {
    let (_tmp, stores) = both_stores();
    for store in stores {
        let (id, initial) = seeded_project(store.as_ref(), "Stale Gate");
        let op = MappingOp::Park {
            track_id: track_id_for(&track(0)),
        };

        // op_id is 0, so the only accepted expected_op_id is 1.
        for stale in [0u64, 2, 99] {
            match store.apply_mapping_op(&id, op.clone(), stale) {
                Err(StoreError::MappingStaleOp { server, expected }) => {
                    assert_eq!(server, 0);
                    assert_eq!(expected, stale);
                }
                other => panic!("expected MappingStaleOp for {stale}, got {other:?}"),
            }
        }
        let persisted = store.get(&id).unwrap().unwrap().mapping.unwrap();
        assert_eq!(persisted, initial, "stale op must not mutate state");
    }
}

#[test]
fn apply_mapping_op_round_trips_through_persistence() {
    let (_tmp, stores) = both_stores();
    for store in stores {
        let (id, _) = seeded_project(store.as_ref(), "Round Trip");

        let returned = store
            .apply_mapping_op(
                &id,
                MappingOp::Park {
                    track_id: track_id_for(&track(1)),
                },
                1,
            )
            .expect("first op applies");
        assert_eq!(returned.op_id, 1);
        assert_eq!(returned.pairs[1].track_id, None);
        assert_eq!(returned.parking_lot, vec![track_id_for(&track(1))]);

        let persisted = store.get(&id).unwrap().unwrap().mapping.unwrap();
        assert_eq!(
            persisted, returned,
            "returned state must equal persisted state"
        );

        // Next op chains off the persisted op_id.
        let returned = store
            .apply_mapping_op(
                &id,
                MappingOp::Unpark {
                    track_id: track_id_for(&track(1)),
                    chapter_id: cid(1),
                },
                2,
            )
            .expect("second op applies");
        assert_eq!(returned.op_id, 2);
        assert_eq!(store.get(&id).unwrap().unwrap().mapping.unwrap(), returned);
    }
}

/// The seeding path: a project starts with `mapping: None`; the orchestrator
/// persists the matcher's initial state via a guarded `update`. After seeding,
/// the first real op succeeds instead of failing `UnknownChapter` on the
/// `unwrap_or_default()` empty state.
#[test]
fn guarded_update_seeds_mapping_once_and_never_clobbers_edits() {
    let (_tmp, stores) = both_stores();
    for store in stores {
        let id = ProjectId::from_title_author("Seeding", "Author");
        let p = Project::new_test(id.clone(), "Seeding");
        assert!(p.mapping.is_none());
        store.put(&p).unwrap();

        let chapters = [chapter(0), chapter(1)];
        let tracks = [track(0), track(1)];
        let seeded = seed_mapping_state(&[(0, 0), (1, 1)], &chapters, &tracks);

        // First seed lands.
        let after = store
            .update(&id, &mut |p| {
                if p.mapping.is_none() {
                    p.mapping = Some(seeded.clone());
                }
            })
            .unwrap();
        let state = after.mapping.expect("seeded");
        assert_eq!(state.op_id, 0);
        assert_eq!(state.pairs.len(), 2);
        assert!(state.pairs.iter().all(|pr| !pr.touched));
        assert!(state.parking_lot.is_empty());

        // The first real op now succeeds (no UnknownChapter on empty pairs).
        let edited = store
            .apply_mapping_op(
                &id,
                MappingOp::Swap {
                    chapter_id: cid(0),
                    track_id: track_id_for(&track(1)),
                },
                1,
            )
            .expect("op after seeding");
        assert_eq!(edited.pairs[0].track_id, Some(track_id_for(&track(1))));

        // Re-match re-seed attempt must not clobber the user's edit.
        let after = store
            .update(&id, &mut |p| {
                if p.mapping.is_none() {
                    p.mapping = Some(seeded.clone());
                }
            })
            .unwrap();
        assert_eq!(
            after.mapping.unwrap(),
            edited,
            "guarded seed must not overwrite an edited MappingState"
        );
    }
}

#[test]
fn update_returns_not_found_for_missing_project() {
    let (_tmp, stores) = both_stores();
    for store in stores {
        let ghost = ProjectId::from_title_author("Ghost", "Nobody");
        match store.update(&ghost, &mut |_| {}) {
            Err(StoreError::NotFound { .. }) => (),
            other => panic!("expected NotFound, got {other:?}"),
        }
    }
}

#[test]
fn seed_mapping_state_pairs_chapters_with_track_ids() {
    let chapters = [chapter(0), chapter(1), chapter(2)];
    let tracks = [track(0), track(1), track(2)];
    let state = seed_mapping_state(&[(0, 0), (1, 1), (2, 2)], &chapters, &tracks);

    let expected: Vec<MappingPair> = (0..3)
        .map(|i| MappingPair {
            chapter_id: cid(i),
            track_id: Some(track_id_for(&track(i))),
            confidence: state.pairs[i].confidence,
            touched: false,
            original_confidence: state.pairs[i].original_confidence,
        })
        .collect();
    assert_eq!(state.pairs, expected);
    assert!(state.parking_lot.is_empty());
    assert_eq!(state.op_id, 0);
}
