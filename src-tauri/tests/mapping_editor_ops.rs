//! Pure-function contract for the two-column mapping editor.
//!
//! Covers `Swap`, `Park`, `Unpark`, the parking-lot moves, the op_id bump,
//! and the score-gate predicate. No DOM, no I/O.

use lingq_upload_lib::core::epub::ChapterId;
use lingq_upload_lib::core::matcher::{
    apply_mapping_op, gate_continue, MappingError, MappingOp, MappingPair, MappingState,
};

fn ch(id: &str) -> ChapterId {
    ChapterId(id.into())
}

fn pair(chapter: &str, track: Option<&str>, confidence: f32, touched: bool) -> MappingPair {
    MappingPair {
        chapter_id: ch(chapter),
        track_id: track.map(|s| s.to_string()),
        confidence,
        touched,
        original_confidence: confidence,
    }
}

fn seed_three_pairs() -> MappingState {
    MappingState {
        pairs: vec![
            pair("c0", Some("t0"), 0.9, false),
            pair("c1", Some("t1"), 0.5, false),
            pair("c2", Some("t2"), 0.7, false),
        ],
        parking_lot: vec![],
        op_id: 0,
        ..Default::default()
    }
}

#[test]
fn swap_moves_old_pair_to_parking_lot() {
    let state = MappingState {
        pairs: vec![
            pair("c0", Some("t0"), 0.9, false),
            pair("c1", Some("t1"), 0.5, false),
        ],
        parking_lot: vec!["t9".into()],
        op_id: 5,
        ..Default::default()
    };

    let next = apply_mapping_op(
        &state,
        MappingOp::Swap {
            chapter_id: ch("c1"),
            track_id: "t9".into(),
        },
    )
    .unwrap();

    assert_eq!(next.op_id, 6);
    assert_eq!(next.pairs[1].track_id, Some("t9".into()));
    assert!(next.pairs[1].touched);
    assert!(next.parking_lot.contains(&"t1".to_string()));
    assert!(!next.parking_lot.contains(&"t9".to_string()));
}

#[test]
fn swap_between_chapters_unpairs_source_chapter() {
    let state = seed_three_pairs();

    // Assign c0's t0 to c2; c0 becomes unpaired (touched, confidence reset to
    // original — visual "no pairing" signal); c2's prior t2 goes to lot.
    let next = apply_mapping_op(
        &state,
        MappingOp::Swap {
            chapter_id: ch("c2"),
            track_id: "t0".into(),
        },
    )
    .unwrap();

    assert_eq!(next.pairs[0].track_id, None);
    assert!(next.pairs[0].touched);
    assert_eq!(next.pairs[0].confidence, next.pairs[0].original_confidence);
    assert_eq!(next.pairs[2].track_id, Some("t0".into()));
    assert!(next.pairs[2].touched);
    assert!(next.parking_lot.contains(&"t2".to_string()));
}

#[test]
fn park_unpairs_and_pushes_to_lot() {
    let state = seed_three_pairs();
    let next = apply_mapping_op(
        &state,
        MappingOp::Park {
            track_id: "t1".into(),
        },
    )
    .unwrap();

    assert_eq!(next.pairs[1].track_id, None);
    assert!(next.pairs[1].touched);
    assert_eq!(next.parking_lot, vec!["t1".to_string()]);
    assert_eq!(next.op_id, 1);
}

#[test]
fn park_unknown_track_errors() {
    let state = seed_three_pairs();
    let err = apply_mapping_op(
        &state,
        MappingOp::Park {
            track_id: "nope".into(),
        },
    )
    .unwrap_err();
    assert!(matches!(err, MappingError::UnknownTrack(_)));
}

