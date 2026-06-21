pub mod mismatch;
pub mod ops;
pub mod pack;

pub use mismatch::{allowed, classify, MismatchCondition, MismatchResponse};
pub use ops::{
    apply as apply_mapping_op, gate_continue, MappingError, MappingOp, MappingPair, MappingState,
    TrackId,
};
pub use pack::{build_preview, proportional_pack, Bucket, BucketPreview};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::core::audio::AudioTrack;
use crate::core::epub::Chapter;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MatchOutcome {
    Paired {
        pairs: Vec<(usize, usize)>,
    },
    Mismatch {
        condition: MismatchCondition,
        options: Vec<MismatchResponse>,
        preselect: MismatchResponse,
    },
}

/// Pair chapters ↔ tracks. Count-based: equal counts pair by index;
/// otherwise classify the mismatch and return the allowed response set.
pub fn auto_match(chapters: &[Chapter], tracks: &[AudioTrack]) -> MatchOutcome {
    auto_match_counts(chapters.len(), tracks.len())
}

/// Stable identifier for an audio track in [`MappingState`]. Tracks resolve
/// from sorted paths, so the path alone is stable; embedded-chapter fan-out
/// reuses one path for many tracks, so the slice window disambiguates.
pub fn track_id_for(track: &AudioTrack) -> TrackId {
    match track.window {
        Some((start, end)) => format!("{}#{start}-{end}", track.path.display()),
        None => track.path.display().to_string(),
    }
}

/// Initial [`MappingState`] for a cleanly auto-matched project: one untouched
/// pair per chapter, empty parking lot, `op_id` 0. `pairs` carries the
/// positional (chapter, track) indices from [`MatchOutcome::Paired`].
pub fn seed_mapping_state(
    pairs: &[(usize, usize)],
    chapters: &[Chapter],
    tracks: &[AudioTrack],
) -> MappingState {
    MappingState {
        pairs: pairs
            .iter()
            .map(|&(c, t)| MappingPair {
                chapter_id: chapters[c].id.clone(),
                track_id: Some(track_id_for(&tracks[t])),
                confidence: ops::RECOMPUTED_CONFIDENCE,
                touched: false,
                original_confidence: ops::RECOMPUTED_CONFIDENCE,
            })
            .collect(),
        parking_lot: Vec::new(),
        op_id: 0,
        buckets: Vec::new(),
    }
}

pub fn auto_match_counts(chapters: usize, tracks: usize) -> MatchOutcome {
    match classify(chapters, tracks) {
        None => {
            let pairs = (0..chapters).map(|i| (i, i)).collect();
            MatchOutcome::Paired { pairs }
        }
        Some(condition) => {
            let (options, preselect) = allowed(condition);
            MatchOutcome::Mismatch {
                condition,
                options,
                preselect,
            }
        }
    }
}
