//! Title-anchored chapter ↔ track alignment.
//!
//! Index pairing (chapter[i] ↔ track[i]) drifts whenever the ebook carries
//! matter the audiobook omits — a cover page, a table of contents, an
//! afterword, a colophon. When both sides label their chapters the labels are
//! the stronger signal: anchor on them and the unmatched matter folds into the
//! neighbouring audio instead of shifting every later pair.

use std::collections::HashSet;

/// Fate of text chapters that no track title claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leftovers {
    /// Fold into the neighbouring anchored track — front matter joins the
    /// audio before the first anchor, back matter the last anchor's track.
    Squeeze,
    /// Leave unpaired.
    Drop,
}

/// Minimum bigram-Dice similarity for a chapter/track title pair to anchor.
const ANCHOR_SIMILARITY: f64 = 0.6;
/// Fraction of the smaller side that must anchor before titles are trusted
/// over index order.
const MIN_ANCHOR_SHARE: f64 = 0.5;

/// Assign each chapter a track index by matching titles in order. `None` for
/// the whole result means the titles carry too little signal — the caller
/// should fall back to index pairing.
pub fn align_by_title(
    chapter_titles: &[&str],
    track_titles: &[Option<&str>],
    leftovers: Leftovers,
) -> Option<Vec<Option<usize>>> {
    let chapters: Vec<String> = chapter_titles.iter().map(|t| normalize(t)).collect();

    // Greedy monotone anchoring: each track claims the best-scoring chapter at
    // or after the previous anchor, so anchors can never cross.
    let mut anchors: Vec<(usize, usize)> = Vec::new();
    let mut cursor = 0usize;
    for (t, title) in track_titles.iter().enumerate() {
        let Some(track) = title.map(normalize).filter(|s| !s.is_empty()) else {
            continue;
        };
        let mut best: Option<(usize, f64)> = None;
        for (c, chapter) in chapters.iter().enumerate().skip(cursor) {
            let score = similarity(chapter, &track);
            if best.is_none_or(|(_, b)| score > b) {
                best = Some((c, score));
            }
        }
        if let Some((c, _)) = best.filter(|&(_, s)| s >= ANCHOR_SIMILARITY) {
            anchors.push((c, t));
            cursor = c + 1;
        }
    }

    let min_side = chapter_titles.len().min(track_titles.len());
    if anchors.len() < 2 || (anchors.len() as f64) < MIN_ANCHOR_SHARE * min_side as f64 {
        return None;
    }

    let mut out: Vec<Option<usize>> = vec![None; chapter_titles.len()];
    for &(c, t) in &anchors {
        out[c] = Some(t);
    }

    if leftovers == Leftovers::Squeeze {
        let (first_c, first_t) = anchors[0];
        // Front matter belongs to the track just before the first anchor when
        // there is one (the title/intro track), else to the anchor itself.
        // ponytail: two or more unanchored head tracks leave the earliest ones
        // unpaired; the mapping editor shows them, park or reassign by hand.
        let head = first_t.saturating_sub(1);
        for slot in out.iter_mut().take(first_c) {
            *slot = Some(head);
        }
        // Carry the last anchor forward across gaps and the tail.
        let mut current = out[first_c];
        for slot in out.iter_mut().skip(first_c) {
            match *slot {
                Some(t) => current = Some(t),
                None => *slot = current,
            }
        }
    }

    Some(out)
}

/// Fold away the cosmetic differences between an EPUB nav label and an M4B
/// chapter tag: full-width and ASCII whitespace, case.
fn normalize(title: &str) -> String {
    title
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Sørensen–Dice coefficient over character bigrams. `1.0` for equal strings,
/// including single-character titles that yield no bigrams.
fn similarity(a: &str, b: &str) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    if a == b {
        return 1.0;
    }
    let (ga, gb) = (bigrams(a), bigrams(b));
    if ga.is_empty() || gb.is_empty() {
        return 0.0;
    }
    let shared = ga.intersection(&gb).count();
    2.0 * shared as f64 / (ga.len() + gb.len()) as f64
}

