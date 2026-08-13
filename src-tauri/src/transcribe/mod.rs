use std::path::Path;

use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use specta::Type;
use tempfile::TempDir;
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::core::audio::{self, AudioError, AudioTrack, EncoderSettings, TranscodeReport};
use crate::core::epub::Chapter;
use crate::core::project::Project;
use crate::error::AppError;
use crate::events::DetectionPhase;

mod align;
mod error;
mod provider;
pub mod sample;
mod whisper_like;

pub use align::{
    normalize_for_alignment, title_match, transcript_match_head, transcript_match_tail,
    AlignSource, AlignmentMatch, BoundaryResult, ChapterCandidate, DetectedRange,
};
pub use error::{TranscribeError, TranscribeErrorKind};
pub use provider::{provider_language_hint, PricingHint, ProviderCatalog, ProviderDescriptor};
pub use sample::{
    AlignmentConfig, NoTranscriptReason, SamplePlan, SampleSide, SampleWindow, SideSamplePlan,
};
pub use whisper_like::WhisperLikeTranscriber;

#[derive(Clone, Debug, Error, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(tag = "kind", content = "message")]
pub enum DetectedRangeError {
    #[error("detected chapter boundary is missing: {0}")]
    MissingBoundary(String),
    #[error("detected chapter boundary is duplicated: {0}")]
    DuplicateBoundary(String),
    #[error("detected range end precedes start")]
    EndBeforeStart,
    #[error("detected range has no eligible chapters")]
    EmptyRange,
}

#[derive(Debug, Error)]
pub enum SamplePlanningError {
    #[error("no transcript: {0:?}")]
    NoTranscript(NoTranscriptReason),
    #[error(transparent)]
    Audio(#[from] AudioError),
    #[error(transparent)]
    Resolve(#[from] AppError),
}

pub async fn resolve_and_plan_sample_windows(
    project: &Project,
    config: &AlignmentConfig,
) -> Result<SamplePlan, SamplePlanningError> {
    let tracks = crate::core::job::resolve_audio_tracks(project).await?;
    probe_and_plan_sample_windows(&tracks, config).await
}

async fn probe_and_plan_sample_windows(
    tracks: &[AudioTrack],
    config: &AlignmentConfig,
) -> Result<SamplePlan, SamplePlanningError> {
    let Some(head_track) = tracks.first() else {
        return Err(SamplePlanningError::NoTranscript(NoTranscriptReason::Empty));
    };
    let tail_track = tracks.last().expect("non-empty tracks");
    let head_duration = audio::probe_duration(&head_track.path).await?;
    let tail_duration = if head_track.path == tail_track.path {
        head_duration
    } else {
        audio::probe_duration(&tail_track.path).await?
    };

    sample::plan_sample_windows(tracks, (head_duration, tail_duration), config)
        .map_err(SamplePlanningError::NoTranscript)
}

pub struct ExtractedSample {
    _temp_dir: TempDir,
    path: std::path::PathBuf,
    report: TranscodeReport,
}

impl ExtractedSample {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn report(&self) -> &TranscodeReport {
        &self.report
    }
}

pub async fn extract_sample(
    window: &SampleWindow,
    cancel: &CancellationToken,
) -> Result<ExtractedSample, AudioError> {
    if cancel.is_cancelled() {
        return Err(AudioError::Cancelled);
    }

    let temp_dir = tempfile::tempdir()?;
    let side = match window.side {
        SampleSide::Head => "head",
        SampleSide::Tail => "tail",
    };
    let path = temp_dir
        .path()
        .join(format!("{side}-{}.mp3", window.attempt));
    let encoder = EncoderSettings {
        bitrate: "64k".into(),
        sample_rate: 16_000,
        channels: 1,
    };
    let report = audio::transcode(
        &window.path,
        &path,
        &encoder,
        Some((window.start_sec, window.end_sec)),
    )
    .await?;

    // spawn_blocking codec work completes before this post-check and cleanup.
    if cancel.is_cancelled() {
        return Err(AudioError::Cancelled);
    }

    Ok(ExtractedSample {
        _temp_dir: temp_dir,
        path,
        report,
    })
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TranscribeProviderId {
    #[default]
    Groq,
    OpenAi,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq)]
pub struct TranscribeConsent {
    pub provider_id: TranscribeProviderId,
    pub accepted_at: DateTime<Utc>,
}

pub fn consent_matches(
    consent: Option<&TranscribeConsent>,
    active_provider: TranscribeProviderId,
) -> bool {
    consent.is_some_and(|consent| consent.provider_id == active_provider)
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, Type)]
pub struct TranscribeOpts {
    pub language: Option<String>,
    pub prompt: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type)]
