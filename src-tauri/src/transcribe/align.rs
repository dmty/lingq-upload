use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use specta::Type;
use unicode_normalization::UnicodeNormalization;

use crate::core::epub::{Chapter, ChapterId};
use crate::transcribe::sample::AlignmentConfig;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AlignSource {
    Title,
    Transcript,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq)]
pub struct DetectedRange {
    pub start_chapter_id: ChapterId,
    pub end_chapter_id: ChapterId,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq)]
pub struct AlignmentMatch {
    pub range: DetectedRange,
    pub confidence: f32,
    pub source: AlignSource,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq)]
pub struct ChapterCandidate {
    pub chapter_id: ChapterId,
    pub order: usize,
    pub title: String,
    pub score: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BoundaryResult {
    Confident {
        chapter_id: ChapterId,
        score: f32,
        top: Vec<ChapterCandidate>,
    },
    LowConfidence {
        top: Vec<ChapterCandidate>,
    },
    ContentPoor,
}

impl BoundaryResult {
    pub fn top(&self) -> &[ChapterCandidate] {
        match self {
            Self::Confident { top, .. } | Self::LowConfidence { top } => top,
            Self::ContentPoor => &[],
        }
    }

    pub fn confident_id(&self) -> Option<&ChapterId> {
        match self {
            Self::Confident { chapter_id, .. } => Some(chapter_id),
            _ => None,
        }
    }
}

pub fn normalize_for_alignment(input: &str) -> String {
    // NFKC + Unicode lowercase (not full case folding) + drop non letter/number
    // (non-whitespace) + collapse whitespace.
    let nfkc: String = input.nfkc().collect();
    let lower = nfkc.to_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut prev_space = false;
    for c in lower.chars() {
        if c.is_alphanumeric() {
            out.push(c);
            prev_space = false;
        } else if c.is_whitespace() {
            if !prev_space && !out.is_empty() {
                out.push(' ');
                prev_space = true;
            }
        }
        // punctuation and other marks dropped
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

pub fn title_match(
    first_title: Option<&str>,
    last_title: Option<&str>,
    chapters: &[Chapter],
    config: &AlignmentConfig,
) -> Option<AlignmentMatch> {
    let first = first_title?;
    let last = last_title?;
    let first_norm = normalize_for_alignment(first);
    let last_norm = normalize_for_alignment(last);
    if first_norm.is_empty() || last_norm.is_empty() {
        return None;
    }
    if is_generic_title(&first_norm) || is_generic_title(&last_norm) {
        return None;
    }

    let (start_idx, start_score) = best_unique_match(&first_norm, chapters, config)?;
    let (end_idx, end_score) = best_unique_match(&last_norm, chapters, config)?;
    if chapters[end_idx].order < chapters[start_idx].order {
        return None;
    }

    Some(AlignmentMatch {
        range: DetectedRange {
            start_chapter_id: chapters[start_idx].id.clone(),
            end_chapter_id: chapters[end_idx].id.clone(),
        },
        confidence: (start_score + end_score) * 0.5,
        source: AlignSource::Title,
    })
}

fn is_generic_title(normalized: &str) -> bool {
    const KEYWORDS: &[&str] = &["chapter", "track", "part"];
    let tokens: Vec<&str> = normalized.split_whitespace().collect();
    match tokens.as_slice() {
        [kw, num] if KEYWORDS.contains(kw) && is_digit_token(num) => true,
        [combined] => KEYWORDS.iter().any(|kw| {
            combined
                .strip_prefix(kw)
                .is_some_and(|rest| !rest.is_empty() && is_digit_token(rest))
        }),
        _ => false,
    }
}

fn is_digit_token(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_numeric())
}

fn best_unique_match(
    needle: &str,
    chapters: &[Chapter],
    config: &AlignmentConfig,
) -> Option<(usize, f32)> {
    let mut scored: Vec<(usize, f32)> = chapters
        .iter()
        .enumerate()
        .map(|(i, ch)| {
            let hay = normalize_for_alignment(&ch.title);
            (i, alignment_score(needle, &hay))
        })
        .collect();
    // Descending score, then chapter.order, then slice index.
    scored.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| chapters[a.0].order.cmp(&chapters[b.0].order))
            .then_with(|| a.0.cmp(&b.0))
    });
    let (best_idx, best) = *scored.first()?;
    if best < config.title_confidence {
        return None;
    }
    let runner_up = scored.get(1).map(|(_, s)| *s).unwrap_or(0.0);
    if best - runner_up < config.runner_up_gap {
        return None;
    }
    Some((best_idx, best))
}