#[test]
fn unpark_moves_track_from_lot_to_chapter() {
    let mut state = seed_three_pairs();
    state.pairs[0].track_id = None;
    state.parking_lot = vec!["spare".into()];

    let next = apply_mapping_op(
        &state,
        MappingOp::Unpark {
            track_id: "spare".into(),
            chapter_id: ch("c0"),
        },
    )
    .unwrap();

    assert_eq!(next.pairs[0].track_id, Some("spare".into()));
    assert!(next.pairs[0].touched);
    assert!(next.parking_lot.is_empty());
}

#[test]
fn unpark_into_paired_chapter_errors() {
    let mut state = seed_three_pairs();
    state.parking_lot = vec!["spare".into()];
    let err = apply_mapping_op(
        &state,
        MappingOp::Unpark {
            track_id: "spare".into(),
            chapter_id: ch("c0"),
        },
    )
    .unwrap_err();
    assert!(matches!(err, MappingError::Invalid(_)));
}

#[test]
fn unknown_chapter_errors() {
    let state = seed_three_pairs();
    let err = apply_mapping_op(
        &state,
        MappingOp::Swap {
            chapter_id: ch("ghost"),
            track_id: "t0".into(),
        },
    )
    .unwrap_err();
    assert!(matches!(err, MappingError::UnknownChapter(_)));
}

#[test]
fn op_id_is_monotonic() {
    let mut state = seed_three_pairs();
    state.op_id = 10;
    let next = apply_mapping_op(
        &state,
        MappingOp::Park {
            track_id: "t0".into(),
        },
    )
    .unwrap();
    assert_eq!(next.op_id, 11);
}

#[test]
fn gate_continue_blocks_untouched_red() {
    let state = MappingState {
        pairs: vec![
            pair("c0", Some("t0"), 0.9, false),
            pair("c1", Some("t1"), 0.4, false),
        ],
        parking_lot: vec![],
        op_id: 0,
        ..Default::default()
    };
    assert!(!gate_continue(&state));
}

#[test]
fn gate_continue_clears_after_touch() {
    let state = MappingState {
        pairs: vec![pair("c1", Some("t1"), 0.4, true)],
        parking_lot: vec![],
        op_id: 0,
        ..Default::default()
    };
    assert!(gate_continue(&state));
}

#[test]
fn gate_continue_allows_all_green() {
    let state = MappingState {
        pairs: vec![
            pair("c0", Some("t0"), 0.9, false),
            pair("c1", Some("t1"), 0.6, false),
        ],
        parking_lot: vec![],
        op_id: 0,
        ..Default::default()
    };
    assert!(gate_continue(&state));
}

#[test]
fn gate_continue_ignores_unpaired_chapters() {
    // An unpaired chapter (track_id == None) with red original_confidence
    // must never block Continue — there is no pairing to confirm.
    let state = MappingState {
        pairs: vec![
            pair("c0", Some("t0"), 0.9, false),
            pair("c1", None, 0.2, false),
        ],
        parking_lot: vec!["t1".into()],
        op_id: 0,
        ..Default::default()
    };
    assert!(gate_continue(&state));
}

#[test]
fn gate_continue_uses_original_confidence_not_recomputed() {
    // After Swap, `confidence` jumps to the placeholder RECOMPUTED_CONFIDENCE
    // (1.0). The gate must still see the original red score until the user
    // touches the row.
    let state = seed_three_pairs();
    // c1 is red (0.5) originally; bump `confidence` to mimic a stale apply
    // without touching the row.
    let mut tampered = state.clone();
    tampered.pairs[1].confidence = 1.0;
    tampered.pairs[1].touched = false;
    assert!(!gate_continue(&tampered));
}

#[test]
fn swap_with_self_is_idempotent_touch() {
    let state = seed_three_pairs();
    let next = apply_mapping_op(
        &state,
        MappingOp::Swap {
            chapter_id: ch("c0"),
            track_id: "t0".into(),
        },
    )
    .unwrap();
    assert_eq!(next.pairs[0].track_id, Some("t0".into()));
    assert!(next.pairs[0].touched);
    assert!(next.parking_lot.is_empty());
}
