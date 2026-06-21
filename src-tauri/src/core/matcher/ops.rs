//! Pure state transitions for the two-column mapping editor.
//!
//! The mapping editor lets the user re-confirm the matcher's chapter ↔ track
//! pairings. This module owns the data model and the three mutating ops:
//! `Swap`, `Park`, and `Unpark`. All ops are pure functions over
//! [`MappingState`] — no I/O, no async, no DOM. Persistence lives in the
//! caller (`commands::mapping::cmd_apply_mapping_op`).
//!
//! Confidence note: the current count-based matcher (see
//! `core/matcher/mod.rs`) does not produce a per-pair confidence score. Until
//! a scoring function lands, recomputed pairs receive a placeholder of `1.0`.
//! This keeps the score gate sane (every touched pair clears the < 0.6 red
//! band) and lets the UI exercise the colour code via fixtures.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::core::epub::ChapterId;

pub type TrackId = String;

/// Recomputed confidence for a freshly-(re)paired chapter/track. The current
/// matcher is count-based so we have nothing better than a constant; once the
/// matcher learns to score (title similarity, duration ratio, etc) this
/// becomes a call into `pack::score(...)`.
pub const RECOMPUTED_CONFIDENCE: f32 = 1.0;

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MappingOp {
    Swap {
        chapter_id: ChapterId,
        track_id: TrackId,
    },
    Park {
        track_id: TrackId,
    },
    Unpark {
        track_id: TrackId,
        chapter_id: ChapterId,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct MappingPair {
    pub chapter_id: ChapterId,
    #[serde(default)]
    pub track_id: Option<TrackId>,
    pub confidence: f32,
    #[serde(default)]
    pub touched: bool,
    /// Confidence at pair construction. The score gate blocks Continue when
    /// the original score is red AND the pair is still untouched, even if
    /// `confidence` was bumped to the recomputed placeholder by an op.
    #[serde(default)]
    pub original_confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BucketMeta {
    pub track_id: TrackId,
    pub atom_title: Option<String>,
    pub atom_duration_sec: f64,
    pub chars_per_sec: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Default)]
pub struct MappingState {
    pub pairs: Vec<MappingPair>,
    #[serde(default)]
    pub parking_lot: Vec<TrackId>,
    #[serde(default)]
    pub op_id: u64,
    #[serde(default)]
    pub partition_locked: bool,
    #[serde(default)]
    pub buckets: Vec<BucketMeta>,
}

#[derive(Debug, thiserror::Error, Serialize, Deserialize, Type, PartialEq)]
#[serde(tag = "kind", content = "message")]
pub enum MappingError {
    #[error("unknown chapter: {0}")]
    UnknownChapter(String),
    #[error("unknown track: {0}")]
    UnknownTrack(String),
    #[error("invalid op: {0}")]
    Invalid(String),
}

/// Apply `op` to `state`, returning a new state. Pure: caller persists.
pub fn apply(state: &MappingState, op: MappingOp) -> Result<MappingState, MappingError> {
    let mut next = state.clone();
    match op {
        MappingOp::Swap {
            chapter_id,
            track_id,
        } => swap(&mut next, chapter_id, track_id)?,
        MappingOp::Park { track_id } => park(&mut next, track_id)?,
        MappingOp::Unpark {
            track_id,
            chapter_id,
        } => unpark(&mut next, track_id, chapter_id)?,
    }
    next.op_id = state.op_id.saturating_add(1);
    next.partition_locked = true;
    Ok(next)
}

fn swap(
    state: &mut MappingState,
    chapter_id: ChapterId,
    track_id: TrackId,
) -> Result<(), MappingError> {
    let target_idx = pair_idx(state, &chapter_id)?;

    // Is the track in the parking lot, or paired to another chapter?
    let in_lot = state.parking_lot.iter().position(|t| t == &track_id);
    let source_idx = state
        .pairs
        .iter()
        .position(|p| p.track_id.as_ref() == Some(&track_id));

    if in_lot.is_none() && source_idx.is_none() {
        return Err(MappingError::UnknownTrack(track_id));
    }

    // Snapshot the displaced track before we mutate target.
    let displaced = state.pairs[target_idx].track_id.clone();

    state.pairs[target_idx].track_id = Some(track_id.clone());
    state.pairs[target_idx].confidence = RECOMPUTED_CONFIDENCE;
    state.pairs[target_idx].touched = true;

    if let Some(idx) = in_lot {
        state.parking_lot.remove(idx);
    } else if let Some(src) = source_idx {
        if src != target_idx {
            state.pairs[src].track_id = None;
            state.pairs[src].confidence = state.pairs[src].original_confidence;
            state.pairs[src].touched = true;
        }
    }

    if let Some(prev) = displaced {
        if prev != track_id {
            state.parking_lot.push(prev);
        }
    }

    Ok(())
}

fn park(state: &mut MappingState, track_id: TrackId) -> Result<(), MappingError> {
    let idx = state
        .pairs
        .iter()
        .position(|p| p.track_id.as_ref() == Some(&track_id))
        .ok_or_else(|| MappingError::UnknownTrack(track_id.clone()))?;

    state.pairs[idx].track_id = None;
    state.pairs[idx].confidence = state.pairs[idx].original_confidence;
    state.pairs[idx].touched = true;
    state.parking_lot.push(track_id);
    Ok(())
}

fn unpark(
    state: &mut MappingState,
    track_id: TrackId,
    chapter_id: ChapterId,
) -> Result<(), MappingError> {
    let target = pair_idx(state, &chapter_id)?;
    if state.pairs[target].track_id.is_some() {
        return Err(MappingError::Invalid(format!(
            "chapter {chapter_id} already has a track; swap instead"
        )));
    }
    let lot_idx = state
        .parking_lot
        .iter()
        .position(|t| t == &track_id)
        .ok_or_else(|| MappingError::UnknownTrack(track_id.clone()))?;
    state.parking_lot.remove(lot_idx);
    state.pairs[target].track_id = Some(track_id);
    state.pairs[target].confidence = RECOMPUTED_CONFIDENCE;
    state.pairs[target].touched = true;
    Ok(())
}

fn pair_idx(state: &MappingState, chapter_id: &ChapterId) -> Result<usize, MappingError> {
    state
        .pairs
        .iter()
        .position(|p| &p.chapter_id == chapter_id)
        .ok_or_else(|| MappingError::UnknownChapter(chapter_id.to_string()))
}

/// Group contiguous pairs by shared `track_id` into per-bucket audio metadata.
/// `track_meta` is (track_id, title, duration_sec); chapters without a track
/// (parking-lot remnants) start no bucket.
pub fn build_bucket_meta(
    pairs: &[MappingPair],
    track_meta: &[(TrackId, Option<String>, f64)],
    chars_by_chapter: &std::collections::HashMap<ChapterId, usize>,
) -> Vec<BucketMeta> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < pairs.len() {
        let Some(tid) = pairs[i].track_id.clone() else { i += 1; continue; };
        let start = i;
        while i < pairs.len() && pairs[i].track_id.as_ref() == Some(&tid) {
            i += 1;
        }
        let chars: usize = pairs[start..i]
            .iter()
            .map(|p| *chars_by_chapter.get(&p.chapter_id).unwrap_or(&0))
            .sum();
        let (title, dur) = track_meta
            .iter()
            .find(|(t, _, _)| *t == tid)
            .map(|(_, ti, d)| (ti.clone(), *d))
            .unwrap_or((None, 0.0));
        let chars_per_sec = if dur > 0.0 { chars as f64 / dur } else { 0.0 };
        out.push(BucketMeta { track_id: tid, atom_title: title, atom_duration_sec: dur, chars_per_sec });
    }
    out
}

/// True iff no paired pair is both red-`original_confidence` (< 0.6) AND
/// untouched. Unpaired pairs (`track_id == None`) never block — there is no
/// pairing to confirm. Pure.
pub fn gate_continue(state: &MappingState) -> bool {
    state
        .pairs
        .iter()
        .all(|p| p.touched || p.track_id.is_none() || p.original_confidence >= 0.6)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::epub::ChapterId;

    fn cid(s: &str) -> ChapterId {
        ChapterId(s.to_string())
    }

    #[test]
    fn apply_locks_the_partition() {
        let state = MappingState {
            pairs: vec![MappingPair {
                chapter_id: cid("c0"),
                track_id: Some("t0".into()),
                confidence: 1.0,
                touched: false,
                original_confidence: 1.0,
            }],
            partition_locked: false,
            ..Default::default()
        };
        let next = apply(&state, MappingOp::Park { track_id: "t0".into() }).unwrap();
        assert!(next.partition_locked, "any op must freeze the partition");
    }

    #[test]
    fn build_bucket_meta_groups_contiguous_track_ids() {
        use std::collections::HashMap;
        let pairs = vec![
            MappingPair { chapter_id: cid("c0"), track_id: Some("t0".into()), confidence: 1.0, touched: false, original_confidence: 1.0 },
            MappingPair { chapter_id: cid("c1"), track_id: Some("t0".into()), confidence: 1.0, touched: false, original_confidence: 1.0 },
            MappingPair { chapter_id: cid("c2"), track_id: Some("t1".into()), confidence: 1.0, touched: false, original_confidence: 1.0 },
        ];
        let track_meta = vec![
            ("t0".to_string(), Some("Audio 1".to_string()), 100.0),
            ("t1".to_string(), Some("Audio 2".to_string()), 50.0),
        ];
        let chars: HashMap<ChapterId, usize> =
            [(cid("c0"), 300), (cid("c1"), 200), (cid("c2"), 100)].into();
        let buckets = build_bucket_meta(&pairs, &track_meta, &chars);
        assert_eq!(buckets.len(), 2);
        assert_eq!(buckets[0].track_id, "t0");
        assert_eq!(buckets[0].atom_duration_sec, 100.0);
        assert_eq!(buckets[0].chars_per_sec, 5.0); // (300+200)/100
        assert_eq!(buckets[1].chars_per_sec, 2.0); // 100/50
    }
}
