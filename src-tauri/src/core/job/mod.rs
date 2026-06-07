//! End-to-end project job orchestrator.
//!
//! Wires the building blocks parse -> matcher -> transcode -> import -> receipt
//! into one sequential pass. Persists project state after every chapter so a
//! crash or cancel leaves a resumable [`Project`] on disk.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use tokio_util::sync::CancellationToken;

use crate::core::audio::{self, AudioTrack, EncoderSettings};
use crate::core::epub::{parse_epub, Chapter, HeadingStrategy};
use crate::core::identity::ProjectId;
use crate::core::lesson::single_lesson_concat;
use crate::core::matcher::{auto_match, MatchOutcome, MismatchResponse};
use crate::core::project::{ChapterReceipt, MatcherDecision, Project};
use crate::core::store::ProjectStore;
use crate::core::text::read_text_for_upload;
use crate::error::AppError;
use crate::ingest::{AudioSource, TextSource};
use crate::lingq::{ImportLessonRequest, LessonStatus, LingqClient};

/// Sink that receives [`crate::events::JobEvent`]-equivalent notifications.
///
/// The orchestrator is decoupled from `tauri::AppHandle` so it can be unit
/// tested without booting tauri. Production wraps a `JobEmitter`; tests wrap
/// an in-memory recorder.
pub trait JobSink: Send {
    fn started(&mut self);
    fn progress(&mut self, pct: f32, message: Option<String>);
    fn chapter_done(&mut self, chapter_index: usize, lesson_id: i64, degraded: bool);
    fn cancelled(&mut self);
    fn result(&mut self, ok: bool, payload: serde_json::Value);
}

