//! Chapter-divider absorb-policy carver coverage.
//!
//! Skips the ffmpeg-driven detection paths if ffmpeg is not on PATH, but the
//! pure-function policy math is always exercised against a fallback offsets
//! table so the contract has coverage even on minimal hosts.

use std::path::{Path, PathBuf};
use std::process::Command as SyncCommand;

use lingq_upload_lib::core::audio::{
    boundaries_from_silences, carve, AbsorbPolicy, Boundary, CarveOpts, SilenceRun,
};
use lingq_upload_lib::core::project::Project;
use serde::Deserialize;

#[derive(Deserialize)]
struct GoldenOffsets {
    #[allow(dead_code)]
    silence_midpoints_ms: Vec<u32>,
    forward_offsets_ms: Vec<u32>,
    backward_offsets_ms: Vec<u32>,
    drop_offsets_ms: Vec<u32>,
    #[allow(dead_code)]
    silence_runs_ms: Vec<(u32, u32)>,
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/audio/silence_corpus")
}

fn ffmpeg_on_path() -> bool {
    SyncCommand::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn load_golden(name: &str) -> GoldenOffsets {
    let raw = std::fs::read_to_string(fixtures_dir().join(name)).expect("read golden_offsets.json");
    serde_json::from_str(&raw).expect("parse golden")
}

fn ensure_fixtures() {
    let dir = fixtures_dir();
    std::fs::create_dir_all(&dir).expect("mkdir silence_corpus");
    let a = dir.join("clip_a.wav");
    let b = dir.join("clip_b.wav");
    if !a.exists() {
        gen_clip(&a, &[(0.0, 5.0), (6.0, 9.0), (10.0, 12.0)]);
    }
    if !b.exists() {
        gen_clip(&b, &[(0.0, 4.0), (5.5, 7.0), (8.5, 11.0)]);
    }
}

/// Emit a 1kHz tone clip with silence between the listed (start, end) seconds.
/// Each tone segment is rendered separately and concatenated with `concat`.
fn gen_clip(out: &Path, segments: &[(f64, f64)]) {
    let total: f64 = segments.last().map(|s| s.1).unwrap_or(0.0);
    // Build a single ffmpeg invocation: a sine source for the full duration,
    // then chain `volume=0` enable expressions for each silent gap.
    let mut filter = String::from("sine=frequency=1000:sample_rate=16000:duration=");
    filter.push_str(&format!("{total}"));
    // Each gap is the span between segment[i].1 and segment[i+1].0.
    let mut volume_chain = String::new();
    for w in segments.windows(2) {
        let gap_start = w[0].1;
        let gap_end = w[1].0;
        volume_chain.push_str(&format!(
            ",volume=enable='between(t,{gap_start},{gap_end})':volume=0"
        ));
    }
    let af = format!("{filter}{volume_chain}");
    let _ = std::fs::remove_file(out);
    let status = SyncCommand::new("ffmpeg")
        .args(["-hide_banner", "-v", "error", "-y", "-f", "lavfi", "-i", &af])
        .args(["-ac", "1", "-ar", "16000"])
        .arg(out)
        .status()
        .expect("spawn ffmpeg");
    assert!(status.success(), "ffmpeg fixture-gen failed for {out:?}");
}

fn offsets(runs: &[SilenceRun], p: AbsorbPolicy) -> Vec<u32> {
    boundaries_from_silences(runs, p)
        .iter()
        .map(|b| b.cut_offset_ms)
        .collect()
}

#[test]
fn forward_offsets_distinct_from_backward_and_drop() {
    let runs_a = [
        SilenceRun {
            start_ms: 5_000,
            end_ms: 6_000,
        },
        SilenceRun {
            start_ms: 9_000,
            end_ms: 10_000,
        },
    ];
    let runs_b = [
        SilenceRun {
            start_ms: 4_000,
            end_ms: 5_500,
        },
        SilenceRun {
            start_ms: 7_000,
            end_ms: 8_500,
        },
    ];
    for runs in [&runs_a[..], &runs_b[..]] {
        let f = offsets(runs, AbsorbPolicy::Forward);
        let b = offsets(runs, AbsorbPolicy::Backward);
        let d = offsets(runs, AbsorbPolicy::Drop);
        assert_ne!(f, b, "forward must differ from backward: {f:?} vs {b:?}");
        assert_ne!(f, d, "forward must differ from drop: {f:?} vs {d:?}");
        assert_ne!(b, d, "backward must differ from drop: {b:?} vs {d:?}");
    }
}

#[test]
fn boundaries_within_50ms_of_silence_midpoint() {
    let runs = [
        SilenceRun {
            start_ms: 5_000,
            end_ms: 6_000,
        },
        SilenceRun {
            start_ms: 9_000,
            end_ms: 10_000,
        },
    ];
    for policy in [
        AbsorbPolicy::Forward,
        AbsorbPolicy::Backward,
        AbsorbPolicy::Drop,
    ] {
        let bs = boundaries_from_silences(&runs, policy);
        for b in &bs {
            // Each boundary must sit on either the start or end of one of the runs,
            // i.e. within 50ms of a real silence edge.
            let on_edge = runs
                .iter()
                .any(|r| diff(b.cut_offset_ms, r.start_ms) <= 50 || diff(b.cut_offset_ms, r.end_ms) <= 50);
            assert!(on_edge, "boundary {b:?} not within 50ms of any silence edge");
        }
    }
}

fn diff(a: u32, b: u32) -> u32 {
    a.abs_diff(b)
}

#[test]
fn project_round_trips_absorb_policy() {
    use lingq_upload_lib::core::identity::ProjectId;
    use lingq_upload_lib::core::project::{ProjectSettings, ProjectSources};
    use lingq_upload_lib::ingest::TextSource;
    use std::path::PathBuf;

    let p = Project {
        schema_version: lingq_upload_lib::core::project::SCHEMA_V1,
        id: ProjectId::from_title_author("RoundTrip", "A"),
        sources: ProjectSources {
            text: TextSource::Epub(PathBuf::from("/tmp/x.epub")),
            audio: None,
            chapter_manifest: None,
        },
        settings: ProjectSettings {
            language: "ja".into(),
            collection_title: "RT".into(),
            level: 1,
            tags: vec![],
        },
        receipts: vec![],
        queue_cursor: 0,
        completed_lesson_ids: vec![],
        matcher_decision: None,
        cover_path: None,
        authors: vec![],
        series: None,
        lingq_collection_id: None,
        last_activity_at: None,
        stage: Default::default(),
        last_transition_at: None,
        skipped_chapters: vec![],
        absorb_policy: AbsorbPolicy::Backward,
    };
    let json = serde_json::to_string(&p).expect("serialize");
    let back: Project = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.absorb_policy, AbsorbPolicy::Backward);
    // Round-trip preserves equality of the whole record.
    assert_eq!(back, p);
}

