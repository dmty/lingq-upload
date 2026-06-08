//! End-to-end project job orchestrator.
//!
//! Wires the building blocks parse -> matcher -> transcode -> import -> receipt
//! into one sequential pass. Persists project state after every chapter so a
//! crash or cancel leaves a resumable [`Project`] on disk.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use tokio_util::sync::CancellationToken;

use crate::core::audio::{self, AudioTrack, ChapterAtom, EncoderSettings};
use crate::core::epub::{parse_epub, Chapter, HeadingStrategy};
use crate::core::identity::ProjectId;
use crate::core::lesson::single_lesson_concat;
use crate::core::matcher::pack::{build_preview, proportional_pack};
use crate::core::matcher::{
    auto_match, BucketPreview, MatchOutcome, MismatchCondition, MismatchResponse,
};
use crate::core::project::{ChapterReceipt, MatcherDecision, Project, ProjectStage};
use crate::core::store::ProjectStore;
use crate::core::text::read_text_for_upload;
use crate::error::AppError;
use crate::ingest::{AudioSource, TextSource};
use crate::lingq::{ImportLessonRequest, LessonStatus, LingqClient};

/// Returns the next lifecycle stage the project should advance to, or `None`
/// when the project is already `Done`. Pure function; does not look at receipts
/// or filesystem.
pub fn next_stage(p: &Project) -> Option<ProjectStage> {
    match p.stage() {
        ProjectStage::New => Some(ProjectStage::Parsed),
        ProjectStage::Parsed => Some(ProjectStage::Mapped),
        ProjectStage::Mapped => Some(ProjectStage::Transcoded),
        ProjectStage::Transcoded => Some(ProjectStage::Uploaded),
        ProjectStage::Uploaded => Some(ProjectStage::Done),
        ProjectStage::Done => None,
    }
}

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
    /// Terminal signal: orchestrator paused because chapter/track counts
    /// don't pair cleanly and there's no recorded `MatcherDecision`. The UI
    /// consumes this to navigate to `/match` and record the user's choice.
    #[allow(clippy::too_many_arguments)]
    fn needs_match(
        &mut self,
        title: String,
        chapters: usize,
        tracks: usize,
        condition: MismatchCondition,
        options: Vec<MismatchResponse>,
        preselect: MismatchResponse,
        bucket_preview: Option<Vec<BucketPreview>>,
    );
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

    if next_stage(&project).is_none() {
        sink.result(
            true,
            serde_json::json!({"skipped": true, "reason": "already_done"}),
        );
        return Ok(());
    }

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

    advance_if_behind(&mut project, ProjectStage::Parsed)?;
    persist_project(store.as_ref(), &project)?;

    // Decide pairing. If decision is missing AND counts mismatch, ask the UI
    // via the dedicated `NeedsMatch` terminal event. Don't reuse
    // `Result { ok: false }` — that's reserved for "job finished" and
    // breaks downstream consumers that key off the terminal kind.
    let plan = match build_plan(&project, &chapters, &tracks) {
        PlanOrPause::Plan(p) => p,
        PlanOrPause::NeedsMatch {
            condition,
            options,
            preselect,
            bucket_preview,
        } => {
            sink.needs_match(
                project.settings.collection_title.clone(),
                chapters.len(),
                tracks.len(),
                condition,
                options,
                preselect,
                bucket_preview,
            );
            return Ok(());
        }
        PlanOrPause::Cancelled => {
            let err = AppError::Other("matcher decision was Cancel".into());
            sink.result(false, serde_json::json!({"error": err.to_string()}));
            return Err(err);
        }
        PlanOrPause::Failed(msg) => {
            let err = AppError::Other(msg);
            sink.result(false, serde_json::json!({"error": err.to_string()}));
            return Err(err);
        }
    };

    advance_if_behind(&mut project, ProjectStage::Mapped)?;
    prepopulate_receipts(&mut project, &plan);
    persist_project(store.as_ref(), &project)?;

    // Resolve the collection up front. Idempotent on the server side.
    let collection = match client
        .find_or_create_collection(
            &project.settings.collection_title,
            "",
            &project.settings.language,
        )
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

    // Pin the resolved collection on the project so library list & external
    // links can deep-link without re-querying the LingQ API.
    if project.lingq_collection_id != Some(collection.0) {
        project.lingq_collection_id = Some(collection.0);
        project.last_activity_at = Some(Utc::now());
        persist_project(store.as_ref(), &project)?;
    }

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
        let dst = staging
            .path()
            .join(format!("chapter_{:03}.mp3", step.chapter_index));
        let transcode_fut = audio::transcode(&track.path, &dst, &enc, track.window);
        let report = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                tracing::info!(
                    at = step_pos,
                    dst = %dst.display(),
                    "job: cancelled mid-transcode; ffmpeg child killed via Drop",
                );
                if dst.exists() {
                    if let Err(e) = std::fs::remove_file(&dst) {
                        tracing::warn!(error = %e, dst = %dst.display(), "job: failed to unlink partial transcode output");
                    }
                }
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
        tracing::info!(
            chapter = step.chapter_index,
            lesson_id,
            "job: imported lesson"
        );

        // Receipts are pre-populated at Mapped; the upload loop only mutates
        // existing slots via the atomic patch_chapter write.
        let receipt = ChapterReceipt {
            chapter_index: step.chapter_index,
            track_index: Some(step.track_index),
            lesson_id: Some(lesson_id),
            degraded: step.degraded,
            uploaded_at: Some(Utc::now()),
        };
        let slot_idx = project
            .receipts
            .iter()
            .position(|r| r.chapter_index == step.chapter_index)
            .ok_or_else(|| {
                AppError::Other("receipt slot missing — receipts not pre-populated".into())
            })?;
        project.receipts[slot_idx] = receipt.clone();
        store
            .patch_chapter(&project.id, slot_idx, receipt)
            .map_err(|e| AppError::Other(format!("store.patch_chapter: {e}")))?;
        // Trade-off: queue_cursor / completed_lesson_ids / last_activity_at are
        // updated in memory only and persist via the final put at end-of-loop.
        // Resume keys off receipts[].lesson_id, so a crash here is still safe.
        project.queue_cursor = step.chapter_index + 1;
        project.completed_lesson_ids.push(lesson_id);
        project.last_activity_at = Some(Utc::now());

        sink.chapter_done(step.chapter_index, lesson_id, step.degraded);
        let pct = (step_pos as f32 + 1.0) / total.max(1) as f32;
        sink.progress(
            pct,
            Some(format!("Uploaded chapter {}", step.chapter_index + 1)),
        );
    }

    project.advance(ProjectStage::Done)?;
    persist_project(store.as_ref(), &project)?;

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

