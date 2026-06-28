use std::path::Path;

use lingq_upload_lib::core::audio::AudioTrack;
use lingq_upload_lib::core::epub::Chapter;
use lingq_upload_lib::core::matcher::{
    allowed, auto_match, classify, MatchOutcome, MismatchCondition, MismatchResponse,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    chapters: usize,
    tracks: usize,
    expected_outcome: String,
    expected_condition: Option<String>,
    expected_options: Option<Vec<String>>,
    expected_preselect: Option<String>,
}

fn condition_str(c: MismatchCondition) -> &'static str {
    match c {
        MismatchCondition::OneToMany => "OneToMany",
        MismatchCondition::ManyToOne => "ManyToOne",
        MismatchCondition::ManyToFew => "ManyToFew",
        MismatchCondition::CountOff => "CountOff",
        MismatchCondition::Unalignable => "Unalignable",
        MismatchCondition::Unknown => "Unknown",
    }
}

fn response_str(r: MismatchResponse) -> &'static str {
    match r {
        MismatchResponse::PairAccept => "PairAccept",
        MismatchResponse::PairDrop => "PairDrop",
        MismatchResponse::SingleLesson => "SingleLesson",
        MismatchResponse::SplitProportional => "SplitProportional",
        MismatchResponse::Cancel => "Cancel",
        MismatchResponse::Unknown => "Unknown",
    }
}

fn check_fixture(path: &Path) {
    let raw = std::fs::read_to_string(path).expect("read fixture");
    let f: Fixture = serde_json::from_str(&raw).expect("parse fixture");
    let outcome = auto_match_for_counts(f.chapters, f.tracks);
    match (&f.expected_outcome[..], outcome) {
        ("Paired", MatchOutcome::Paired { pairs }) => {
            assert_eq!(pairs.len(), f.chapters);
        }
        (
            "Mismatch",
            MatchOutcome::Mismatch {
                condition,
                options,
                preselect,
            },
        ) => {
            assert_eq!(
                Some(condition_str(condition).to_string()),
                f.expected_condition,
                "{}",
                path.display()
            );
            let got: Vec<String> = options
                .iter()
                .copied()
                .map(|r| response_str(r).into())
                .collect();
            assert_eq!(Some(got), f.expected_options, "{}", path.display());
            assert_eq!(
                Some(response_str(preselect).to_string()),
                f.expected_preselect,
                "{}",
                path.display()
            );
        }
        (other, outcome) => panic!(
            "unexpected outcome for {}: expected {other}, got {outcome:?}",
            path.display()
        ),
    }
}

fn auto_match_for_counts(c: usize, t: usize) -> MatchOutcome {
    let chapters: Vec<Chapter> = (0..c)
        .map(|i| Chapter {
            order: i,
            title: format!("c{i}"),
            body: String::new(),
            ..Default::default()
        })
        .collect();
    let tracks: Vec<AudioTrack> = (0..t)
        .map(|i| AudioTrack {
            order: i,
            path: std::path::PathBuf::from(format!("/tmp/t{i}.mp3")),
            duration_sec: None,
            title: None,
            window: None,
        })
        .collect();
    auto_match(&chapters, &tracks)
}

#[test]
fn fixtures_clean_pair() {
    check_fixture(Path::new("tests/fixtures/matcher/clean_pair.json"));
}
#[test]
fn fixtures_one_to_many() {
    check_fixture(Path::new("tests/fixtures/matcher/one_to_many.json"));
}
#[test]
fn fixtures_many_to_one() {
    check_fixture(Path::new("tests/fixtures/matcher/many_to_one.json"));
}
#[test]
fn fixtures_count_off_plus_one() {
    check_fixture(Path::new("tests/fixtures/matcher/count_off_plus_one.json"));
}
#[test]
fn fixtures_count_off_minus_two() {
    check_fixture(Path::new("tests/fixtures/matcher/count_off_minus_two.json"));
}
#[test]
fn fixtures_unalignable() {
    check_fixture(Path::new("tests/fixtures/matcher/unalignable.json"));
}

#[test]
fn empty_inputs_classify_as_unalignable() {
    use lingq_upload_lib::core::matcher::MatchOutcome;
    let outcome = auto_match_for_counts(0, 0);
    match outcome {
        MatchOutcome::Mismatch { condition, .. } => {
            assert_eq!(condition, MismatchCondition::Unalignable);
        }
        _ => panic!("expected Mismatch for (0, 0)"),
    }
}

#[test]
fn equal_counts_returns_paired_by_index() {
    let outcome = auto_match_for_counts(3, 3);
    match outcome {
        MatchOutcome::Paired { pairs } => {
            assert_eq!(pairs, vec![(0, 0), (1, 1), (2, 2)]);
        }
        _ => panic!("expected Paired"),
    }
}

#[test]
fn manual_pair_is_not_a_response() {
    for c in [
        MismatchCondition::OneToMany,
        MismatchCondition::ManyToOne,
        MismatchCondition::ManyToFew,
        MismatchCondition::CountOff,
        MismatchCondition::Unalignable,
        MismatchCondition::Unknown,
    ] {
        let (opts, _) = allowed(c);
        for opt in opts {
            // No "ManualPair" variant exists; ensure preselect/options are within enum.
            assert!(matches!(
                opt,
                MismatchResponse::PairAccept
                    | MismatchResponse::PairDrop
                    | MismatchResponse::SingleLesson
                    | MismatchResponse::SplitProportional
                    | MismatchResponse::Cancel
                    | MismatchResponse::Unknown
            ));
        }
    }
}

#[test]
fn classify_boundary_cases() {
    // (0, 0) is not a clean pair — empty book ↔ no audio has nothing to align.
    assert_eq!(classify(0, 0), Some(MismatchCondition::Unalignable));
    assert_eq!(classify(0, 5), Some(MismatchCondition::Unalignable));
    assert_eq!(classify(5, 0), Some(MismatchCondition::Unalignable));
    assert_eq!(classify(1, 1), None);
    assert_eq!(classify(1, 2), Some(MismatchCondition::CountOff));
    assert_eq!(classify(1, 3), Some(MismatchCondition::OneToMany));
    assert_eq!(classify(3, 1), Some(MismatchCondition::ManyToOne));
    assert_eq!(classify(5, 20), Some(MismatchCondition::Unalignable));
    // (20, 5) is chapters-heavy with ratio 4.0 — within the ManyToFew band
    // (>1.5, ≤30×). The text-side Unalignable branch only fires when neither
    // ManyToFew arm matches; here ManyToFew correctly wins.
    assert_eq!(classify(20, 5), Some(MismatchCondition::ManyToFew));

    // ManyToFew: chapters > tracks, both >= 2, ratio strictly above 1.5, <= 30×.
    assert_eq!(classify(6, 3), Some(MismatchCondition::ManyToFew));
    assert_eq!(classify(85, 6), Some(MismatchCondition::ManyToFew));

    // CountOff wins over ManyToFew when |c - t| <= 2 (precedence guard).
    assert_eq!(classify(4, 2), Some(MismatchCondition::CountOff));
    assert_eq!(classify(5, 3), Some(MismatchCondition::CountOff));
    assert_eq!(classify(22, 20), Some(MismatchCondition::CountOff));

    // tracks == 1 still wins ManyToOne even at extreme ratios.
    assert_eq!(classify(85, 1), Some(MismatchCondition::ManyToOne));
}
