use std::collections::HashMap;

use lingq_upload_lib::core::epub::{Chapter, ChapterId};
use lingq_upload_lib::transcribe::{
    title_match, transcript_match_head, transcript_match_tail, AlignmentConfig, BoundaryResult,
    DetectedRange,
};
use serde::Deserialize;

const CORPUS: &str = include_str!("fixtures/transcribe/corpus.json");

#[derive(Deserialize)]
struct CorpusChapter {
    id: ChapterId,
    title: String,
    body: String,
}

#[derive(Deserialize)]
struct CorpusCase {
    id: String,
    language: String,
    chapters: Vec<CorpusChapter>,
    first_audio_title: Option<String>,
    last_audio_title: Option<String>,
    head_transcript: String,
    tail_transcript: String,
    expected_start_chapter_id: ChapterId,
    expected_end_chapter_id: ChapterId,
    valid_sample: bool,
}

struct CorpusMetrics {
    auto_accept_precision: f64,
    valid_top1_inclusion: f64,
}

/// NFR12 release floors for auto mode. Manual detection stays callable below
/// them; only the unattended path is gated.
fn meets_auto_release_gate(metrics: &CorpusMetrics) -> bool {
    metrics.auto_accept_precision >= 0.95 && metrics.valid_top1_inclusion >= 0.90
}

fn corpus_chapters(case: &CorpusCase) -> Vec<Chapter> {
    case.chapters
        .iter()
        .enumerate()
        .map(|(order, chapter)| Chapter {
            order,
            id: chapter.id.clone(),
            title: chapter.title.clone(),
            body: chapter.body.clone(),
            ..Default::default()
        })
        .collect()
}

fn evaluate_corpus(cases: &[CorpusCase]) -> CorpusMetrics {
    let config = AlignmentConfig::default();
    let mut auto_accepted = 0;
    let mut correct_auto_accepted = 0;
    let mut valid_samples = 0;
    let mut valid_top1 = 0;
    let mut stage_a_cases = 0;
    let mut stage_b_cases = 0;

    for case in cases {
        let chapters = corpus_chapters(case);
        let title = title_match(
            case.first_audio_title.as_deref(),
            case.last_audio_title.as_deref(),
            &chapters,
            &config,
        );
        let (path, auto_range, top1_pair, head_score, tail_score) = if let Some(alignment) = title {
            stage_a_cases += 1;
            let top1 = (
                alignment.range.start_chapter_id.clone(),
                alignment.range.end_chapter_id.clone(),
            );
            (
                "stage_a",
                Some(alignment.range),
                Some(top1),
                alignment.confidence,
                alignment.confidence,
            )
        } else {
            stage_b_cases += 1;
            let head = transcript_match_head(&case.head_transcript, &chapters, &config);
            let tail = transcript_match_tail(
                &case.tail_transcript,
                &chapters,
                head.confident_id(),
                &config,
            );
            let top1 = head
                .top()
                .first()
                .zip(tail.top().first())
                .map(|(head, tail)| (head.chapter_id.clone(), tail.chapter_id.clone()));
            let auto_range = match (&head, &tail) {
                (
                    BoundaryResult::Confident {
                        chapter_id: start_chapter_id,
                        ..
                    },
                    BoundaryResult::Confident {
                        chapter_id: end_chapter_id,
                        ..
                    },
                ) => Some(DetectedRange {
                    start_chapter_id: start_chapter_id.clone(),
                    end_chapter_id: end_chapter_id.clone(),
                }),
                _ => None,
            };
            (
                "stage_b",
                auto_range,
                top1,
                head.top()
                    .first()
                    .map(|candidate| candidate.score)
                    .unwrap_or(0.0),
                tail.top()
                    .first()
                    .map(|candidate| candidate.score)
                    .unwrap_or(0.0),
            )
        };
        let expected = (
            &case.expected_start_chapter_id,
            &case.expected_end_chapter_id,
        );
        let top1_correct = top1_pair
            .as_ref()
            .is_some_and(|pair| (&pair.0, &pair.1) == expected);
        let auto_correct = auto_range.as_ref().is_some_and(|range| {
            case.valid_sample && (&range.start_chapter_id, &range.end_chapter_id) == expected
        });

        if case.valid_sample {
            valid_samples += 1;
            valid_top1 += usize::from(top1_correct);
        }
        if auto_range.is_some() {
            auto_accepted += 1;
            correct_auto_accepted += usize::from(auto_correct);
        }
        println!(
            "{} {} {path} auto_accepted={} auto_correct={auto_correct} top1_correct={top1_correct} head_score={head_score:.3} tail_score={tail_score:.3}",
            case.language,
            case.id,
            auto_range.is_some(),
        );
    }

    assert!(
        auto_accepted > 0,
        "corpus must auto-accept at least one case"
    );
    assert!(valid_samples > 0, "corpus must contain valid samples");
    assert!(
        stage_a_cases > 0 && stage_b_cases > 0,
        "corpus must exercise Stage A and Stage B"
    );
    CorpusMetrics {
        auto_accept_precision: correct_auto_accepted as f64 / auto_accepted as f64,
        valid_top1_inclusion: valid_top1 as f64 / valid_samples as f64,
    }
}

