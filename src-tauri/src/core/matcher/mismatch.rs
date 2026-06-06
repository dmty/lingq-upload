use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MismatchCondition {
    OneToMany,
    ManyToOne,
    CountOff,
    Unalignable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MismatchResponse {
    PairAccept,
    PairDrop,
    SingleLesson,
    Cancel,
}

/// Classify a count-pair. `None` means equal non-zero counts → clean pair.
///
/// Precedence (high → low): equal-nonzero → empty-vs-content → OneToMany →
/// ManyToOne → CountOff → Unalignable.
///
/// - `(0, 0)` is *not* a clean pair — an empty book paired with no audio is
///   Unalignable; there is nothing to align.
/// - `(0, n)` or `(n, 0)` for n > 0 is Unalignable for the same reason.
/// - CountOff covers `|c − t| ≤ 2` (with both sides non-zero).
/// - Unalignable covers ratio > 3× or < 1/3.
pub fn classify(chapters: usize, tracks: usize) -> Option<MismatchCondition> {
    if chapters == 0 || tracks == 0 {
        return Some(MismatchCondition::Unalignable);
    }
    if chapters == tracks {
        return None;
    }
    if chapters == 1 && tracks >= 3 {
        return Some(MismatchCondition::OneToMany);
    }
    if tracks == 1 && chapters >= 3 {
        return Some(MismatchCondition::ManyToOne);
    }
    let delta = chapters.abs_diff(tracks);
    if delta <= 2 {
        return Some(MismatchCondition::CountOff);
    }
    let (lo, hi) = (chapters.min(tracks), chapters.max(tracks));
    if hi > lo.saturating_mul(3) {
        return Some(MismatchCondition::Unalignable);
    }
    Some(MismatchCondition::CountOff)
}

/// Allowed responses + preselected default for each condition.
pub fn allowed(condition: MismatchCondition) -> (Vec<MismatchResponse>, MismatchResponse) {
    use MismatchCondition::*;
    use MismatchResponse::*;
    match condition {
        OneToMany => (vec![SingleLesson, Cancel], SingleLesson),
        ManyToOne => (vec![SingleLesson, Cancel], SingleLesson),
        CountOff => (vec![PairAccept, PairDrop, Cancel], PairAccept),
        Unalignable => (vec![SingleLesson, Cancel], Cancel),
    }
}