/// Run a project end to end: resolve audio/text, optionally pause for matcher
/// resolution, then for each chapter transcode + upload + persist a receipt.
///
/// The function returns `Ok(())` even on cancel — a cancelled run is a normal
/// outcome that emits `Cancelled` and leaves a partial set of receipts.
/// Hard failures (missing audio source, parse error, network error) return
/// `Err(AppError)` and emit `Result { ok: false }`.
pub async fn run_project_job(
    store: Arc<dyn ProjectStore>,
    client: Arc<LingqClient>,
    project_id: ProjectId,
    cancel: CancellationToken,
    sink: &mut dyn JobSink,
) -> Result<(), AppError> {
    sink.started();

    let mut project = match store
        .get(&project_id)
        .map_err(|e| AppError::Other(format!("store.get: {e}")))?
    {
        Some(p) => p,
        None => {
            let err = AppError::Other("project not found".into());
            sink.result(false, serde_json::json!({"error": err.to_string()}));
            return Err(err);
        }
    };

    let tracks = match resolve_audio_tracks(&project).await {
        Ok(t) => t,
        Err(e) => {
            sink.result(false, serde_json::json!({"error": e.to_string()}));
            return Err(e);
        }
    };
    let chapters = match resolve_chapters(&project.sources.text) {
        Ok(c) => c,
        Err(e) => {
            sink.result(false, serde_json::json!({"error": e.to_string()}));
            return Err(e);
        }
    };

    tracing::info!(
        project = %project.id.join_key(),
        chapters = chapters.len(),
        tracks = tracks.len(),
        "job: resolved inputs",
    );

    // Decide pairing. If decision is missing AND counts mismatch, ask the UI.
    let plan = match build_plan(&project, &chapters, &tracks) {
        PlanOrPause::Plan(p) => p,
        PlanOrPause::NeedsMatch {
            condition,
            options,
            preselect,
        } => {
            let payload = serde_json::json!({
                "needs_match": true,
                "condition": condition,
                "options": options,
                "preselect": preselect,
                "title": project.settings.collection_title,
                "chapters": chapters.len(),
                "tracks": tracks.len(),
            });
            sink.result(false, payload);
            return Ok(());
        }
        PlanOrPause::Cancelled => {
            let err = AppError::Other("matcher decision was Cancel".into());
            sink.result(false, serde_json::json!({"error": err.to_string()}));
            return Err(err);
        }
    };

    // Resolve the collection up front. Idempotent on the server side.
    let collection = match client
        .find_or_create_collection(&project.settings.collection_title, "", &project.settings.language)
        .await
    {
        Ok(id) => id,
        Err(e) => {
            let app = AppError::from(e);
            sink.result(false, serde_json::json!({"error": app.to_string()}));
            return Err(app);
        }
    };
    tracing::info!(collection = collection.0, "job: collection resolved");

    let staging = tempfile::tempdir()?;
    let enc = EncoderSettings::default();
    let total = plan.steps.len();

    for (step_pos, step) in plan.steps.iter().enumerate() {
        if cancel.is_cancelled() {
            tracing::info!(at = step_pos, "job: cancelled before chapter");
            persist_project(store.as_ref(), &project)?;
            sink.cancelled();
            return Ok(());
        }

        // Resume: skip chapters that already carry a lesson_id.
        if project
            .receipts
            .iter()
            .any(|r| r.chapter_index == step.chapter_index && r.lesson_id.is_some())
        {
            tracing::info!(
                chapter = step.chapter_index,
                "job: skipping previously uploaded chapter",
            );
            continue;
        }

        let track = &tracks[step.track_index];
        let dst = staging.path().join(format!("chapter_{:03}.mp3", step.chapter_index));
        let transcode_fut = audio::transcode(&track.path, &dst, &enc);
        let report = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                tracing::info!(at = step_pos, "job: cancelled during transcode");
                persist_project(store.as_ref(), &project)?;
                sink.cancelled();
                return Ok(());
            }
            r = transcode_fut => match r {
                Ok(rep) => rep,
                Err(e) => {
                    let app = AppError::from(e);
                    sink.result(false, serde_json::json!({"error": app.to_string()}));
                    return Err(app);
                }
            }
        };
        tracing::info!(
            chapter = step.chapter_index,
            delta = report.delta_sec,
            "job: transcoded chapter",
        );

        let title = step
            .title_override
            .clone()
            .unwrap_or_else(|| chapter_title(&chapters, step.chapter_index));
        let text = step.text_override.clone().unwrap_or_else(|| {
            chapters
                .iter()
                .find(|c| c.order == step.chapter_index)
                .map(|c| c.body.clone())
                .unwrap_or_default()
        });

        let req = ImportLessonRequest {
            collection,
            title: &title,
            text: &text,
            audio: Some(&dst),
            language: &project.settings.language,
            level: project.settings.level,
            status: LessonStatus::Private,
            tags: &[],
            save: true,
        };

        let lesson_id = match client.import_lesson_v2(req).await {
            Ok(id) => id,
            Err(e) => {
                let app = AppError::from(e);
                sink.result(false, serde_json::json!({"error": app.to_string()}));
                return Err(app);
            }
        };
        tracing::info!(chapter = step.chapter_index, lesson_id, "job: imported lesson");

        // Update receipts in place if one already exists for this chapter.
        let now = Utc::now();
        match project
            .receipts
            .iter_mut()
            .find(|r| r.chapter_index == step.chapter_index)
        {
            Some(existing) => {
                existing.track_index = Some(step.track_index);
                existing.lesson_id = Some(lesson_id);
                existing.degraded = step.degraded;
                existing.uploaded_at = Some(now);
            }
            None => project.receipts.push(ChapterReceipt {
                chapter_index: step.chapter_index,
                track_index: Some(step.track_index),
                lesson_id: Some(lesson_id),
                degraded: step.degraded,
                uploaded_at: Some(now),
            }),
        }
        project.queue_cursor = step.chapter_index + 1;
        project.completed_lesson_ids.push(lesson_id);
        persist_project(store.as_ref(), &project)?;

        sink.chapter_done(step.chapter_index, lesson_id, step.degraded);
        let pct = (step_pos as f32 + 1.0) / total.max(1) as f32;
        sink.progress(
            pct,
            Some(format!("Uploaded chapter {}", step.chapter_index + 1)),
        );
    }

    sink.result(
        true,
        serde_json::json!({"lesson_ids": project.completed_lesson_ids.clone()}),
    );
    Ok(())
}

fn persist_project(store: &dyn ProjectStore, project: &Project) -> Result<(), AppError> {
    store
        .put(project)
        .map_err(|e| AppError::Other(format!("store.put: {e}")))
}

fn chapter_title(chapters: &[Chapter], idx: usize) -> String {
    chapters
        .iter()
        .find(|c| c.order == idx)
        .map(|c| c.title.clone())
        .unwrap_or_else(|| format!("Chapter {}", idx + 1))
}

#[derive(Debug, Clone)]
struct Step {
    chapter_index: usize,
    track_index: usize,
    degraded: bool,
    /// When set, overrides the chapter body (used for `SingleLesson`).
    text_override: Option<String>,
    /// When set, overrides the chapter title (used for `SingleLesson`).
    title_override: Option<String>,
}

#[derive(Debug, Clone)]
struct Plan {
    steps: Vec<Step>,
}

enum PlanOrPause {
    Plan(Plan),
    NeedsMatch {
        condition: crate::core::matcher::MismatchCondition,
        options: Vec<MismatchResponse>,
        preselect: MismatchResponse,
    },
    Cancelled,
}