#[test]
fn defaults_meet_multilingual_calibration_floors() {
    let cases: Vec<CorpusCase> = serde_json::from_str(CORPUS).expect("parse calibration corpus");
    let mut coverage: HashMap<&str, (usize, usize)> = HashMap::new();
    for case in &cases {
        let counts = coverage.entry(&case.language).or_default();
        if case.valid_sample {
            counts.0 += 1;
        } else {
            counts.1 += 1;
        }
    }
    for language in ["en", "ja", "ru", "es", "de"] {
        let (valid, negative) = coverage.get(language).copied().unwrap_or_default();
        assert!(
            valid >= 2 && negative >= 1,
            "{language}: expected at least two valid and one negative case, got {valid}/{negative}"
        );
    }

    let metrics = evaluate_corpus(&cases);
    println!(
        "auto_accept_precision={:.3} >= 0.95 valid_top1_inclusion={:.3} >= 0.90",
        metrics.auto_accept_precision, metrics.valid_top1_inclusion
    );
    assert!(
        meets_auto_release_gate(&metrics),
        "auto release gate failed: auto_accept_precision {:.3} (>= 0.95), valid_top1_inclusion {:.3} (>= 0.90)",
        metrics.auto_accept_precision,
        metrics.valid_top1_inclusion
    );
}

#[test]
fn sub_threshold_metrics_cannot_pass_the_auto_release_gate() {
    for (label, metrics) in [
        (
            "precision below floor",
            CorpusMetrics {
                auto_accept_precision: 0.94,
                valid_top1_inclusion: 0.99,
            },
        ),
        (
            "top-1 inclusion below floor",
            CorpusMetrics {
                auto_accept_precision: 1.0,
                valid_top1_inclusion: 0.89,
            },
        ),
        (
            "both below floor",
            CorpusMetrics {
                auto_accept_precision: 0.5,
                valid_top1_inclusion: 0.5,
            },
        ),
    ] {
        assert!(
            !meets_auto_release_gate(&metrics),
            "{label} must not pass the auto release gate"
        );
    }

    assert!(
        meets_auto_release_gate(&CorpusMetrics {
            auto_accept_precision: 0.95,
            valid_top1_inclusion: 0.90,
        }),
        "the exact floors must pass"
    );
}

#[test]
fn manual_detection_stays_callable_below_the_auto_release_gate() {
    let cases: Vec<CorpusCase> = serde_json::from_str(CORPUS).expect("parse calibration corpus");
    let case = cases
        .iter()
        .find(|case| case.valid_sample)
        .expect("corpus contains a valid sample");
    let chapters = corpus_chapters(case);
    // Unreachable floors: auto accept becomes impossible, manual review must not.
    let config = AlignmentConfig {
        transcript_confidence: 2.0,
        title_confidence: 2.0,
        ..AlignmentConfig::default()
    };

    assert!(title_match(
        case.first_audio_title.as_deref(),
        case.last_audio_title.as_deref(),
        &chapters,
        &config,
    )
    .is_none());
    let head = transcript_match_head(&case.head_transcript, &chapters, &config);
    let tail = transcript_match_tail(
        &case.tail_transcript,
        &chapters,
        head.confident_id(),
        &config,
    );

    assert!(!matches!(head, BoundaryResult::Confident { .. }));
    assert!(!matches!(tail, BoundaryResult::Confident { .. }));
    assert!(
        !head.top().is_empty(),
        "manual review still needs head candidates"
    );
    assert!(
        !tail.top().is_empty(),
        "manual review still needs tail candidates"
    );
}
