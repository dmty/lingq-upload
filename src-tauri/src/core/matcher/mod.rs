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
