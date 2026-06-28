use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MismatchCondition {
    OneToMany,
    ManyToOne,
    ManyToFew,
    CountOff,
    Unalignable,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MismatchResponse {
    PairAccept,
    PairDrop,
    SingleLesson,
    SplitProportional,
    Cancel,
    #[serde(other)]
    Unknown,
}

/// Classify a count-pair. `None` means equal non-zero counts → clean pair.
///
/// Precedence (high → low): equal-nonzero → empty-vs-content → OneToMany →
/// ManyToOne → CountOff → ManyToFew → Unalignable.
///
/// - `(0, 0)` is *not* a clean pair — an empty book paired with no audio is
///   Unalignable; there is nothing to align.
/// - `(0, n)` or `(n, 0)` for n > 0 is Unalignable for the same reason.
/// - CountOff covers `|c − t| ≤ 2` (with both sides non-zero) and wins over
///   ManyToFew for small near-misses such as `(22, 20)` or `(5, 3)`.
/// - ManyToFew covers `chapters > tracks` with ratio strictly above 1.5
///   (encoded as `2·c > 3·t` to stay on integers) and at most 30×.
/// - Unalignable covers ratio > 3× when neither side is 1, plus the extreme
///   tail above 30× on the chapters-heavy side.
///
/// `Unknown` is never returned — that variant only appears via deserialisation
/// of foreign tags from a newer build.
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
    if chapters > tracks && tracks >= 2 && 2 * chapters > 3 * tracks && chapters <= 30 * tracks {
        return Some(MismatchCondition::ManyToFew);
    }
    let (lo, hi) = (chapters.min(tracks), chapters.max(tracks));
    if hi > lo.saturating_mul(3) {
        return Some(MismatchCondition::Unalignable);
    }
    Some(MismatchCondition::CountOff)
}

/// Allowed responses + preselected default for each condition.
pub fn allowed(condition: MismatchCondition) -> (Vec<MismatchResponse>, MismatchResponse) {
    use MismatchResponse::*;
    match condition {
        MismatchCondition::OneToMany => (vec![SingleLesson, Cancel], SingleLesson),
        MismatchCondition::ManyToOne => (vec![SingleLesson, Cancel], SingleLesson),
        MismatchCondition::CountOff => (vec![PairAccept, PairDrop, Cancel], PairAccept),
        MismatchCondition::ManyToFew => (
            vec![SplitProportional, SingleLesson, Cancel],
            SplitProportional,
        ),
        MismatchCondition::Unalignable => (vec![SingleLesson, Cancel], Cancel),
        // Foreign tag from a newer build — refuse to act. The user must redo
        // the match step on a build that understands the variant.
        MismatchCondition::Unknown => (vec![Cancel], Cancel),
    }
}
