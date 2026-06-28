use std::path::Path;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{run_ffprobe, AudioError};

/// Hard floor for the atom-duration filter. The effective threshold is
/// `FILTER_MIN_SECONDS.max(total_duration / atom_count / 10)`, so on real
/// audiobooks (≥ 6 atoms × ≥ 600 s) the dynamic term always wins and the
/// floor is a no-op. The floor only bites on very short fixtures where the
/// dynamic term would otherwise collapse — see `docs/specs/m4b-chapters.md`,
/// "Filter rules" — and catches sub-10 s branding atoms even there.
const FILTER_MIN_SECONDS: f64 = 6.0;

/// Tolerance for the contiguity check between the last atom end and
/// `format.duration`. Float drift up to 0.014 s observed in a 56,402 s file.
const COVERAGE_GAP_EPSILON_SEC: f64 = 0.05;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChapterAtom {
    pub start: f64,
    pub end: f64,
    pub title: Option<String>,
}

#[derive(Deserialize)]
struct FfprobeOut {
    chapters: Vec<RawChapter>,
    format: RawFormat,
}

#[derive(Deserialize)]
struct RawChapter {
    start_time: String,
    end_time: String,
    tags: Option<RawTags>,
}

#[derive(Deserialize)]
struct RawTags {
    title: Option<String>,
}

#[derive(Deserialize)]
struct RawFormat {
    duration: String,
}

pub async fn probe_chapters(path: &Path) -> Result<Vec<ChapterAtom>, AudioError> {
    let stdout = run_ffprobe(
        &[
            "-v",
            "error",
            "-show_chapters",
            "-show_format",
            "-print_format",
            "json",
        ],
        path,
    )
    .await?;

    let parsed: FfprobeOut = serde_json::from_slice(&stdout)
        .map_err(|e| AudioError::Probe(format!("ffprobe json: {e}")))?;

    let total_duration = parse_f64("format.duration", &parsed.format.duration)?;

    let raw: Vec<ChapterAtom> = parsed
        .chapters
        .into_iter()
        .map(|c| {
            Ok(ChapterAtom {
                start: parse_f64("chapter start_time", &c.start_time)?,
                end: parse_f64("chapter end_time", &c.end_time)?,
                title: c.tags.and_then(|t| t.title),
            })
        })
        .collect::<Result<_, AudioError>>()?;

    if raw.is_empty() {
        return Ok(Vec::new());
    }

    let atom_count = raw.len() as f64;
    let threshold = FILTER_MIN_SECONDS.max(total_duration / atom_count / 10.0);

    let filtered: Vec<ChapterAtom> = raw
        .into_iter()
        .filter(|a| (a.end - a.start) >= threshold)
        .collect();

    if let Some(last) = filtered.last() {
        let gap = (total_duration - last.end).abs();
        if gap > COVERAGE_GAP_EPSILON_SEC {
            // TODO: emit `AudioWarning::AtomCoverageGap` once the warning event
            // channel exists; for now this is informational only.
            tracing::warn!(
                path = %path.display(),
                gap_sec = gap,
                last_end = last.end,
                total_duration,
                "atom coverage gap exceeds epsilon"
            );
        }
    }

    Ok(filtered)
}

fn parse_f64(field: &str, s: &str) -> Result<f64, AudioError> {
    s.parse::<f64>()
        .map_err(|e| AudioError::Probe(format!("{field}: {e}")))
}

#[cfg(test)]
mod tests {
    use super::super::resolve_ffprobe_bin;
    use super::*;
    use std::path::PathBuf;
    use std::process::Stdio;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/audio")
            .join(name)
    }

    fn ffprobe_available() -> bool {
        let Ok(bin) = resolve_ffprobe_bin() else {
            return false;
        };
        std::process::Command::new(bin)
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    #[tokio::test]
    async fn probe_chapters_yields_three_atoms_for_generic_fixture() {
        if !ffprobe_available() {
            eprintln!("ffprobe not on PATH — skipping");
            return;
        }
        let atoms = probe_chapters(&fixture("synth_chapters_generic.m4b"))
            .await
            .expect("probe");
        assert_eq!(atoms.len(), 3, "atoms: {atoms:?}");
        let titles: Vec<_> = atoms
            .iter()
            .map(|a| a.title.as_deref().unwrap_or(""))
            .collect();
        assert_eq!(titles, vec!["Chapter 1", "Chapter 2", "Chapter 3"]);
        for a in &atoms {
            assert!(
                ((a.end - a.start) - 20.0).abs() < 0.05,
                "duration not ~20s: {a:?}"
            );
        }
    }

    #[tokio::test]
    async fn probe_chapters_yields_narrative_titles_for_narrative_fixture() {
        if !ffprobe_available() {
            eprintln!("ffprobe not on PATH — skipping");
            return;
        }
        let atoms = probe_chapters(&fixture("synth_chapters_narrative.m4b"))
            .await
            .expect("probe");
        assert_eq!(atoms.len(), 3);
        let titles: Vec<_> = atoms
            .iter()
            .map(|a| a.title.as_deref().unwrap_or(""))
            .collect();
        assert_eq!(titles, vec!["序章", "第一章", "第二章"]);
    }

    #[tokio::test]
    async fn probe_chapters_filters_tiny_intro_atom() {
        if !ffprobe_available() {
            eprintln!("ffprobe not on PATH — skipping");
            return;
        }
        let atoms = probe_chapters(&fixture("synth_chapters_intro.m4b"))
            .await
            .expect("probe");
        assert_eq!(atoms.len(), 2, "atoms: {atoms:?}");
        assert!((atoms[0].start - 5.0).abs() < 0.05);
        assert!((atoms[0].end - 60.0).abs() < 0.05);
        assert!((atoms[1].start - 60.0).abs() < 0.05);
        assert!((atoms[1].end - 120.0).abs() < 0.05);
    }

    fn ensure_silence_fixture(path: &std::path::Path) {
        if path.exists() {
            return;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create fixtures dir");
        }
        let status = std::process::Command::new("ffmpeg")
            .args([
                "-y",
                "-hide_banner",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "anullsrc=r=44100:cl=stereo",
                "-t",
                "5",
                "-c:a",
                "aac",
            ])
            .arg(path)
            .status()
            .expect("spawn ffmpeg");
        assert!(status.success(), "ffmpeg silence-fixture gen failed");
    }

    #[tokio::test]
    async fn probe_chapters_returns_empty_on_atomless_file() {
        if !ffprobe_available() {
            eprintln!("ffprobe not on PATH — skipping");
            return;
        }
        let path = fixture("silence.m4a");
        ensure_silence_fixture(&path);
        let atoms = probe_chapters(&path).await.expect("probe");
        assert!(atoms.is_empty(), "expected no atoms, got {atoms:?}");
    }

    #[tokio::test]
    async fn probe_chapters_errors_on_missing_file() {
        if !ffprobe_available() {
            eprintln!("ffprobe not on PATH — skipping");
            return;
        }
        let res = probe_chapters(Path::new("/definitely/not/a/real/path/missing.m4b")).await;
        assert!(
            matches!(res, Err(AudioError::FfmpegFailed { .. })),
            "got {res:?}"
        );
    }
}
