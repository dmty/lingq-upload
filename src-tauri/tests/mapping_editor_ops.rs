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

fn seed_three_pairs() -> MappingState {
    MappingState {
        pairs: vec![
            MappingPair {
                chapter_id: ch("c0"),
                track_id: Some("t0".into()),
                confidence: 0.9,
                touched: false,
            },
            MappingPair {
                chapter_id: ch("c1"),
                track_id: Some("t1".into()),
                confidence: 0.5,
                touched: false,
            },
            MappingPair {
                chapter_id: ch("c2"),
                track_id: Some("t2".into()),
                confidence: 0.7,
                touched: false,
            },
        ],
        parking_lot: vec![],
        op_id: 0,
    }
}

#[test]
fn swap_moves_old_pair_to_parking_lot() {
    let state = MappingState {
        pairs: vec![
            MappingPair {
                chapter_id: ch("c0"),
                track_id: Some("t0".into()),
                confidence: 0.9,
                touched: false,
            },
            MappingPair {
                chapter_id: ch("c1"),
                track_id: Some("t1".into()),
                confidence: 0.5,
                touched: false,
            },
        ],
        parking_lot: vec!["t9".into()],
        op_id: 5,
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

    // Assign c0's t0 to c2; c0 becomes unpaired, c2's prior t2 goes to lot.
    let next = apply_mapping_op(
        &state,
        MappingOp::Swap {
            chapter_id: ch("c2"),
            track_id: "t0".into(),
        },
    )
    .unwrap();

    assert_eq!(next.pairs[0].track_id, None);
    assert_eq!(next.pairs[2].track_id, Some("t0".into()));
    assert!(next.pairs[2].touched);
    assert!(next.parking_lot.contains(&"t2".to_string()));
}

#[test]
fn park_unpairs_and_pushes_to_lot() {
    let state = seed_three_pairs();
    let next = apply_mapping_op(&state, MappingOp::Park { track_id: "t1".into() }).unwrap();

    assert_eq!(next.pairs[1].track_id, None);
    assert!(next.pairs[1].touched);
    assert_eq!(next.parking_lot, vec!["t1".to_string()]);
    assert_eq!(next.op_id, 1);
}

#[test]
fn park_unknown_track_errors() {
    let state = seed_three_pairs();
    let err = apply_mapping_op(&state, MappingOp::Park { track_id: "nope".into() }).unwrap_err();
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
            MappingPair {
                chapter_id: ch("c0"),
                track_id: Some("t0".into()),
                confidence: 0.9,
                touched: false,
            },
            MappingPair {
                chapter_id: ch("c1"),
                track_id: Some("t1".into()),
                confidence: 0.4,
                touched: false,
            },
        ],
        parking_lot: vec![],
        op_id: 0,
    };
    assert!(!gate_continue(&state));
}

#[test]
fn gate_continue_clears_after_touch() {
    let state = MappingState {
        pairs: vec![MappingPair {
            chapter_id: ch("c1"),
            track_id: Some("t1".into()),
            confidence: 0.4,
            touched: true,
        }],
        parking_lot: vec![],
        op_id: 0,
    };
    assert!(gate_continue(&state));
}

#[test]
fn gate_continue_allows_all_green() {
    let state = MappingState {
        pairs: vec![
            MappingPair {
                chapter_id: ch("c0"),
                track_id: Some("t0".into()),
                confidence: 0.9,
                touched: false,
            },
            MappingPair {
                chapter_id: ch("c1"),
                track_id: Some("t1".into()),
                confidence: 0.6,
                touched: false,
            },
        ],
        parking_lot: vec![],
        op_id: 0,
    };
    assert!(gate_continue(&state));
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
