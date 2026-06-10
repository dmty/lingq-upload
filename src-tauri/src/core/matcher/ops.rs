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

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Default)]
pub struct MappingState {
    pub pairs: Vec<MappingPair>,
    #[serde(default)]
    pub parking_lot: Vec<TrackId>,
    #[serde(default)]
    pub op_id: u64,
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
        MappingOp::Swap { chapter_id, track_id } => swap(&mut next, chapter_id, track_id)?,
        MappingOp::Park { track_id } => park(&mut next, track_id)?,
        MappingOp::Unpark { track_id, chapter_id } => unpark(&mut next, track_id, chapter_id)?,
    }
    next.op_id = state.op_id.saturating_add(1);
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

/// True iff no paired pair is both red-`original_confidence` (< 0.6) AND
/// untouched. Unpaired pairs (`track_id == None`) never block — there is no
/// pairing to confirm. Pure.
pub fn gate_continue(state: &MappingState) -> bool {
    state.pairs.iter().all(|p| {
        p.touched || p.track_id.is_none() || p.original_confidence >= 0.6
    })
}