fn alignment_score(a: &str, b: &str) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    0.5 * token_set_jaccard(a, b) + 0.5 * normalized_lcs(a, b)
}

fn token_set_jaccard(a: &str, b: &str) -> f32 {
    let sa: HashSet<&str> = a.split_whitespace().collect();
    let sb: HashSet<&str> = b.split_whitespace().collect();
    if sa.is_empty() || sb.is_empty() {
        return 0.0;
    }
    let inter = sa.intersection(&sb).count() as f32;
    let union = sa.union(&sb).count() as f32;
    inter / union
}

fn normalized_lcs(a: &str, b: &str) -> f32 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let max_len = a.len().max(b.len());
    if max_len == 0 {
        return 0.0;
    }
    lcs_substring_len(&a, &b) as f32 / max_len as f32
}

/// Longest common substring length; O(min(n, m)) extra memory.
fn lcs_substring_len(a: &[char], b: &[char]) -> usize {
    if a.is_empty() || b.is_empty() {
        return 0;
    }
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    let mut prev = vec![0usize; short.len() + 1];
    let mut cur = vec![0usize; short.len() + 1];
    let mut best = 0usize;
    for &ch in long {
        for (j, &sch) in short.iter().enumerate() {
            let j1 = j + 1;
            cur[j1] = if ch == sch { prev[j] + 1 } else { 0 };
            best = best.max(cur[j1]);
        }
        std::mem::swap(&mut prev, &mut cur);
        cur.fill(0);
    }
    best
}

fn probe_len(transcript_scalars: usize) -> usize {
    (transcript_scalars as f64 * 1.5).ceil().min(400.0) as usize
}

fn take_head_probe(normalized: &str, len: usize) -> String {
    normalized.chars().take(len).collect()
}

fn take_tail_probe(normalized: &str, len: usize) -> String {
    let chars: Vec<char> = normalized.chars().collect();
    if chars.len() <= len {
        return normalized.to_string();
    }
    chars[chars.len() - len..].iter().collect()
}

fn transcript_lcs_score(transcript: &str, probe: &str) -> f32 {
    let a: Vec<char> = transcript.chars().collect();
    let b: Vec<char> = probe.chars().collect();
    let denom = a.len().min(b.len());
    if denom == 0 {
        return 0.0;
    }
    lcs_substring_len(&a, &b) as f32 / denom as f32
}

fn rank_boundary<'a>(
    transcript_norm: &str,
    chapters: impl IntoIterator<Item = &'a Chapter>,
    head: bool,
    config: &AlignmentConfig,
    force_low: bool,
) -> BoundaryResult {
    let len = probe_len(transcript_norm.chars().count());
    let mut scored: Vec<ChapterCandidate> = chapters
        .into_iter()
        .map(|ch| {
            let body = normalize_for_alignment(&ch.body);
            let probe = if head {
                take_head_probe(&body, len)
            } else {
                take_tail_probe(&body, len)
            };
            ChapterCandidate {
                chapter_id: ch.id.clone(),
                order: ch.order,
                title: ch.title.clone(),
                score: transcript_lcs_score(transcript_norm, &probe),
            }
        })
        .collect();
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.order.cmp(&b.order))
    });
    scored.truncate(3);
    let best = scored.first().map(|c| c.score).unwrap_or(0.0);
    let runner_up = scored.get(1).map(|c| c.score).unwrap_or(0.0);
    if !force_low
        && best >= config.transcript_confidence
        && best - runner_up >= config.runner_up_gap
    {
        let best_c = scored[0].clone();
        return BoundaryResult::Confident {
            chapter_id: best_c.chapter_id.clone(),
            score: best_c.score,
            top: scored,
        };
    }
    BoundaryResult::LowConfidence { top: scored }
}

fn prepare_transcript(transcript: &str) -> Result<String, BoundaryResult> {
    let norm = normalize_for_alignment(transcript);
    if norm.is_empty() || norm.chars().count() < 20 {
        return Err(BoundaryResult::ContentPoor);
    }
    Ok(norm)
}

pub fn transcript_match_head(
    head: &str,
    chapters: &[Chapter],
    config: &AlignmentConfig,
) -> BoundaryResult {
    let Ok(norm) = prepare_transcript(head) else {
        return BoundaryResult::ContentPoor;
    };
    rank_boundary(&norm, chapters.iter(), true, config, false)
}

