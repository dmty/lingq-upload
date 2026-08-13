use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::core::audio::AudioTrack;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SampleSide {
    Head,
    Tail,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SampleWindow {
    pub side: SampleSide,
    pub attempt: u8,
    pub track_index: usize,
    pub path: PathBuf,
    pub start_sec: f64,
    pub end_sec: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SideSamplePlan {
    pub initial: SampleWindow,
    pub retry: Option<SampleWindow>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SamplePlan {
    pub head: SideSamplePlan,
    pub tail: SideSamplePlan,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NoTranscriptReason {
    Empty,
    InsufficientAudio,
    ContentPoor,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AlignmentConfig {
    pub target_sample_sec: f64,
    pub min_sample_sec: f64,
    pub skip_sec: f64,
    pub transcript_confidence: f32,
    pub runner_up_gap: f32,
    pub title_confidence: f32,
}

impl Default for AlignmentConfig {
    fn default() -> Self {
        Self {
            target_sample_sec: 30.0,
            min_sample_sec: 10.0,
            skip_sec: 5.0,
            transcript_confidence: 0.45,
            runner_up_gap: 0.15,
            title_confidence: 0.70,
        }
    }
}

pub fn plan_sample_windows(
    tracks: &[AudioTrack],
    boundary_file_durations: (f64, f64),
    config: &AlignmentConfig,
) -> Result<SamplePlan, NoTranscriptReason> {
    let Some(head_track) = tracks.first() else {
        return Err(NoTranscriptReason::Empty);
    };
    let tail_index = tracks.len() - 1;
    let tail_track = &tracks[tail_index];

    if tail_index == 0 {
        let head_bounds = verified_bounds(head_track, boundary_file_durations.0)?;
        let tail_bounds = verified_bounds(tail_track, boundary_file_durations.1)?;
        return plan_one_track(
            head_track,
            (
                head_bounds.0.max(tail_bounds.0),
                head_bounds.1.min(tail_bounds.1),
            ),
            config,
        );
    }

    let head = plan_head(
        head_track,
        0,
        verified_bounds(head_track, boundary_file_durations.0)?,
        config,
    )?;
    let tail = plan_tail(
        tail_track,
        tail_index,
        verified_bounds(tail_track, boundary_file_durations.1)?,
        config,
    )?;
    Ok(SamplePlan { head, tail })
}

/// One head window per audio part. Used when the file already has embedded
/// chapters — sampling the last part's tail cannot see interiors.
pub fn plan_atom_head_windows(
    tracks: &[AudioTrack],
    durations: &[f64],
    config: &AlignmentConfig,
) -> Result<Vec<SideSamplePlan>, NoTranscriptReason> {
    if tracks.is_empty() {
        return Err(NoTranscriptReason::Empty);
    }
    tracks
        .iter()
        .zip(durations)
        .enumerate()
        .map(|(index, (track, duration))| {
            plan_head(track, index, verified_bounds(track, *duration)?, config)
        })
        .collect()
}

fn verified_bounds(
    track: &AudioTrack,
    verified_duration_sec: f64,
) -> Result<(f64, f64), NoTranscriptReason> {
    if !verified_duration_sec.is_finite() || verified_duration_sec <= 0.0 {
        return Err(NoTranscriptReason::InsufficientAudio);
    }
    let (start, end) = track.window.unwrap_or((0.0, verified_duration_sec));
    if !start.is_finite() || !end.is_finite() {
        return Err(NoTranscriptReason::InsufficientAudio);
    }
    Ok((start.max(0.0), end.min(verified_duration_sec)))
}

fn plan_head(
    track: &AudioTrack,
    track_index: usize,
    bounds: (f64, f64),
    config: &AlignmentConfig,
) -> Result<SideSamplePlan, NoTranscriptReason> {
    let start = bounds.0 + config.skip_sec;
    let end = (start + config.target_sample_sec).min(bounds.1);
    let initial = valid_window(SampleSide::Head, 0, track, track_index, start, end, config)?;
    let retry_start = start + config.target_sample_sec;
    let retry_end = (retry_start + config.target_sample_sec).min(bounds.1);
    let retry = optional_window(
        SampleSide::Head,
        track,
        track_index,
        retry_start,
        retry_end,
        config,
    );
    Ok(SideSamplePlan { initial, retry })
}

fn plan_tail(
    track: &AudioTrack,
    track_index: usize,
    bounds: (f64, f64),
    config: &AlignmentConfig,
) -> Result<SideSamplePlan, NoTranscriptReason> {
    let end = bounds.1 - config.skip_sec;
    let start = bounds.0.max(end - config.target_sample_sec);
    let initial = valid_window(SampleSide::Tail, 0, track, track_index, start, end, config)?;
    let retry_end = end - config.target_sample_sec;
    let retry_start = bounds.0.max(retry_end - config.target_sample_sec);
    let retry = optional_window(
        SampleSide::Tail,
        track,
        track_index,
        retry_start,
        retry_end,
        config,
    );
    Ok(SideSamplePlan { initial, retry })
}

fn plan_one_track(
    track: &AudioTrack,
    bounds: (f64, f64),
    config: &AlignmentConfig,
) -> Result<SamplePlan, NoTranscriptReason> {
    let start = bounds.0 + config.skip_sec;
    let end = bounds.1 - config.skip_sec;
    if end - start < config.min_sample_sec * 2.0 {
        return Err(NoTranscriptReason::InsufficientAudio);
    }
    let midpoint = start + (end - start) / 2.0;
    // Half-bounds: plan_head/plan_tail re-apply skip and keep retries inside each half.
    Ok(SamplePlan {
        head: plan_head(track, 0, (bounds.0, midpoint), config)?,
        tail: plan_tail(track, 0, (midpoint, bounds.1), config)?,
    })
}

fn valid_window(
    side: SampleSide,
    attempt: u8,
    track: &AudioTrack,
    track_index: usize,
    start_sec: f64,
    end_sec: f64,
    config: &AlignmentConfig,
) -> Result<SampleWindow, NoTranscriptReason> {
    (end_sec - start_sec >= config.min_sample_sec)
        .then(|| SampleWindow {
            side,
            attempt,
            track_index,
            path: track.path.clone(),
            start_sec,
            end_sec,
        })
        .ok_or(NoTranscriptReason::InsufficientAudio)
}

fn optional_window(
    side: SampleSide,
    track: &AudioTrack,
    track_index: usize,
    start_sec: f64,
    end_sec: f64,
    config: &AlignmentConfig,
) -> Option<SampleWindow> {
    valid_window(side, 1, track, track_index, start_sec, end_sec, config).ok()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::core::audio::AudioTrack;

    use super::*;

    fn track(order: usize, path: &str, window: Option<(f64, f64)>) -> AudioTrack {
        AudioTrack {
            order,
            path: PathBuf::from(path),
            duration_sec: Some(9_999.0),
            title: None,
            window,
        }
    }

    #[test]
    fn alignment_defaults_match_the_detection_contract() {
        let config = AlignmentConfig::default();

        assert_eq!(config.target_sample_sec, 30.0);
        assert_eq!(config.min_sample_sec, 10.0);
        assert_eq!(config.skip_sec, 5.0);
        assert_eq!(config.transcript_confidence, 0.45);
        assert_eq!(config.runner_up_gap, 0.15);
        assert_eq!(config.title_confidence, 0.70);
    }

    #[test]
    fn embedded_windows_are_clamped_to_verified_boundary_durations() {
        let tracks = vec![
            track(0, "head.m4b", Some((-20.0, 50.0))),
            track(1, "middle.mp3", None),
            track(2, "tail.m4b", Some((60.0, 130.0))),
        ];

        let plan =
            plan_sample_windows(&tracks, (40.0, 100.0), &AlignmentConfig::default()).unwrap();

        assert_eq!(
            (
                plan.head.initial.track_index,
                plan.head.initial.path.as_path(),
                plan.head.initial.start_sec,
                plan.head.initial.end_sec,
            ),
            (0, std::path::Path::new("head.m4b"), 5.0, 35.0)
        );
        assert_eq!(
            (
                plan.tail.initial.track_index,
                plan.tail.initial.path.as_path(),
                plan.tail.initial.start_sec,
                plan.tail.initial.end_sec,
            ),
            (2, std::path::Path::new("tail.m4b"), 65.0, 95.0)
        );
    }

    #[test]
    fn different_tracks_shorten_each_side_independently_at_exact_minimum() {
        let tracks = vec![
            track(0, "head.mp3", Some((20.0, 35.0))),
            track(1, "tail.mp3", Some((70.0, 85.0))),
        ];

        let plan =
            plan_sample_windows(&tracks, (100.0, 100.0), &AlignmentConfig::default()).unwrap();

        assert_eq!(
            (plan.head.initial.start_sec, plan.head.initial.end_sec),
            (25.0, 35.0)
        );
        assert_eq!(
            (plan.tail.initial.start_sec, plan.tail.initial.end_sec),
            (70.0, 80.0)
        );
        assert_eq!(plan.head.initial.side, SampleSide::Head);
        assert_eq!(plan.tail.initial.side, SampleSide::Tail);
        assert_eq!(plan.head.initial.attempt, 0);
        assert_eq!(plan.tail.initial.attempt, 0);
        assert!(plan.head.retry.is_none());
        assert!(plan.tail.retry.is_none());
    }

    #[test]
    fn different_track_window_below_minimum_is_rejected() {
        let tracks = vec![
            track(0, "head.mp3", Some((20.0, 34.999))),
            track(1, "tail.mp3", Some((0.0, 100.0))),
        ];

        assert_eq!(
            plan_sample_windows(&tracks, (100.0, 100.0), &AlignmentConfig::default()),
            Err(NoTranscriptReason::InsufficientAudio)
        );
    }

    #[test]
    fn one_short_track_splits_non_overlapping_at_midpoint() {
        let tracks = vec![track(0, "book.m4b", Some((100.0, 140.0)))];

        let plan =
            plan_sample_windows(&tracks, (200.0, 200.0), &AlignmentConfig::default()).unwrap();

        assert_eq!(
            (plan.head.initial.start_sec, plan.head.initial.end_sec),
            (105.0, 120.0)
        );
        assert_eq!(
            (plan.tail.initial.start_sec, plan.tail.initial.end_sec),
            (120.0, 135.0)
        );
    }

    #[test]
    fn one_track_with_less_than_two_minimums_is_rejected() {
        let tracks = vec![track(0, "book.mp3", Some((0.0, 29.999)))];

        assert_eq!(
            plan_sample_windows(&tracks, (100.0, 100.0), &AlignmentConfig::default()),
            Err(NoTranscriptReason::InsufficientAudio)
        );
    }

    #[test]
    fn retries_shift_inward_once_and_stay_on_their_side() {
        let tracks = vec![
            track(0, "head.mp3", Some((0.0, 80.0))),
            track(1, "tail.mp3", Some((0.0, 80.0))),
        ];

        let plan =
            plan_sample_windows(&tracks, (100.0, 100.0), &AlignmentConfig::default()).unwrap();

        let head_retry = plan.head.retry.unwrap();
        let tail_retry = plan.tail.retry.unwrap();
        assert_eq!(
            (head_retry.attempt, head_retry.start_sec, head_retry.end_sec),
            (1, 35.0, 65.0)
        );
        assert_eq!(
            (tail_retry.attempt, tail_retry.start_sec, tail_retry.end_sec),
            (1, 15.0, 45.0)
        );
    }

    #[test]
    fn long_single_track_retries_do_not_cross_the_midpoint() {
        let tracks = vec![track(0, "book.mp3", Some((0.0, 130.0)))];

        let plan =
            plan_sample_windows(&tracks, (200.0, 200.0), &AlignmentConfig::default()).unwrap();

        assert_eq!(
            (plan.head.initial.start_sec, plan.head.initial.end_sec),
            (5.0, 35.0)
        );
        assert_eq!(
            plan.head
                .retry
                .map(|window| (window.start_sec, window.end_sec)),
            Some((35.0, 65.0))
        );
        assert_eq!(
            plan.tail
                .retry
                .map(|window| (window.start_sec, window.end_sec)),
            Some((65.0, 95.0))
        );
        assert_eq!(
            (plan.tail.initial.start_sec, plan.tail.initial.end_sec),
            (95.0, 125.0)
        );
    }

    #[test]
    fn no_tracks_is_an_empty_content_outcome() {
        assert_eq!(
            plan_sample_windows(&[], (0.0, 0.0), &AlignmentConfig::default()),
            Err(NoTranscriptReason::Empty)
        );
    }

    #[test]
    fn atom_heads_sample_the_opening_of_every_track() {
        let tracks = vec![
            track(0, "book.m4b", Some((0.0, 600.0))),
            track(1, "book.m4b", Some((600.0, 1200.0))),
            track(2, "book.m4b", Some((1200.0, 1800.0))),
        ];
        let plans = plan_atom_head_windows(
            &tracks,
            &[1800.0, 1800.0, 1800.0],
            &AlignmentConfig::default(),
        )
        .unwrap();

        assert_eq!(plans.len(), 3);
        let windows: Vec<_> = plans
            .iter()
            .map(|plan| {
                (
                    plan.initial.track_index,
                    plan.initial.side,
                    plan.initial.start_sec,
                    plan.initial.end_sec,
                )
            })
            .collect();
        assert_eq!(
            windows,
            [
                (0, SampleSide::Head, 5.0, 35.0),
                (1, SampleSide::Head, 605.0, 635.0),
                (2, SampleSide::Head, 1205.0, 1235.0),
            ]
        );
    }
}
