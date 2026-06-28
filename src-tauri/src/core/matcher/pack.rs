use serde::{Deserialize, Serialize};

use crate::core::audio::ChapterAtom;

#[derive(Debug, Clone, PartialEq)]
pub struct Bucket {
    pub audio: ChapterAtom,
    pub text_range: std::ops::Range<usize>,
}

/// Read-only preview row for the Mismatch UI's `SplitProportional` card.
/// One row per audio atom: text-chapter index range that the proportional
/// packer assigned, the atom's title and duration, and the bucket's
/// chars-per-second density. The frontend uses `chars_per_sec` to flag
/// buckets that deviate from the corpus median by more than ±30%, hinting at
/// narrator skips or extra material at the boundary. See AD-023 and
/// `docs/specs/m4b-chapters.md`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BucketPreview {
    pub text_range_start: usize,
    pub text_range_end: usize,
    pub atom_title: Option<String>,
    pub atom_duration_sec: f64,
    pub chars_per_sec: f64,
}

/// Pack N text chapters into M audio chapter atoms by proportional duration vs
/// character count. See AD-023 and `docs/specs/m4b-chapters.md` for the
/// contract; the spec's algorithm doc is the source of truth.
///
/// Preconditions (enforced by the caller, not at runtime): `atoms` is
/// non-empty, and `text_chars` has ≥ 1 entry — the producer side
/// (`resolve_audio_tracks`) routes the WholeFile case before invoking the
/// packer, and the matcher only reaches this path when there is text to pack.
pub fn proportional_pack(atoms: &[ChapterAtom], text_chars: &[usize]) -> Vec<Bucket> {
    debug_assert!(!atoms.is_empty());
    debug_assert!(!text_chars.is_empty());

    let total_duration: f64 = atoms.iter().map(|a| a.end - a.start).sum();
    let audio_boundaries: Vec<f64> = {
        let mut acc = 0.0;
        atoms
            .iter()
            .map(|a| {
                acc += (a.end - a.start) / total_duration;
                acc
            })
            .collect()
    };

    let total_chars: u64 = text_chars.iter().map(|&c| c as u64).sum();
    let cum_chars: Vec<u64> = {
        let mut acc = 0u64;
        text_chars
            .iter()
            .map(|&c| {
                acc += c as u64;
                acc
            })
            .collect()
    };

    let n_atoms = atoms.len();
    let n_text = text_chars.len();
    let total_chars_f = total_chars as f64;
    let mut ranges: Vec<std::ops::Range<usize>> = Vec::with_capacity(n_atoms);

    let mut bucket = 0usize;
    let mut start = 0usize;
    let mut j = 0usize;
    while j < n_text && bucket + 1 < n_atoms {
        let boundary_chars = audio_boundaries[bucket] * total_chars_f;
        let cum_j = cum_chars[j] as f64;
        if cum_j <= boundary_chars {
            j += 1;
            continue;
        }
        let prev = if j == 0 { 0u64 } else { cum_chars[j - 1] } as f64;
        let assigns_to_current = if prev >= boundary_chars {
            // Chapter j starts past the boundary — it belongs forward.
            false
        } else if j == start {
            // Oversized head chapter: prev < boundary < cum and the bucket has
            // nothing in it yet. Keep it here so the bucket isn't lost, even
            // if majority overlap leans right.
            true
        } else {
            // Snap rule: more than half of the chapter on the current side
            // keeps it; tie → earlier bucket. Compared symmetrically in
            // character space to make exact ties (`prev + cum == 2 * boundary`)
            // robust against the floating-point drift of normalised shares.
            prev + cum_j <= 2.0 * boundary_chars
        };
        let end = if assigns_to_current { j + 1 } else { j };
        ranges.push(start..end);
        start = end;
        bucket += 1;
        if assigns_to_current {
            j += 1;
        }
    }

    while ranges.len() < n_atoms - 1 {
        ranges.push(start..start);
    }
    ranges.push(start..n_text);

    ranges
        .into_iter()
        .zip(atoms.iter())
        .map(|(text_range, atom)| Bucket {
            audio: atom.clone(),
            text_range,
        })
        .collect()
}