pub struct Transcript {
    pub text: String,
}

pub trait Transcriber: Send + Sync {
    fn provider_id(&self) -> TranscribeProviderId;
    fn transcribe(
        &self,
        audio: &Path,
        opts: &TranscribeOpts,
    ) -> BoxFuture<'_, Result<Transcript, TranscribeError>>;
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq)]
pub struct DetectionEvidence {
    pub provider_id: Option<TranscribeProviderId>,
    pub align_source: AlignSource,
    pub range: DetectedRange,
    pub confidence: f32,
    pub transcript_head_preview: Option<String>,
    pub transcript_tail_preview: Option<String>,
    pub detected_at: DateTime<Utc>,
}

pub fn detection_provider_matches_source(
    align_source: AlignSource,
    provider_id: Option<TranscribeProviderId>,
) -> bool {
    matches!(
        (align_source, provider_id),
        (AlignSource::Title, None) | (AlignSource::Transcript, Some(_))
    )
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq)]
pub struct DetectionPreview {
    pub provider_id: Option<TranscribeProviderId>,
    pub align_source: AlignSource,
    pub range: DetectedRange,
    pub confidence: f32,
    pub transcript_head_preview: Option<String>,
    pub transcript_tail_preview: Option<String>,
    pub detected_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DetectStartResult {
    Detected {
        preview: DetectionPreview,
    },
    LowConfidence {
        transcript_head_preview: Option<String>,
        transcript_tail_preview: Option<String>,
        top_head: Vec<ChapterCandidate>,
        top_tail: Vec<ChapterCandidate>,
    },
    NoTranscript {
        reason: NoTranscriptReason,
    },
}

pub trait DetectionSink: Send {
    fn started(&mut self, job_id: Uuid);
    fn progress(&mut self, job_id: Uuid, pct: f32, phase: DetectionPhase);
    fn result(&mut self, job_id: Uuid, result: &DetectStartResult);
    fn error(&mut self, job_id: Uuid, error: &TranscribeError);
    fn cancelled(&mut self, job_id: Uuid);
}

pub type ProviderFactory<'a> =
    Box<dyn FnOnce() -> Result<Box<dyn Transcriber>, TranscribeError> + Send + 'a>;

enum DetectFailure {
    Cancelled,
    Operational(TranscribeError),
    Content(NoTranscriptReason),
}

struct DetectionLifecycle<'a> {
    job_id: Uuid,
    sink: &'a mut dyn DetectionSink,
    terminal: bool,
}

impl<'a> DetectionLifecycle<'a> {
    fn start(job_id: Uuid, sink: &'a mut dyn DetectionSink) -> Self {
        sink.started(job_id);
        Self {
            job_id,
            sink,
            terminal: false,
        }
    }

    fn progress(&mut self, pct: f32, phase: DetectionPhase) {
        debug_assert!(!self.terminal);
        self.sink.progress(self.job_id, pct, phase);
    }

    fn result(&mut self, result: &DetectStartResult) {
        self.finish();
        self.sink.result(self.job_id, result);
    }

    fn error(&mut self, error: &TranscribeError) {
        self.finish();
        self.sink.error(self.job_id, error);
    }

    fn cancelled(&mut self) {
        self.finish();
        self.sink.cancelled(self.job_id);
    }

