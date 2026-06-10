//! Chapter-divider absorb-policy carver coverage.
//!
//! The pure-function policy math is always exercised. The ffmpeg-driven
//! detection paths are gated behind `LINGQ_E2E_AUDIO=0` so devs without
//! ffmpeg on PATH can opt out; CI sets nothing and the tests run by default.
//!
//! The committed `clip_*.wav` fixtures are immutable inputs — pinned by
//! sha256. Regenerate all three via `scripts/fixtures/gen_silence_corpus.sh`
//! (clip_c ends mid-silence to exercise the EOF silence_end synthesis), then
//! update the sha256 constants below + the matching
//! `clip_*.golden_offsets.json` (record `ffmpeg -version` first line as
//! `ffmpeg_version`).

use std::path::PathBuf;
use std::process::Command as SyncCommand;

use lingq_upload_lib::core::audio::{
    boundaries_from_silences, carve, AbsorbPolicy, Boundary, BoundaryKind, CarveOpts, SilenceRun,
};
use lingq_upload_lib::core::project::Project;
use lingq_upload_lib::core::store::{InMemoryProjectStore, ProjectStore};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const CLIP_A_SHA256: &str = "88b14ee03f3742018ea492389f151eaa41c40cdf63389af3929a6a0a9f8d5585";
const CLIP_B_SHA256: &str = "52fb99df6b4327279f086f2ec26fb9f43639b84c483a7246d7195e42c62b6344";
const CLIP_C_SHA256: &str = "e98d7f7fb4fde62faf2cba8206180beccdcec75633d7217d7bea17d8cdbce11e";

