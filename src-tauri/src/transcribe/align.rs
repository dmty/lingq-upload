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
}