/// Advance the project to `to` only if it isn't already at or past it. Lets the
/// orchestrator re-enter mid-pipeline (after a resume) without tripping
/// `advance`'s backward-transition guard.
fn advance_if_behind(project: &mut Project, to: ProjectStage) -> Result<(), AppError> {
    if project.stage() >= to {
        return Ok(());
    }
    project
        .advance(to)
        .map_err(|e| AppError::Other(e.to_string()))
}

/// Idempotently append a placeholder receipt for every plan step that doesn't
/// already have one. The upload loop only mutates these slots — it never grows
/// the Vec — so the slot index for `patch_chapter` is stable from here on.
fn prepopulate_receipts(project: &mut Project, plan: &Plan) {
    for step in &plan.steps {
        if project
            .receipts
            .iter()
            .any(|r| r.chapter_index == step.chapter_index)
        {
            continue;
        }
        project.receipts.push(ChapterReceipt {
            chapter_index: step.chapter_index,
            track_index: Some(step.track_index),
            lesson_id: None,
            degraded: step.degraded,
            uploaded_at: None,
        });
    }
}

fn chapter_title(chapters: &[Chapter], idx: usize) -> String {
    chapters
        .iter()
        .find(|c| c.order == idx)
        .map(|c| c.title.clone())
        .unwrap_or_else(|| format!("Chapter {}", idx + 1))
}