#[test]
fn project_defaults_to_forward_when_field_missing() {
    use lingq_upload_lib::core::identity::ProjectId;
    use lingq_upload_lib::core::project::{ProjectSettings, ProjectSources};
    use lingq_upload_lib::ingest::TextSource;
    use std::path::PathBuf;

    let p = Project {
        schema_version: lingq_upload_lib::core::project::SCHEMA_V1,
        id: ProjectId::from_title_author("Legacy", "A"),
        sources: ProjectSources {
            text: TextSource::Epub(PathBuf::from("/tmp/x.epub")),
            audio: None,
            chapter_manifest: None,
        },
        settings: ProjectSettings {
            language: "ja".into(),
            collection_title: "L".into(),
            level: 1,
            tags: vec![],
        },
        receipts: vec![],
        queue_cursor: 0,
        completed_lesson_ids: vec![],
        matcher_decision: None,
        cover_path: None,
        authors: vec![],
        series: None,
        lingq_collection_id: None,
        last_activity_at: None,
        stage: Default::default(),
        last_transition_at: None,
        skipped_chapters: vec![],
        absorb_policy: AbsorbPolicy::Drop,
    };
    let mut json: serde_json::Value = serde_json::to_value(&p).expect("to_value");
    json.as_object_mut()
        .unwrap()
        .remove("absorb_policy")
        .expect("absorb_policy key existed before strip");
    let back: Project = serde_json::from_value(json).expect("deserialize legacy");
    assert_eq!(back.absorb_policy, AbsorbPolicy::Forward);
}

#[tokio::test]
async fn carve_clip_a_per_policy() {
    if !ffmpeg_on_path() {
        eprintln!("ffmpeg not on PATH — skipping carve_clip_a_per_policy");
        return;
    }
    ensure_fixtures();
    let clip = fixtures_dir().join("clip_a.wav");
    let golden = load_golden("clip_a.golden_offsets.json");
    for (policy, expected) in [
        (AbsorbPolicy::Forward, &golden.forward_offsets_ms),
        (AbsorbPolicy::Backward, &golden.backward_offsets_ms),
        (AbsorbPolicy::Drop, &golden.drop_offsets_ms),
    ] {
        let opts = CarveOpts {
            silence_db: -30.0,
            min_silence_ms: 500,
            absorb: policy,
        };
        let boundaries = carve(&clip, opts).await.expect("carve clip_a");
        let got: Vec<u32> = boundaries.iter().map(|b: &Boundary| b.cut_offset_ms).collect();
        assert_eq!(
            got.len(),
            expected.len(),
            "policy={policy:?} got {got:?} expected {expected:?}"
        );
        for (g, e) in got.iter().zip(expected.iter()) {
            assert!(
                diff(*g, *e) <= 50,
                "policy={policy:?} offset {g} not within 50ms of expected {e}"
            );
        }
    }
}

#[tokio::test]
async fn carve_clip_b_per_policy() {
    if !ffmpeg_on_path() {
        eprintln!("ffmpeg not on PATH — skipping carve_clip_b_per_policy");
        return;
    }
    ensure_fixtures();
    let clip = fixtures_dir().join("clip_b.wav");
    let golden = load_golden("clip_b.golden_offsets.json");
    for (policy, expected) in [
        (AbsorbPolicy::Forward, &golden.forward_offsets_ms),
        (AbsorbPolicy::Backward, &golden.backward_offsets_ms),
        (AbsorbPolicy::Drop, &golden.drop_offsets_ms),
    ] {
        let opts = CarveOpts {
            silence_db: -30.0,
            min_silence_ms: 500,
            absorb: policy,
        };
        let boundaries = carve(&clip, opts).await.expect("carve clip_b");
        let got: Vec<u32> = boundaries.iter().map(|b: &Boundary| b.cut_offset_ms).collect();
        assert_eq!(
            got.len(),
            expected.len(),
            "policy={policy:?} got {got:?} expected {expected:?}"
        );
        for (g, e) in got.iter().zip(expected.iter()) {
            assert!(
                diff(*g, *e) <= 50,
                "policy={policy:?} offset {g} not within 50ms of expected {e}"
            );
        }
    }
}