pub fn transcript_match_tail(
    tail: &str,
    chapters: &[Chapter],
    start: Option<&ChapterId>,
    config: &AlignmentConfig,
) -> BoundaryResult {
    let Ok(norm) = prepare_transcript(tail) else {
        return BoundaryResult::ContentPoor;
    };
    match start {
        None => rank_boundary(&norm, chapters.iter(), false, config, false),
        Some(id) => {
            let Some(start_ch) = chapters.iter().find(|c| &c.id == id) else {
                return rank_boundary(&norm, chapters.iter(), false, config, true);
            };
            let min_order = start_ch.order;
            rank_boundary(
                &norm,
                chapters.iter().filter(|c| c.order >= min_order),
                false,
                config,
                false,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn chapters_with_titles(titles: impl IntoIterator<Item = impl AsRef<str>>) -> Vec<Chapter> {
        titles
            .into_iter()
            .enumerate()
            .map(|(i, t)| {
                let title = t.as_ref().to_string();
                Chapter {
                    order: i,
                    id: ChapterId::from_chapter_parts("test", &format!("spine-{i}"), &title),
                    title: title.clone(),
                    body: String::new(),
                    ..Default::default()
                }
            })
            .collect()
    }

    #[test]
    fn normalize_nfkc_lower_punct_whitespace() {
        // ﬁ (U+FB01) → fi under NFKC; punctuation dropped; whitespace collapsed.
        assert_eq!(normalize_for_alignment("  The  ﬁsh!  "), "the fish");
        assert_eq!(normalize_for_alignment("Hello,\tWorld!!!"), "hello world");
    }

    #[test]
    fn confident_titles_return_stable_inclusive_ids() {
        let chapters = chapters_with_titles(["序章", "風の谷", "帰還"]);
        let m = title_match(
            Some("風の谷"),
            Some("帰還"),
            &chapters,
            &AlignmentConfig::default(),
        )
        .unwrap();
        assert_eq!(m.range.start_chapter_id, chapters[1].id);
        assert_eq!(m.range.end_chapter_id, chapters[2].id);
        assert_eq!(m.source, AlignSource::Title);
        assert!(m.confidence >= 0.70);
    }

    #[test]
    fn english_confident_pair() {
        let chapters = chapters_with_titles(["Prologue", "Valley of Wind", "The Return"]);
        let m = title_match(
            Some("Valley of Wind"),
            Some("The Return"),
            &chapters,
            &AlignmentConfig::default(),
        )
        .unwrap();
        assert_eq!(m.range.start_chapter_id, chapters[1].id);
        assert_eq!(m.range.end_chapter_id, chapters[2].id);
        assert_eq!(m.source, AlignSource::Title);
    }

    #[test]
    fn russian_confident_pair() {
        let chapters = chapters_with_titles(["Пролог", "Ветер в долине", "Возвращение"]);
        let m = title_match(
            Some("Ветер в долине"),
            Some("Возвращение"),
            &chapters,
            &AlignmentConfig::default(),
        )
        .unwrap();
        assert_eq!(m.range.start_chapter_id, chapters[1].id);
        assert_eq!(m.range.end_chapter_id, chapters[2].id);
    }

    #[test]
    fn missing_boundary_is_inconclusive() {
        let chapters = chapters_with_titles(["A", "B", "C"]);
        assert!(title_match(None, Some("C"), &chapters, &AlignmentConfig::default()).is_none());
        assert!(title_match(Some("A"), None, &chapters, &AlignmentConfig::default()).is_none());
    }

    #[test]
    fn generic_titles_are_inconclusive() {
        let chapters = chapters_with_titles(["Chapter 1", "Track-03", "Part 2", "Real Story"]);
        let cfg = AlignmentConfig::default();
        assert!(title_match(Some("Chapter 1"), Some("Real Story"), &chapters, &cfg).is_none());
        assert!(title_match(Some("Real Story"), Some("Track-03"), &chapters, &cfg).is_none());
        assert!(title_match(Some("Part 2"), Some("Real Story"), &chapters, &cfg).is_none());
    }

    #[test]
    fn narrative_titles_with_chapter_word_are_kept() {
        let chapters = chapters_with_titles(["Intro", "The Chapter of Secrets", "Epilogue"]);
        let m = title_match(
            Some("The Chapter of Secrets"),
            Some("Epilogue"),
            &chapters,
            &AlignmentConfig::default(),
        );
        assert!(m.is_some());
    }

    #[test]
    fn missing_middle_titles_do_not_force_inconclusive() {
        let mut chapters = chapters_with_titles(["Start Here", "middle", "End Here"]);
        chapters[1].title.clear();
        let m = title_match(
            Some("Start Here"),
            Some("End Here"),
            &chapters,
            &AlignmentConfig::default(),
        )
        .unwrap();
        assert_eq!(m.range.start_chapter_id, chapters[0].id);
        assert_eq!(m.range.end_chapter_id, chapters[2].id);
    }

    #[test]
    fn repeated_titles_without_runner_up_gap_are_inconclusive() {
        let chapters = chapters_with_titles(["Same Title", "Same Title", "Unique End"]);
        assert!(title_match(
            Some("Same Title"),
            Some("Unique End"),
            &chapters,
            &AlignmentConfig::default(),
        )
        .is_none());
    }

    #[test]
    fn end_before_start_is_inconclusive() {
        let chapters = chapters_with_titles(["Alpha", "Beta", "Gamma"]);
        assert!(title_match(
            Some("Gamma"),
            Some("Alpha"),
            &chapters,
            &AlignmentConfig::default(),
        )
        .is_none());
    }

    #[test]
    fn returns_stable_id_not_slice_index() {
        let mut chapters = chapters_with_titles(["One", "Two", "Three"]);
        // Non-index-shaped ids already come from from_chapter_parts; scramble orders.
        chapters[0].order = 10;
        chapters[1].order = 20;
        chapters[2].order = 30;
        let m = title_match(
            Some("Two"),
            Some("Three"),
            &chapters,
            &AlignmentConfig::default(),
        )
        .unwrap();
        assert_eq!(m.range.start_chapter_id, chapters[1].id);
        assert_eq!(m.range.end_chapter_id, chapters[2].id);
        assert!(!m.range.start_chapter_id.0.starts_with("idx:"));
    }

    proptest! {
        #[test]
        fn normalization_is_idempotent(s in ".{0,256}") {
            let once = normalize_for_alignment(&s);
            prop_assert_eq!(normalize_for_alignment(&once), once);
        }
    }

    fn chapters_with_bodies(items: &[(&str, &str)]) -> Vec<Chapter> {
        items
            .iter()
            .enumerate()
            .map(|(i, (title, body))| Chapter {
                order: i,
                id: ChapterId::from_chapter_parts("test", &format!("spine-{i}"), title),
                title: (*title).to_string(),
                body: (*body).to_string(),
                ..Default::default()
            })
            .collect()
    }

    fn fixture_chapters() -> Vec<Chapter> {
        chapters_with_bodies(&[
            (
                "Prologue",
                "Once upon a quiet morning in the village the bells rang softly over the hills.",
            ),
            (
                "Valley",
                "The wind swept through the valley of stone and scattered dust across the road.",
            ),
            (
                "Return",
                "Homeward bound after many years away she finally saw the harbor lights again.",
            ),
            (
                "Epilogue",
                "Years later the same harbor lights guided travelers home through the fog.",
            ),
        ])
    }

    const HEAD_RETURN: &str =
        "Homeward bound after many years away she finally saw the harbor lights again.";
    const TAIL_EPILOGUE: &str =
        "Years later the same harbor lights guided travelers home through the fog.";
    const TAIL_RETURN: &str =
        "Homeward bound after many years away she finally saw the harbor lights again.";

    #[test]
    fn transcript_confident_head_english() {
        let chapters = fixture_chapters();
        let head = transcript_match_head(HEAD_RETURN, &chapters, &AlignmentConfig::default());
        let BoundaryResult::Confident {
            chapter_id,
            score,
            top,
        } = head
        else {
            panic!("expected Confident, got {head:?}");
        };
        assert_eq!(chapter_id, chapters[2].id);
        assert!(score >= 0.45);
        assert!(!top.is_empty());
        assert_eq!(top[0].chapter_id, chapters[2].id);
        assert_eq!(top[0].order, chapters[2].order);
        assert_eq!(top[0].title, "Return");
    }

    #[test]
    fn transcript_confident_tail_english() {
        let chapters = fixture_chapters();
        let tail =
            transcript_match_tail(TAIL_EPILOGUE, &chapters, None, &AlignmentConfig::default());
        assert!(matches!(
            tail,
            BoundaryResult::Confident {
                ref chapter_id,
                ..
            } if *chapter_id == chapters[3].id
        ));
    }

    #[test]
    fn transcript_confident_head_japanese() {
        let chapters = chapters_with_bodies(&[
            (
                "序章",
                "むかしむかしあるところに小さな村があり人々は平和に暮らしていました。",
            ),
            (
                "風の谷",
                "風が谷を吹き抜けて砂ぼこりを舞い上げ道を覆い尽くしてしまった。",
            ),
            (
                "帰還",
                "長い旅の末に彼女はようやく港の灯を見て故郷へ帰ることができた。",
            ),
        ]);
        let head = transcript_match_head(
            "長い旅の末に彼女はようやく港の灯を見て故郷へ帰ることができた。",
            &chapters,
            &AlignmentConfig::default(),
        );
        assert_eq!(head.confident_id(), Some(&chapters[2].id));
    }

    #[test]
    fn transcript_confident_head_russian() {
        let chapters = chapters_with_bodies(&[
            (
                "Пролог",
                "Однажды тихим утром колокола мягко звонили над холмами.",
            ),
            ("Долина", "Ветер пронесся по каменной долине и поднял пыль."),
            (
                "Возвращение",
                "После многих лет она снова увидела огни гавани.",
            ),
        ]);
        let head = transcript_match_head(
            "После многих лет она снова увидела огни гавани.",
            &chapters,
            &AlignmentConfig::default(),
        );
        assert_eq!(head.confident_id(), Some(&chapters[2].id));
    }

    #[test]
    fn transcript_confident_head_spanish() {
        let chapters = chapters_with_bodies(&[
            ("Prólogo", "Había una vez una aldea tranquila junto al río."),
            (
                "Valle",
                "El viento barrió el valle de piedra y levantó polvo.",
            ),
            (
                "Regreso",
                "Tras muchos años ella volvió a ver las luces del puerto.",
            ),
        ]);
        let head = transcript_match_head(
            "Tras muchos años ella volvió a ver las luces del puerto.",
            &chapters,
            &AlignmentConfig::default(),
        );
        assert_eq!(head.confident_id(), Some(&chapters[2].id));
    }

    #[test]
    fn transcript_confident_head_german() {
        let chapters = chapters_with_bodies(&[
            ("Prolog", "Es war einmal ein ruhiges Dorf am Fluss."),
            (
                "Tal",
                "Der Wind fegte durch das Steintal und wirbelte Staub auf.",
            ),
            (
                "Rückkehr",
                "Nach vielen Jahren sah sie endlich die Hafenlichter wieder.",
            ),
        ]);
        let head = transcript_match_head(
            "Nach vielen Jahren sah sie endlich die Hafenlichter wieder.",
            &chapters,
            &AlignmentConfig::default(),
        );
        assert_eq!(head.confident_id(), Some(&chapters[2].id));
    }

    #[test]
    fn transcript_nfkc_lowercase_matches_body() {
        let chapters = chapters_with_bodies(&[(
            "Fish",
            "The ﬁsh swam quickly through the clear mountain stream today.",
        )]);
        let head = transcript_match_head(
            "THE FISH SWAM QUICKLY THROUGH THE CLEAR MOUNTAIN STREAM TODAY.",
            &chapters,
            &AlignmentConfig::default(),
        );
        assert_eq!(head.confident_id(), Some(&chapters[0].id));
    }

    #[test]
    fn transcript_probe_respects_400_scalar_cap() {
        let unique = "UNIQUEBOUNDARYMARKERXYZABC";
        // Head probe is first 400 scalars: Early contains unique; Late only past 400.
        let early = format!("{}{}", unique, "a".repeat(400));
        let late_only = format!("{}{}", "b".repeat(450), unique);
        let chapters = chapters_with_bodies(&[("Late", &late_only), ("Early", &early)]);
        let head = transcript_match_head(unique, &chapters, &AlignmentConfig::default());
        assert_eq!(head.confident_id(), Some(&chapters[1].id));
        assert!(head
            .top()
            .iter()
            .find(|c| c.chapter_id == chapters[0].id)
            .map(|c| c.score < 0.45)
            .unwrap_or(true));
    }

    #[test]
    fn transcript_ambiguous_tie_is_low_confidence() {
        let shared = "Shared opening text that matches both chapters equally well here.";
        let chapters = chapters_with_bodies(&[
            ("A", shared),
            ("B", shared),
            (
                "C",
                "Completely different ending material for the third chapter body.",
            ),
        ]);
        let head = transcript_match_head(shared, &chapters, &AlignmentConfig::default());
        assert!(matches!(head, BoundaryResult::LowConfidence { .. }));
        assert!(head.confident_id().is_none());
        assert!(head.top().len() >= 2);
    }

    #[test]
    fn transcript_low_score_is_low_confidence() {
        let chapters = fixture_chapters();
        let head = transcript_match_head(
            "zzz completely unrelated audio gibberish with no chapter overlap xxx",
            &chapters,
            &AlignmentConfig::default(),
        );
        assert!(matches!(head, BoundaryResult::LowConfidence { .. }));
    }

    #[test]
    fn transcript_under_20_scalars_is_content_poor() {
        let chapters = fixture_chapters();
        let head = transcript_match_head("short text", &chapters, &AlignmentConfig::default());
        assert_eq!(head, BoundaryResult::ContentPoor);
        let empty = transcript_match_head("", &chapters, &AlignmentConfig::default());
        assert_eq!(empty, BoundaryResult::ContentPoor);
    }

    #[test]
    fn transcript_top_three_stable_ids_order_title_score() {
        let chapters = chapters_with_bodies(&[
            (
                "C0",
                "alpha unique opening for chapter zero body text here now.",
            ),
            (
                "C1",
                "bravo unique opening for chapter one body text here now.",
            ),
            (
                "C2",
                "charlie unique opening for chapter two body text here now.",
            ),
            (
                "C3",
                "delta unique opening for chapter three body text here now.",
            ),
            (
                "C4",
                "echo unique opening for chapter four body text here now.",
            ),
        ]);
        let head = transcript_match_head(
            "charlie unique opening for chapter two body text here now.",
            &chapters,
            &AlignmentConfig::default(),
        );
        let top = head.top();
        assert!(top.len() <= 3);
        assert_eq!(top[0].chapter_id, chapters[2].id);
        assert_eq!(top[0].order, 2);
        assert_eq!(top[0].title, "C2");
        assert!(top[0].score >= top.get(1).map(|c| c.score).unwrap_or(0.0));
        for w in top.windows(2) {
            assert!(
                w[0].score > w[1].score
                    || ((w[0].score - w[1].score).abs() < f32::EPSILON && w[0].order <= w[1].order)
            );
        }
    }

    #[test]
    fn transcript_confident_head_limits_tail_search_by_stable_order() {
        let chapters = fixture_chapters();
        let head = transcript_match_head(HEAD_RETURN, &chapters, &AlignmentConfig::default());
        let start = head.confident_id().unwrap();
        let tail = transcript_match_tail(
            TAIL_EPILOGUE,
            &chapters,
            Some(start),
            &AlignmentConfig::default(),
        );
        assert!(tail.top().iter().all(|c| c.order >= chapters[2].order));
        assert_eq!(tail.confident_id(), Some(&chapters[3].id));
    }

    #[test]
    fn transcript_uncertain_head_widens_tail_search() {
        let chapters = fixture_chapters();
        // Uncertain head → pass None; early-chapter tail may appear.
        let tail = transcript_match_tail(TAIL_RETURN, &chapters, None, &AlignmentConfig::default());
        assert!(tail.top().iter().any(|c| c.order < chapters[3].order));
    }

    #[test]
    fn transcript_end_before_start_is_rejected() {
        let chapters = fixture_chapters();
        let head = transcript_match_head(HEAD_RETURN, &chapters, &AlignmentConfig::default());
        let start = head.confident_id().unwrap();
        // Prologue-shaped tail must not win once search is narrowed from Return.
        let tail = transcript_match_tail(
            "Once upon a quiet morning in the village the bells rang softly over the hills.",
            &chapters,
            Some(start),
            &AlignmentConfig::default(),
        );
        assert!(tail.top().iter().all(|c| c.order >= chapters[2].order));
        assert!(tail.confident_id().is_none() || tail.confident_id() != Some(&chapters[0].id));
    }

    #[test]
    fn transcript_missing_start_id_is_low_confidence() {
        let chapters = fixture_chapters();
        let missing = ChapterId("missing-id".into());
        let tail = transcript_match_tail(
            TAIL_EPILOGUE,
            &chapters,
            Some(&missing),
            &AlignmentConfig::default(),
        );
        assert!(matches!(tail, BoundaryResult::LowConfidence { .. }));
        assert!(tail.confident_id().is_none());
    }
}