/// Title for an audio-only lesson minted from a leftover track. Prefers the
/// track's filename stem; falls back to "Track {n}" (1-based).
fn audio_only_title(track: &AudioTrack, track_index: usize) -> String {
    track
        .path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(str::to_string)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("Track {}", track_index + 1))
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
        condition: MismatchCondition,
        options: Vec<MismatchResponse>,
        preselect: MismatchResponse,
        bucket_preview: Option<Vec<BucketPreview>>,
    },
    Cancelled,
    Failed(String),
}

/// Re-derived mismatch payload for a project parked in `needs_match`.
///
/// Mirrors the `NeedsMatch` job event so the Resolve UI can hydrate from a
/// cold reload without replaying the upload job.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct MismatchInspection {
    pub title: String,
    pub chapter_count: usize,
    pub track_count: usize,
    pub condition: MismatchCondition,
    pub options: Vec<MismatchResponse>,
    pub preselect: MismatchResponse,
    pub bucket_preview: Option<Vec<BucketPreview>>,
}

/// Re-probe a project's sources and recompute the mismatch payload.
///
/// Returns `None` when the project pairs cleanly (no mismatch) or already
/// carries a `matcher_decision`. The caller should redirect the user back to
/// the run/library view in that case.
pub async fn inspect_mismatch(project: &Project) -> Result<Option<MismatchInspection>, AppError> {
    if project.matcher_decision.is_some() {
        return Ok(None);
    }
    let tracks = resolve_audio_tracks(project).await?;
    let chapters = resolve_chapters(&project.sources.text)?;
    match build_plan(project, &chapters, &tracks) {
        PlanOrPause::NeedsMatch {
            condition,
            options,
            preselect,
            bucket_preview,
        } => Ok(Some(MismatchInspection {
            title: project.settings.collection_title.clone(),
            chapter_count: chapters.len(),
            track_count: tracks.len(),
            condition,
            options,
            preselect,
            bucket_preview,
        })),
        _ => Ok(None),
    }
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
        } => {
            let bucket_preview = if condition == MismatchCondition::ManyToFew {
                Some(compute_bucket_preview(chapters, tracks))
            } else {
                None
            };
            PlanOrPause::NeedsMatch {
                condition,
                options,
                preselect,
                bucket_preview,
            }
        }
    }
}

