use std::path::{Path, PathBuf};

use lingq_upload_lib::core::audio::ChapterAtom;
use lingq_upload_lib::core::epub::ChapterId;
use lingq_upload_lib::core::identity::ProjectId;
use lingq_upload_lib::core::job::seed_bounded_mapping;
use lingq_upload_lib::core::matcher::proportional_pack;
use lingq_upload_lib::core::project::Project;
use lingq_upload_lib::error::AppError;
use lingq_upload_lib::ingest::{AudioSource, TextSource};
use lingq_upload_lib::transcribe::{DetectedRange, DetectedRangeError};
use tempfile::TempDir;

fn cid(order: usize) -> ChapterId {
    ChapterId::from_order(order)
}

struct ProjectFixture {
    _dir: TempDir,
    project: Project,
    audio_paths: Vec<PathBuf>,
}

impl ProjectFixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let audio_paths = vec![
            make_silent_wav(dir.path(), "a.wav"),
            make_silent_wav(dir.path(), "b.wav"),
        ];
        let text_paths: Vec<PathBuf> = (0..6)
            .map(|order| {
                let path = dir.path().join(format!("{order:02}.txt"));
                std::fs::write(&path, "x".repeat((order + 1) * 10)).unwrap();
                path
            })
            .collect();
        let mut project =
            Project::new_test(ProjectId::from_title_author("Bounded", "Author"), "Bounded");
        project.sources.text = TextSource::LooseFiles { paths: text_paths };
        project.sources.audio = Some(AudioSource::Folder(dir.path().to_path_buf()));
        Self {
            _dir: dir,
            project,
            audio_paths,
        }
    }
}

fn make_silent_wav(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 8_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&path, spec).unwrap();
    for _ in 0..8_000 {
        writer.write_sample(0_i16).unwrap();
    }
    writer.finalize().unwrap();
    path
}

fn range(start: usize, end: usize) -> DetectedRange {
    DetectedRange {
        start_chapter_id: cid(start),
        end_chapter_id: cid(end),
    }
}

async fn assert_range_error(project: &Project, range: DetectedRange, expected: DetectedRangeError) {
    let before = serde_json::to_vec(project).unwrap();
    let error = seed_bounded_mapping(project, &range).await.unwrap_err();
    match error {
        AppError::DetectedRange(actual) => assert_eq!(actual, expected),
        other => panic!("expected detected-range error, got {other:?}"),
    }
    assert_eq!(
        serde_json::to_vec(project).unwrap(),
        before,
        "range validation must not mutate the project"
    );
}

#[tokio::test]
async fn bounded_mapping_uses_inclusive_stable_id_slice() {
    let fixture = ProjectFixture::new();
    let before = serde_json::to_vec(&fixture.project).unwrap();

    let mapping = seed_bounded_mapping(&fixture.project, &range(2, 5))
        .await
        .unwrap();

    assert_eq!(
        mapping
            .pairs
            .iter()
            .map(|pair| pair.chapter_id.clone())
            .collect::<Vec<_>>(),
        vec![cid(2), cid(3), cid(4), cid(5)]
    );
    assert_eq!(
        mapping
            .buckets
            .iter()
            .map(|bucket| PathBuf::from(&bucket.audio_path))
            .collect::<Vec<_>>(),
        fixture.audio_paths
    );
    assert_eq!(serde_json::to_vec(&fixture.project).unwrap(), before);
}

#[tokio::test]
async fn bounded_mapping_accepts_a_single_chapter_range() {
    let fixture = ProjectFixture::new();

    let mapping = seed_bounded_mapping(&fixture.project, &range(4, 4))
        .await
        .unwrap();

    assert_eq!(mapping.pairs.len(), 1);
    assert_eq!(mapping.pairs[0].chapter_id, cid(4));
}

#[tokio::test]
async fn bounded_mapping_rejects_a_missing_start_id() {
    let fixture = ProjectFixture::new();
    assert_range_error(
        &fixture.project,
        DetectedRange {
            start_chapter_id: ChapterId("missing-start".into()),
            end_chapter_id: cid(5),
        },
        DetectedRangeError::MissingBoundary("missing-start".into()),
    )
    .await;
}

#[tokio::test]
async fn bounded_mapping_rejects_a_missing_end_id() {
    let fixture = ProjectFixture::new();
    assert_range_error(
        &fixture.project,
        DetectedRange {
            start_chapter_id: cid(2),
            end_chapter_id: ChapterId("missing-end".into()),
        },
        DetectedRangeError::MissingBoundary("missing-end".into()),
    )
    .await;
}

#[tokio::test]
async fn bounded_mapping_rejects_a_filtered_boundary_id() {
    let mut fixture = ProjectFixture::new();
    fixture.project.skipped_chapters.push(cid(2));
    assert_range_error(
        &fixture.project,
        range(2, 5),
        DetectedRangeError::MissingBoundary(cid(2).to_string()),
    )
    .await;
}

#[tokio::test]
async fn bounded_mapping_rejects_an_empty_eligible_range() {
    let mut fixture = ProjectFixture::new();
    fixture.project.skipped_chapters = (0..6).map(cid).collect();
    assert_range_error(
        &fixture.project,
        range(0, 5),
        DetectedRangeError::EmptyRange,
    )
    .await;
}

#[tokio::test]
async fn bounded_mapping_rejects_end_before_start() {
    let fixture = ProjectFixture::new();
    assert_range_error(
        &fixture.project,
        range(5, 2),
        DetectedRangeError::EndBeforeStart,
    )
    .await;
}

#[test]
fn proportional_packer_keeps_its_existing_two_argument_result() {
    let atoms = vec![
        ChapterAtom {
            start: 0.0,
            end: 10.0,
            title: Some("first".into()),
        },
        ChapterAtom {
            start: 10.0,
            end: 20.0,
            title: Some("second".into()),
        },
    ];

    let buckets = proportional_pack(&atoms, &[10, 10, 10, 10]);

    assert_eq!(buckets.len(), 2);
    assert_eq!(buckets[0].audio, atoms[0]);
    assert_eq!(buckets[0].text_range, 0..2);
    assert_eq!(buckets[1].audio, atoms[1]);
    assert_eq!(buckets[1].text_range, 2..4);
}