/// Build a UI-friendly preview from packer output. Empty buckets get
/// `chars_per_sec == 0.0`; a degenerate `atom.end == atom.start` also yields
/// `0.0` so the helper never divides by zero even if a future caller breaks
/// the probe-filter contract.
pub fn build_preview(buckets: &[Bucket], text_chars: &[usize]) -> Vec<BucketPreview> {
    buckets
        .iter()
        .map(|b| {
            let duration = b.audio.end - b.audio.start;
            let chars: u64 = b.text_range.clone().map(|i| text_chars[i] as u64).sum();
            let chars_per_sec = if duration > 0.0 {
                chars as f64 / duration
            } else {
                0.0
            };
            BucketPreview {
                text_range_start: b.text_range.start,
                text_range_end: b.text_range.end,
                atom_title: b.audio.title.clone(),
                atom_duration_sec: duration,
                chars_per_sec,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atom(start: f64, end: f64) -> ChapterAtom {
        ChapterAtom {
            start,
            end,
            title: None,
        }
    }

    fn ranges(buckets: &[Bucket]) -> Vec<std::ops::Range<usize>> {
        buckets.iter().map(|b| b.text_range.clone()).collect()
    }

    #[test]
    fn equal_shares_uniform_text() {
        let atoms = vec![atom(0.0, 10.0), atom(10.0, 20.0), atom(20.0, 30.0)];
        let text = vec![1usize; 6];
        let out = proportional_pack(&atoms, &text);
        assert_eq!(ranges(&out), vec![0..2, 2..4, 4..6]);
    }

    #[test]
    fn realistic_six_atoms_eighty_five_text() {
        let atoms: Vec<ChapterAtom> = (0..6)
            .map(|i| atom(i as f64 * 100.0, (i + 1) as f64 * 100.0))
            .collect();
        let text = vec![7usize; 85];
        let out = proportional_pack(&atoms, &text);
        assert_eq!(
            ranges(&out),
            vec![0..14, 14..28, 28..43, 43..57, 57..71, 71..85]
        );
    }

    #[test]
    fn straddle_assigns_to_majority_side() {
        let atoms = vec![atom(0.0, 100.0), atom(100.0, 200.0)];
        let text = vec![100usize, 50, 100];
        let out = proportional_pack(&atoms, &text);
        assert_eq!(ranges(&out), vec![0..2, 2..3]);
    }

    #[test]
    fn degenerate_oversize_chapter() {
        let atoms = vec![atom(0.0, 10.0), atom(10.0, 20.0), atom(20.0, 100.0)];
        let text = vec![1000usize, 100];
        let out = proportional_pack(&atoms, &text);
        assert_eq!(ranges(&out), vec![0..1, 1..1, 1..2]);
    }

    #[test]
    fn single_atom_takes_all_text() {
        let atoms = vec![atom(0.0, 42.0)];
        let text = vec![5usize, 5, 5];
        let out = proportional_pack(&atoms, &text);
        assert_eq!(ranges(&out), vec![0..3]);
    }

    #[test]
    fn build_preview_normal_buckets_compute_chars_per_sec() {
        let atoms = vec![atom(0.0, 10.0), atom(10.0, 30.0)];
        let text = vec![50usize, 50, 400];
        let buckets = proportional_pack(&atoms, &text);
        let preview = build_preview(&buckets, &text);
        assert_eq!(preview.len(), 2);
        // Bucket 0 spans chapter 0 only (100 chars total ≤ boundary 0.20 of 500 → no, cum_0=50 ≤ 100, cum_1=100 ≤ 100, so bucket 0 = 0..2). 100 chars / 10 s = 10.0.
        // Bucket 1 = 2..3, 400 chars / 20 s = 20.0.
        assert_eq!(preview[0].text_range_start, 0);
        assert_eq!(preview[0].text_range_end, 2);
        assert_eq!(preview[0].atom_duration_sec, 10.0);
        assert!((preview[0].chars_per_sec - 10.0).abs() < 1e-9);
        assert_eq!(preview[1].text_range_start, 2);
        assert_eq!(preview[1].text_range_end, 3);
        assert_eq!(preview[1].atom_duration_sec, 20.0);
        assert!((preview[1].chars_per_sec - 20.0).abs() < 1e-9);
    }

    #[test]
    fn build_preview_empty_bucket_yields_zero_chars_per_sec() {
        let atoms = vec![atom(0.0, 10.0), atom(10.0, 20.0), atom(20.0, 100.0)];
        let text = vec![1000usize, 100];
        let buckets = proportional_pack(&atoms, &text);
        // Per the existing degenerate_oversize_chapter test, ranges are [0..1, 1..1, 1..2].
        let preview = build_preview(&buckets, &text);
        assert_eq!(preview.len(), 3);
        assert_eq!(preview[1].text_range_start, 1);
        assert_eq!(preview[1].text_range_end, 1);
        assert_eq!(preview[1].chars_per_sec, 0.0);
    }

    #[test]
    fn build_preview_preserves_atom_title_and_duration() {
        let atoms = vec![ChapterAtom {
            start: 5.0,
            end: 25.0,
            title: Some("第一章".to_string()),
        }];
        let text = vec![100usize];
        let buckets = proportional_pack(&atoms, &text);
        let preview = build_preview(&buckets, &text);
        assert_eq!(preview[0].atom_title.as_deref(), Some("第一章"));
        assert_eq!(preview[0].atom_duration_sec, 20.0);
        assert!((preview[0].chars_per_sec - 5.0).abs() < 1e-9);
    }

    #[test]
    fn contiguous_coverage_mixed_input() {
        // 4 atoms of mixed durations (total 200 s); 11 text chapters of mixed
        // sizes (total 1000 chars).
        let atoms = vec![
            atom(0.0, 40.0),    // share 0.20, boundary 0.20
            atom(40.0, 100.0),  // share 0.30, boundary 0.50
            atom(100.0, 150.0), // share 0.25, boundary 0.75
            atom(150.0, 200.0), // share 0.25, boundary 1.00
        ];
        let text = vec![100usize, 50, 80, 70, 120, 80, 120, 80, 100, 100, 100];
        // cum_text (chars): 100,150,230,300,420,500,620,700,800,900,1000
        // cum_text (frac):  .10 .15 .23 .30 .42 .50 .62 .70 .80 .90 1.0
        // Bucket 0 boundary .20: j=0,1 fit. j=2 cum=.23 straddles (prev=.15).
        //   left=.05, right=.03 → side-0 wins → chapter 2 to bucket 0. end=3.
        // Bucket 1 boundary .50: j=3 cum=.30 fits. j=4 cum=.42 fits. j=5 cum=.50
        //   exactly fits (<=). j=6 cum=.62 straddles (prev=.50). left=0, right=.12
        //   → side-1 wins → chapter 6 to bucket 2. end=6.
        // Bucket 2 boundary .75: j=6 cum=.62 fits. j=7 cum=.70 fits. j=8 cum=.80
        //   straddles (prev=.70). left=.05, right=.05 → tie → earlier bucket
        //   wins → chapter 8 to bucket 2. end=9.
        // Bucket 3 final: 9..11.
        let out = proportional_pack(&atoms, &text);
        let r = ranges(&out);
        assert_eq!(r, vec![0..3, 3..6, 6..9, 9..11]);

        assert_eq!(out.len(), atoms.len());
        let mut cursor = 0usize;
        for b in &out {
            assert_eq!(b.text_range.start, cursor);
            cursor = b.text_range.end;
        }
        assert_eq!(cursor, text.len());
    }
}