/// Eagerly run the proportional packer so the Mismatch UI can show the
/// proposed bucket layout BEFORE the user confirms `SplitProportional`. Mirrors
/// `plan_from_decision::SplitProportional` so the preview never lies about the
/// final upload shape.
fn compute_bucket_preview(chapters: &[Chapter], tracks: &[AudioTrack]) -> Vec<BucketPreview> {
    let text_chars: Vec<usize> = chapters.iter().map(|c| c.body.chars().count()).collect();
    let atoms: Vec<ChapterAtom> = tracks
        .iter()
        .map(|t| {
            let (start, end) = t
                .window
                .unwrap_or_else(|| (0.0, t.duration_sec.unwrap_or(1.0)));
            ChapterAtom {
                start,
                end,
                title: t.title.clone(),
            }
        })
        .collect();
    let buckets = proportional_pack(&atoms, &text_chars);
    build_preview(&buckets, &text_chars)
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
            if tracks.is_empty() {
                // The user picked SingleLesson, not Cancel — this is a hard
                // failure (no audio to attach), not a quiet cancellation.
                return PlanOrPause::Failed(
                    "single-lesson upload needs at least one audio file".into(),
                );
            }
            let text = single_lesson_concat(chapters);
            let title = chapters
                .first()
                .map(|c| c.title.clone())
                .unwrap_or_else(|| "Lesson".to_string());
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
        PairAccept => {
            // Pair chapters[i] ↔ tracks[i] for i in 0..chapters.len().
            // Any extra tracks beyond chapters.len() ship as audio-only
            // lessons (degraded). For leftover tracks the receipt's
            // `chapter_index` is set to the track's own index `k` — this is
            // a synthetic placeholder, not a real chapter index. The
            // orchestrator's resume-skip logic only cares that the value
            // is unique per receipt, which is satisfied because real
            // chapters occupy indices 0..chapters.len() and leftovers
            // start at chapters.len(). Future refactor: model leftover
            // receipts with `chapter_index: Option<usize>`.
            let mut steps: Vec<Step> = (0..chapters.len())
                .map(|i| Step {
                    chapter_index: i,
                    track_index: i,
                    degraded: false,
                    text_override: None,
                    title_override: None,
                })
                .collect();
            for (k, track) in tracks.iter().enumerate().skip(chapters.len()) {
                let title = audio_only_title(track, k);
                steps.push(Step {
                    // k >= chapters.len() so this never collides with the
                    // 0..chapters.len() block above. Equivalent to `k` but
                    // written this way to flag the leftover semantics.
                    chapter_index: k,
                    track_index: k,
                    degraded: true,
                    // LingQ rejects empty `text`; a single space satisfies the
                    // required field for an audio-only lesson.
                    text_override: Some(" ".to_string()),
                    title_override: Some(title),
                });
            }
            PlanOrPause::Plan(Plan { steps })
        }
        PairDrop => {
            // Pair chapters[i] ↔ tracks[i] for i in 0..min(c, t); drop the
            // leftover side entirely (no upload, no receipt).
            let n = chapters.len().min(tracks.len());
            let dropped_chapters = chapters.len().saturating_sub(n);
            let dropped_tracks = tracks.len().saturating_sub(n);
            if dropped_chapters > 0 || dropped_tracks > 0 {
                tracing::info!(
                    chapters = chapters.len(),
                    tracks = tracks.len(),
                    paired = n,
                    dropped_chapters,
                    dropped_tracks,
                    "matcher decision PairDrop: leftover items dropped",
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
        SplitProportional => {
            // Pack N text chapters into M audio atoms (M < N) using the
            // publisher's chapter boundaries as the truth signal. See AD-023
            // and `docs/specs/m4b-chapters.md`.
            if tracks.is_empty() || chapters.is_empty() {
                return PlanOrPause::Failed("split-proportional needs chapters and tracks".into());
            }
            let text_chars: Vec<usize> = chapters.iter().map(|c| c.body.chars().count()).collect();
            let atoms: Vec<ChapterAtom> = tracks
                .iter()
                .map(|t| {
                    let (start, end) = t
                        .window
                        .unwrap_or_else(|| (0.0, t.duration_sec.unwrap_or(1.0)));
                    ChapterAtom {
                        start,
                        end,
                        title: t.title.clone(),
                    }
                })
                .collect();
            let buckets = proportional_pack(&atoms, &text_chars);
            let steps: Vec<Step> = buckets
                .into_iter()
                .enumerate()
                .map(|(bucket_index, bucket)| {
                    if bucket.text_range.start == bucket.text_range.end {
                        tracing::warn!(
                            bucket_index,
                            "split-proportional produced an empty bucket; uploading audio-only"
                        );
                        let title = bucket
                            .audio
                            .title
                            .clone()
                            .unwrap_or_else(|| format!("Atom {}", bucket_index + 1));
                        Step {
                            chapter_index: bucket_index,
                            track_index: bucket_index,
                            degraded: true,
                            // LingQ rejects empty `text`; a single space satisfies the field.
                            text_override: Some(" ".to_string()),
                            title_override: Some(title),
                        }
                    } else {
                        let slice = &chapters[bucket.text_range.clone()];
                        Step {
                            chapter_index: bucket_index,
                            track_index: bucket_index,
                            degraded: false,
                            text_override: Some(single_lesson_concat(slice)),
                            title_override: Some(slice[0].title.clone()),
                        }
                    }
                })
                .collect();
            PlanOrPause::Plan(Plan { steps })
        }
        // Unknown only deserialises from a foreign response tag written by a
        // newer build. The classifier + UI in this build cannot produce it.
        Unknown => unreachable!("Unknown response cannot be emitted by this build"),
    }
}

fn resolve_chapters(text: &TextSource) -> Result<Vec<Chapter>, AppError> {
    match text {
        TextSource::Epub(p) => parse_epub(p, HeadingStrategy::Kindle)
            .map_err(|e| AppError::Other(format!("epub parse: {e}"))),
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
            expand_single_file(p).await
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

/// Probe the file for embedded chapter atoms. If two or more survive the
/// filter, fan out into one virtual track per atom with the slice window
/// pinned. Zero atoms (population D) and one atom (degenerate) fall back to
/// the whole-file track so behaviour matches pre-atom builds. See AD-023 and
/// `docs/specs/m4b-chapters.md`.
async fn expand_single_file(path: &Path) -> Result<Vec<AudioTrack>, AppError> {
    let atoms = audio::probe_chapters(path).await.unwrap_or_else(|e| {
        tracing::warn!(path = %path.display(), error = %e, "probe_chapters failed; treating as one track");
        Vec::new()
    });
    if atoms.len() < 2 {
        return Ok(vec![track_for(path, 0).await]);
    }
    Ok(atoms
        .into_iter()
        .enumerate()
        .map(|(i, a)| AudioTrack {
            order: i,
            path: path.to_path_buf(),
            duration_sec: Some(a.end - a.start),
            title: a.title,
            window: Some((a.start, a.end)),
        })
        .collect())
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
        window: None,
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
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
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

    #[test]
    fn plan_from_decision_split_proportional_buckets_chapters_into_atoms() {
        let chapters: Vec<Chapter> = [(0, "c0", 100), (1, "c1", 50), (2, "c2", 60), (3, "c3", 90)]
            .iter()
            .map(|(order, title, n)| Chapter {
                order: *order,
                title: (*title).to_string(),
                body: "a".repeat(*n),
            })
            .collect();
        let tracks = vec![
            AudioTrack {
                order: 0,
                path: PathBuf::from("/x/atom0.mp3"),
                duration_sec: Some(50.0),
                title: Some("Atom 0".into()),
                window: Some((0.0, 50.0)),
            },
            AudioTrack {
                order: 1,
                path: PathBuf::from("/x/atom1.mp3"),
                duration_sec: Some(100.0),
                title: Some("Atom 1".into()),
                window: Some((50.0, 150.0)),
            },
        ];
        let decision = MatcherDecision {
            condition: MismatchCondition::ManyToFew,
            response: MismatchResponse::SplitProportional,
            chapter_count: chapters.len(),
            track_count: tracks.len(),
            user_overrode: false,
            decided_at: Utc::now(),
        };

        let plan = match plan_from_decision(&decision, &chapters, &tracks) {
            PlanOrPause::Plan(p) => p,
            _ => panic!("expected Plan"),
        };
        assert_eq!(plan.steps.len(), 2);
        for (i, step) in plan.steps.iter().enumerate() {
            assert!(!step.degraded, "step {i} should not be degraded");
            assert_eq!(step.chapter_index, i);
            assert_eq!(step.track_index, i);
            let body = step.text_override.as_deref().expect("text_override set");
            assert!(!body.is_empty(), "step {i} body must be non-empty");
        }
        // First bucket starts at chapter 0, so its title is c0. The second
        // bucket starts wherever the packer split the run, so we only assert
        // it matches one of the remaining chapter titles.
        assert_eq!(plan.steps[0].title_override.as_deref(), Some("c0"));
        let second = plan.steps[1].title_override.as_deref().unwrap();
        assert!(
            ["c1", "c2", "c3"].contains(&second),
            "second bucket title was {second}"
        );
    }
}