    fn finish(&mut self) {
        debug_assert!(!self.terminal, "duplicate detection terminal");
        self.terminal = true;
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn detect_start_offset(
    project: &Project,
    tracks: &[AudioTrack],
    chapters: &[Chapter],
    job_id: Uuid,
    config: &AlignmentConfig,
    sink: &mut dyn DetectionSink,
    cancel: CancellationToken,
    provider_factory: ProviderFactory<'_>,
) -> Result<DetectStartResult, TranscribeError> {
    let mut lifecycle = DetectionLifecycle::start(job_id, sink);
    match detect_start_offset_inner(
        project,
        tracks,
        chapters,
        config,
        &mut lifecycle,
        &cancel,
        provider_factory,
    )
    .await
    {
        Ok(result) => {
            lifecycle.result(&result);
            Ok(result)
        }
        Err(DetectFailure::Operational(error)) => {
            lifecycle.error(&error);
            Err(error)
        }
        Err(DetectFailure::Cancelled) => {
            lifecycle.cancelled();
            Err(TranscribeError::new(
                TranscribeErrorKind::Audio,
                "detection cancelled",
            ))
        }
        Err(DetectFailure::Content(reason)) => {
            let result = DetectStartResult::NoTranscript { reason };
            lifecycle.result(&result);
            Ok(result)
        }
    }
}

async fn detect_start_offset_inner(
    project: &Project,
    tracks: &[AudioTrack],
    chapters: &[Chapter],
    config: &AlignmentConfig,
    lifecycle: &mut DetectionLifecycle<'_>,
    cancel: &CancellationToken,
    provider_factory: ProviderFactory<'_>,
) -> Result<DetectStartResult, DetectFailure> {
    lifecycle.progress(0.05, DetectionPhase::TitleCheck);
    ensure_not_cancelled(cancel)?;
    if let Some(alignment) = title_match(
        tracks.first().and_then(|track| track.title.as_deref()),
        tracks.last().and_then(|track| track.title.as_deref()),
        chapters,
        config,
    ) {
        return Ok(DetectStartResult::Detected {
            preview: DetectionPreview {
                provider_id: None,
                align_source: alignment.source,
                range: alignment.range,
                confidence: alignment.confidence,
                transcript_head_preview: None,
                transcript_tail_preview: None,
                detected_at: Utc::now(),
            },
        });
    }

    lifecycle.progress(0.15, DetectionPhase::SampleHead);
    ensure_not_cancelled(cancel)?;
    let plan = match probe_and_plan_sample_windows(tracks, config).await {
        Ok(plan) => plan,
        Err(SamplePlanningError::NoTranscript(reason)) => {
            return Ok(DetectStartResult::NoTranscript { reason })
        }
        Err(SamplePlanningError::Audio(error)) => return Err(audio_failure(error)),
        Err(SamplePlanningError::Resolve(error)) => {
            return Err(DetectFailure::Operational(TranscribeError::new(
                TranscribeErrorKind::Audio,
                error.to_string(),
            )))
        }
    };
    ensure_not_cancelled(cancel)?;

    let head_sample = extract_sample(&plan.head.initial, cancel)
        .await
        .map_err(audio_failure)?;
    lifecycle.progress(0.30, DetectionPhase::TranscribeHead);
    ensure_not_cancelled(cancel)?;
    let transcriber = provider_factory().map_err(DetectFailure::Operational)?;
    let provider_id = transcriber.provider_id();
    let language = ProviderCatalog::built_in()
        .descriptor(provider_id)
        .ok()
        .and_then(|descriptor| provider_language_hint(&project.settings.language, descriptor));
    let opts = TranscribeOpts {
        language,
        prompt: None,
    };
    let head_text = transcribe_side(
        transcriber.as_ref(),
        head_sample,
        plan.head.retry.as_ref(),
        &opts,
        cancel,
    )
    .await?;

    lifecycle.progress(0.45, DetectionPhase::AlignHead);
    ensure_not_cancelled(cancel)?;
    let head = transcript_match_head(&head_text, chapters, config);

    lifecycle.progress(0.55, DetectionPhase::SampleTail);
    ensure_not_cancelled(cancel)?;
    let tail_sample = extract_sample(&plan.tail.initial, cancel)
        .await
        .map_err(audio_failure)?;
    lifecycle.progress(0.70, DetectionPhase::TranscribeTail);
    ensure_not_cancelled(cancel)?;
    let tail_text = transcribe_side(
        transcriber.as_ref(),
        tail_sample,
        plan.tail.retry.as_ref(),
        &opts,
        cancel,
    )
    .await?;

    lifecycle.progress(0.90, DetectionPhase::AlignTail);
    ensure_not_cancelled(cancel)?;
    let tail = transcript_match_tail(&tail_text, chapters, head.confident_id(), config);

    match (&head, &tail) {
        (
            BoundaryResult::Confident {
                chapter_id: start_chapter_id,
                score: head_score,
                ..
            },
            BoundaryResult::Confident {
                chapter_id: end_chapter_id,
                score: tail_score,
                ..
            },
        ) if start_chapter_id != end_chapter_id || chapters.len() <= 1 => {
            Ok(DetectStartResult::Detected {
                preview: DetectionPreview {
                    provider_id: Some(provider_id),
                    align_source: AlignSource::Transcript,
                    range: DetectedRange {
                        start_chapter_id: start_chapter_id.clone(),
                        end_chapter_id: end_chapter_id.clone(),
                    },
                    confidence: (head_score + tail_score) * 0.5,
                    transcript_head_preview: bound_preview(Some(&head_text)),
                    transcript_tail_preview: bound_preview(Some(&tail_text)),
                    detected_at: Utc::now(),
                },
            })
        }
        (BoundaryResult::ContentPoor, _) | (_, BoundaryResult::ContentPoor) => {
            Err(DetectFailure::Content(NoTranscriptReason::ContentPoor))
        }
        _ => Ok(DetectStartResult::LowConfidence {
            transcript_head_preview: bound_preview(Some(&head_text)),
            transcript_tail_preview: bound_preview(Some(&tail_text)),
            top_head: head.top().to_vec(),
            top_tail: tail.top().to_vec(),
        }),
    }
}

async fn transcribe_side(
    transcriber: &dyn Transcriber,
    initial: ExtractedSample,
    retry: Option<&SampleWindow>,
    opts: &TranscribeOpts,
    cancel: &CancellationToken,
) -> Result<String, DetectFailure> {
    let text = transcribe_sample(transcriber, initial, opts, cancel).await?;
    let Err(reason) = usable_transcript(&text) else {
        return Ok(text);
    };
    let Some(retry) = retry else {
        return Err(DetectFailure::Content(reason));
    };

    ensure_not_cancelled(cancel)?;
    let sample = extract_sample(retry, cancel).await.map_err(audio_failure)?;
    ensure_not_cancelled(cancel)?;
    let text = transcribe_sample(transcriber, sample, opts, cancel).await?;
    usable_transcript(&text).map_err(DetectFailure::Content)?;
    Ok(text)
}

async fn transcribe_sample(
    transcriber: &dyn Transcriber,
    sample: ExtractedSample,
    opts: &TranscribeOpts,
    cancel: &CancellationToken,
) -> Result<String, DetectFailure> {
    ensure_not_cancelled(cancel)?;
    let response = transcriber.transcribe(sample.path(), opts).await;
    drop(sample);
    ensure_not_cancelled(cancel)?;
    response
        .map(|transcript| transcript.text)
        .map_err(DetectFailure::Operational)
}

fn usable_transcript(text: &str) -> Result<(), NoTranscriptReason> {
    let normalized = normalize_for_alignment(text);
    if normalized.is_empty() {
        Err(NoTranscriptReason::Empty)
    } else if normalized.chars().count() < 20 {
        Err(NoTranscriptReason::ContentPoor)
    } else {
        Ok(())
    }
}

fn ensure_not_cancelled(cancel: &CancellationToken) -> Result<(), DetectFailure> {
    if cancel.is_cancelled() {
        Err(DetectFailure::Cancelled)
    } else {
        Ok(())
    }
}

fn audio_failure(error: AudioError) -> DetectFailure {
    match error {
        AudioError::Cancelled => DetectFailure::Cancelled,
        error => DetectFailure::Operational(TranscribeError::new(
            TranscribeErrorKind::Audio,
            error.to_string(),
        )),
    }
}

pub(crate) fn bound_preview(transcript: Option<&str>) -> Option<String> {
    transcript.map(|text| text.chars().take(240).collect())
}

#[cfg(test)]
mod evidence_tests {
    use super::*;
    use crate::core::epub::ChapterId;

    #[test]
    fn detection_provider_must_match_the_alignment_source() {
        assert!(detection_provider_matches_source(AlignSource::Title, None));
        assert!(detection_provider_matches_source(
            AlignSource::Transcript,
            Some(TranscribeProviderId::Groq)
        ));
        assert!(!detection_provider_matches_source(
            AlignSource::Title,
            Some(TranscribeProviderId::Groq)
        ));
        assert!(!detection_provider_matches_source(
            AlignSource::Transcript,
            None
        ));
    }

    #[test]
    fn persisted_evidence_contains_only_bounded_previews() {
        let preview = bound_preview(Some(&"界".repeat(241)));
        let evidence = DetectionEvidence {
            provider_id: Some(TranscribeProviderId::OpenAi),
            align_source: AlignSource::Transcript,
            range: DetectedRange {
                start_chapter_id: ChapterId("start".into()),
                end_chapter_id: ChapterId("end".into()),
            },
            confidence: 0.9,
            transcript_head_preview: preview,
            transcript_tail_preview: None,
            detected_at: Utc::now(),
        };

        assert_eq!(
            evidence
                .transcript_head_preview
                .as_deref()
                .unwrap()
                .chars()
                .count(),
            240
        );
        let value = serde_json::to_value(evidence).unwrap();
        assert!(value.get("transcript").is_none());
        assert!(value.get("head_sample").is_none());
        assert!(value.get("tail_sample").is_none());
    }
}