fn build_plan(project: &Project, chapters: &[Chapter], tracks: &[AudioTrack]) -> PlanOrPause {
    if let Some(decision) = &project.matcher_decision {
        return plan_from_decision(decision, chapters, tracks);
    }
    match auto_match(chapters, tracks) {
        MatchOutcome::Paired { pairs } => PlanOrPause::Plan(Plan {
            steps: pairs
                .into_iter()
                .map(|(c, t)| Step {
                    chapter_index: c,
                    track_index: t,
                    degraded: false,
                    text_override: None,
                    title_override: None,
                })
                .collect(),
        }),
        MatchOutcome::Mismatch {
            condition,
            options,
            preselect,
        } => PlanOrPause::NeedsMatch {
            condition,
            options,
            preselect,
        },
    }
}

fn plan_from_decision(
    decision: &MatcherDecision,
    chapters: &[Chapter],
    tracks: &[AudioTrack],
) -> PlanOrPause {
    use MismatchResponse::*;
    match decision.response {
        Cancel => PlanOrPause::Cancelled,
        SingleLesson => {
            // Concatenate all chapters into one lesson body, pair with the
            // first available track. Marked degraded so the UI can flag it.
            let text = single_lesson_concat(chapters);
            let title = chapters
                .first()
                .map(|c| c.title.clone())
                .unwrap_or_else(|| "Lesson".to_string());
            if tracks.is_empty() {
                return PlanOrPause::Cancelled;
            }
            PlanOrPause::Plan(Plan {
                steps: vec![Step {
                    chapter_index: 0,
                    track_index: 0,
                    degraded: true,
                    text_override: Some(text),
                    title_override: Some(title),
                }],
            })
        }
        PairAccept | PairDrop => {
            let n = chapters.len().min(tracks.len());
            if n != chapters.len() || n != tracks.len() {
                tracing::warn!(
                    chapters = chapters.len(),
                    tracks = tracks.len(),
                    paired = n,
                    "matcher decision pairs by min(); leftover side dropped",
                );
            }
            PlanOrPause::Plan(Plan {
                steps: (0..n)
                    .map(|i| Step {
                        chapter_index: i,
                        track_index: i,
                        degraded: false,
                        text_override: None,
                        title_override: None,
                    })
                    .collect(),
            })
        }
    }
}

fn resolve_chapters(text: &TextSource) -> Result<Vec<Chapter>, AppError> {
    match text {
        TextSource::Epub(p) => parse_epub(p, HeadingStrategy::Kindle).map_err(|e| {
            AppError::Other(format!("epub parse: {e}"))
        }),
        TextSource::LooseFiles { paths } => {
            let mut sorted: Vec<PathBuf> = paths.clone();
            sorted.sort();
            let mut out = Vec::with_capacity(sorted.len());
            for (i, p) in sorted.iter().enumerate() {
                let body = read_text_for_upload(p)?;
                let title = p
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Chapter")
                    .to_string();
                out.push(Chapter {
                    order: i,
                    title,
                    body,
                });
            }
            Ok(out)
        }
        TextSource::Missing => Err(AppError::Other("project has no text source".into())),
    }
}

async fn resolve_audio_tracks(project: &Project) -> Result<Vec<AudioTrack>, AppError> {
    let Some(source) = project.sources.audio.as_ref() else {
        return Err(AppError::Other("project has no audio source".into()));
    };
    match source {
        AudioSource::SingleFile(p) | AudioSource::LibationManifest(p) => {
            Ok(vec![track_for(p, 0).await])
        }
        AudioSource::Folder(dir) => {
            let mut paths = list_audio_in_dir(dir)?;
            paths.sort();
            let mut out = Vec::with_capacity(paths.len());
            for (i, p) in paths.into_iter().enumerate() {
                out.push(track_for(&p, i).await);
            }
            Ok(out)
        }
    }
}

async fn track_for(path: &Path, order: usize) -> AudioTrack {
    let duration_sec = audio::probe_duration(path).await.ok();
    AudioTrack {
        order,
        path: path.to_path_buf(),
        duration_sec,
        title: path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string()),
    }
}

fn list_audio_in_dir(dir: &Path) -> Result<Vec<PathBuf>, AppError> {
    let mut out = Vec::new();
    let entries = std::fs::read_dir(dir).map_err(AppError::from)?;
    for entry in entries.flatten() {
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let ext = p.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
        if matches!(ext.as_deref(), Some("m4b" | "m4a" | "mp3")) {
            out.push(p);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_audio_filters_extensions() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["a.mp3", "b.m4b", "c.m4a", "d.txt", "e.flac"] {
            std::fs::write(dir.path().join(name), b"x").unwrap();
        }
        let mut got = list_audio_in_dir(dir.path()).unwrap();
        got.sort();
        let names: Vec<_> = got
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["a.mp3", "b.m4b", "c.m4a"]);
    }
}