fn bigrams(s: &str) -> HashSet<(char, char)> {
    let chars: Vec<char> = s.chars().collect();
    chars.windows(2).map(|w| (w[0], w[1])).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Volume 4 of 転生したらスライムだった件: the EPUB nav carries cover, TOC,
    /// prologue, afterword and colophon that the M4B has no chapter for, and
    /// separates chapter number from name with U+3000 where the M4B uses a
    /// plain space.
    fn slime_v4() -> (Vec<&'static str>, Vec<Option<&'static str>>) {
        (
            vec![
                "表紙",
                "目次",
                "序章\u{3000}動き出す麗人",
                "一章\u{3000}獣王国との交易",
                "二章\u{3000}ガゼル王の招待",
                "三章\u{3000}人間の町へ",
                "四章\u{3000}ブルムンド王国",
                "五章\u{3000}召喚された子供達",
                "六章\u{3000}迷宮攻略",
                "七章\u{3000}救われる魂",
                "終章\u{3000}魔物の天敵",
                "あとがき",
                "奥付",
            ],
            vec![
                Some("タイトル"),
                Some("一章 獣王国との交易"),
                Some("二章 ガゼル王の招待"),
                Some("三章 人間の町へ"),
                Some("四章 ブルムンド王国"),
                Some("五章 召喚された子供達"),
                Some("六章 迷宮攻略"),
                Some("七章 救われる魂"),
                Some("終章 魔物の天敵"),
            ],
        )
    }

    #[test]
    fn squeeze_folds_front_and_back_matter_into_neighbouring_tracks() {
        let (chapters, tracks) = slime_v4();
        let out = align_by_title(&chapters, &tracks, Leftovers::Squeeze).expect("titles align");
        assert_eq!(
            out,
            vec![
                Some(0), // 表紙 → タイトル
                Some(0), // 目次 → タイトル
                Some(0), // 序章 → タイトル
                Some(1), // 一章
                Some(2),
                Some(3),
                Some(4),
                Some(5),
                Some(6),
                Some(7),
                Some(8), // 終章
                Some(8), // あとがき → 終章
                Some(8), // 奥付 → 終章
            ]
        );
    }

    #[test]
    fn drop_leaves_unmatched_matter_unpaired() {
        let (chapters, tracks) = slime_v4();
        let out = align_by_title(&chapters, &tracks, Leftovers::Drop).expect("titles align");
        assert_eq!(
            out,
            vec![
                None,
                None,
                None,
                Some(1),
                Some(2),
                Some(3),
                Some(4),
                Some(5),
                Some(6),
                Some(7),
                Some(8),
                None,
                None,
            ]
        );
    }

    #[test]
    fn anchors_stay_monotone_when_a_title_repeats() {
        let chapters = vec!["Prologue", "Chapter 1", "Chapter 2", "Chapter 1 reprise"];
        let tracks = vec![Some("Chapter 1"), Some("Chapter 2")];
        let out = align_by_title(&chapters, &tracks, Leftovers::Drop).expect("titles align");
        assert_eq!(out, vec![None, Some(0), Some(1), None]);
    }

    #[test]
    fn generic_titles_fall_back_to_index_pairing() {
        let chapters = vec!["Track 01", "Track 02", "Track 03"];
        let tracks = vec![None, None, None];
        assert!(align_by_title(&chapters, &tracks, Leftovers::Squeeze).is_none());
    }

    #[test]
    fn too_few_anchors_falls_back_to_index_pairing() {
        let chapters = vec!["Cover", "One", "Two", "Three", "Four", "Five"];
        let tracks = vec![
            Some("One"),
            Some("完全に無関係"),
            Some("まったく違う"),
            Some("別物"),
            Some("他の何か"),
        ];
        assert!(align_by_title(&chapters, &tracks, Leftovers::Squeeze).is_none());
    }

    #[test]
    fn near_miss_titles_still_anchor() {
        let chapters = vec!["Cover", "Chapter One: Arrival", "Chapter Two: Departure"];
        let tracks = vec![Some("Chapter One - Arrival"), Some("Chapter Two - Departure")];
        let out = align_by_title(&chapters, &tracks, Leftovers::Squeeze).expect("titles align");
        assert_eq!(out, vec![Some(0), Some(0), Some(1)]);
    }
}
