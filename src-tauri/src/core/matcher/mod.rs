pub mod mismatch;

pub use mismatch::{allowed, classify, MismatchCondition, MismatchResponse};

use serde::{Deserialize, Serialize};
use specta::Type;

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

/// Auto-match by index when counts are equal. Otherwise classifies the
/// mismatch and returns the allowed response set.
///
/// Sprint 2 matcher is count-based only. Heading/track-title fuzzy match
/// is deferred; manual mapping editor lands later.
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