// Golden contract is detected silence EDGES per absorb policy.
// `ffmpeg_version` in the JSON is informational only.
#[derive(Deserialize)]
struct GoldenOffsets {
    forward_offsets_ms: Vec<u32>,
    backward_offsets_ms: Vec<u32>,
    drop_offsets_ms: Vec<u32>,
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

fn assert_fixture(name: &str, expected_sha: &str) -> PathBuf {
    let path = fixtures_dir().join(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|_| {
        panic!(
            "missing fixture {}; run scripts/fixtures/gen_silence_corpus.sh to regenerate",
            path.display()
        )
    });
    let got = format!("{:x}", Sha256::digest(&bytes));
    assert_eq!(
        got, expected_sha,
        "fixture sha256 drift for {}: expected {expected_sha}, got {got}",
        path.display()
    );
    path
}

fn offsets(runs: &[SilenceRun], p: AbsorbPolicy) -> Vec<u32> {
    boundaries_from_silences(runs, p)
        .iter()
        .map(|b| b.cut_offset_ms)
        .collect()
}

fn diff(a: u32, b: u32) -> u32 {
    a.abs_diff(b)
}

#[test]
fn forward_offsets_distinct_from_backward_and_drop() {
    let runs_a = [
        SilenceRun { start_ms: 5_000, end_ms: 6_000 },
        SilenceRun { start_ms: 9_000, end_ms: 10_000 },
    ];
    let runs_b = [
        SilenceRun { start_ms: 4_000, end_ms: 5_500 },
        SilenceRun { start_ms: 7_000, end_ms: 8_500 },
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
fn boundaries_within_50ms_of_silence_edge() {
    let runs = [
        SilenceRun { start_ms: 5_000, end_ms: 6_000 },
        SilenceRun { start_ms: 9_000, end_ms: 10_000 },
    ];
    for policy in [AbsorbPolicy::Forward, AbsorbPolicy::Backward, AbsorbPolicy::Drop] {
        let bs = boundaries_from_silences(&runs, policy);
        for b in &bs {
            let on_edge = runs.iter().any(|r| {
                diff(b.cut_offset_ms, r.start_ms) <= 50 || diff(b.cut_offset_ms, r.end_ms) <= 50
            });
            assert!(on_edge, "boundary {b:?} not within 50ms of any silence edge");
        }
    }
}

#[test]
fn drop_kinds_pair_with_shared_track_index() {
    let runs = [SilenceRun { start_ms: 5_000, end_ms: 6_000 }];
    let bs = boundaries_from_silences(&runs, AbsorbPolicy::Drop);
    assert_eq!(bs.len(), 2);
    assert_eq!(bs[0].kind, BoundaryKind::DropStart);
    assert_eq!(bs[1].kind, BoundaryKind::DropEnd);
    assert_eq!(bs[0].track_index, bs[1].track_index);
}

#[test]
fn forward_backward_emit_cut_kind() {
    let runs = [SilenceRun { start_ms: 5_000, end_ms: 6_000 }];
    for policy in [AbsorbPolicy::Forward, AbsorbPolicy::Backward] {
        let bs = boundaries_from_silences(&runs, policy);
        assert!(bs.iter().all(|b| b.kind == BoundaryKind::Cut), "policy={policy:?}");
    }
}

#[test]
fn project_round_trips_absorb_policy() {
    use lingq_upload_lib::core::identity::ProjectId;

    let mut p = Project::new_test(ProjectId::from_title_author("RoundTrip", "A"), "RT");
    p.absorb_policy = AbsorbPolicy::Backward;
    let json = serde_json::to_string(&p).expect("serialize");
    let back: Project = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.absorb_policy, AbsorbPolicy::Backward);
    assert_eq!(back, p);
}

#[test]
fn project_defaults_to_forward_when_field_missing() {
    use lingq_upload_lib::core::identity::ProjectId;

    let mut p = Project::new_test(ProjectId::from_title_author("Legacy", "A"), "L");
    p.absorb_policy = AbsorbPolicy::Drop;
    let mut json: serde_json::Value = serde_json::to_value(&p).expect("to_value");
    json.as_object_mut()
        .unwrap()
        .remove("absorb_policy")
        .expect("absorb_policy key existed before strip");
    let back: Project = serde_json::from_value(json).expect("deserialize legacy");
    assert_eq!(back.absorb_policy, AbsorbPolicy::Forward);
}

#[test]
fn store_round_trips_absorb_policy_via_put_get() {
    use lingq_upload_lib::core::identity::ProjectId;

    let store = InMemoryProjectStore::new();
    let id = ProjectId::from_title_author("StoreRT", "A");
    let mut p = Project::new_test(id.clone(), "SR");
    p.absorb_policy = AbsorbPolicy::Drop;
    store.put(&p).expect("put");

    let mut reloaded = store.get(&id).expect("get").expect("present");
    assert_eq!(reloaded.absorb_policy, AbsorbPolicy::Drop);

    reloaded.absorb_policy = AbsorbPolicy::Backward;
    store.put(&reloaded).expect("put backward");
    let again = store.get(&id).expect("get").expect("present");
    assert_eq!(again.absorb_policy, AbsorbPolicy::Backward);
}

async fn assert_clip_matches_golden(stem: &str, sha256: &str) {
    if std::env::var("LINGQ_E2E_AUDIO").as_deref() == Ok("0") {
        return;
    }
    assert!(
        ffmpeg_on_path(),
        "ffmpeg required for carve_{stem}_per_policy; set LINGQ_E2E_AUDIO=0 to skip"
    );
    let clip = assert_fixture(&format!("{stem}.wav"), sha256);
    let golden = load_golden(&format!("{stem}.golden_offsets.json"));
    for (policy, expected) in [
        (AbsorbPolicy::Forward, &golden.forward_offsets_ms),
        (AbsorbPolicy::Backward, &golden.backward_offsets_ms),
        (AbsorbPolicy::Drop, &golden.drop_offsets_ms),
    ] {
        let opts = CarveOpts { absorb: policy, ..Default::default() };
        let boundaries = carve(&clip, opts).await.unwrap_or_else(|e| panic!("carve {stem}: {e}"));
        let got: Vec<u32> = boundaries.iter().map(|b: &Boundary| b.cut_offset_ms).collect();
        assert_eq!(got.len(), expected.len(), "policy={policy:?} got {got:?} expected {expected:?}");
        for (g, e) in got.iter().zip(expected.iter()) {
            assert!(diff(*g, *e) <= 50, "policy={policy:?} offset {g} not within 50ms of expected {e}");
        }
    }
}

#[tokio::test]
#[ignore = "ffmpeg-backed; runs by default in CI, opt out with LINGQ_E2E_AUDIO=0"]
async fn carve_clip_a_per_policy() {
    assert_clip_matches_golden("clip_a", CLIP_A_SHA256).await;
}

#[tokio::test]
#[ignore = "ffmpeg-backed; runs by default in CI, opt out with LINGQ_E2E_AUDIO=0"]
async fn carve_clip_b_per_policy() {
    assert_clip_matches_golden("clip_b", CLIP_B_SHA256).await;
}

// clip_c ends mid-silence: the final boundary only exists if a silence_end is
// synthesized at stream duration when silencedetect leaves the run open at EOF.
#[tokio::test]
#[ignore = "ffmpeg-backed; runs by default in CI, opt out with LINGQ_E2E_AUDIO=0"]
async fn carve_clip_c_tail_silence_keeps_final_boundary() {
    assert_clip_matches_golden("clip_c", CLIP_C_SHA256).await;
    if std::env::var("LINGQ_E2E_AUDIO").as_deref() == Ok("0") || !ffmpeg_on_path() {
        return;
    }
    let clip = assert_fixture("clip_c.wav", CLIP_C_SHA256);
    let golden = load_golden("clip_c.golden_offsets.json");
    for (policy, expected_last) in [
        (AbsorbPolicy::Backward, *golden.backward_offsets_ms.last().unwrap()),
        (AbsorbPolicy::Drop, *golden.drop_offsets_ms.last().unwrap()),
    ] {
        let opts = CarveOpts { absorb: policy, ..Default::default() };
        let boundaries = carve(&clip, opts).await.expect("carve clip_c");
        let last = boundaries.last().unwrap_or_else(|| {
            panic!("policy={policy:?}: tail-silence boundary missing")
        });
        assert!(
            diff(last.cut_offset_ms, expected_last) <= 50,
            "policy={policy:?} final boundary {} not within 50ms of {expected_last}",
            last.cut_offset_ms
        );
    }
}
