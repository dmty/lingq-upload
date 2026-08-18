use std::path::Path;

use serde::{Deserialize, Serialize};
use specta::Type;

use super::AudioError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChapterAtom {
    pub start: f64,
    pub end: f64,
    pub title: Option<String>,
}

/// Floor under the dynamic drop threshold, in seconds.
const MIN_ATOM_SEC: f64 = 6.0;

/// Drop atoms too short to be a real chapter — publisher intro / branding
/// stingers that would otherwise fan out into a spurious virtual track and
/// push the matcher into a count mismatch.
///
/// The threshold scales with mean atom length so it adapts to both
/// full-length audiobooks and short drama CDs. The floor keeps it from
/// eating legitimate short atoms on very short files, where the dynamic term
/// collapses.
pub fn filter_atoms(atoms: Vec<ChapterAtom>, total_duration_sec: f64) -> Vec<ChapterAtom> {
    if atoms.is_empty() {
        return atoms;
    }
    let threshold = (total_duration_sec / atoms.len() as f64 / 10.0).max(MIN_ATOM_SEC);
    atoms
        .into_iter()
        .filter(|a| a.end - a.start >= threshold)
        .collect()
}

pub async fn probe_chapters(path: &Path) -> Result<Vec<ChapterAtom>, AudioError> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        <crate::codecs::SymphoniaMetadata as crate::codecs::AudioMetadata>::probe_chapters(&path)
    })
    .await
    .map_err(|e| AudioError::Io(e.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/audio")
            .join(name)
    }

    fn atom(start: f64, end: f64) -> ChapterAtom {
        ChapterAtom {
            start,
            end,
            title: None,
        }
    }

    #[test]
    fn filter_drops_tiny_preamble_via_dynamic_threshold() {
        // 21,961 s file, 9 atoms → threshold ≈ 244 s; the 41 s branding atom goes.
        let mut atoms = vec![atom(0.0, 41.0)];
        let mut t = 41.0;
        for _ in 0..8 {
            atoms.push(atom(t, t + 2_740.0));
            t += 2_740.0;
        }
        let kept = filter_atoms(atoms, 21_961.0);
        assert_eq!(kept.len(), 8);
        assert_eq!(kept[0].start, 41.0);
    }

    #[test]
    fn filter_floor_bites_when_dynamic_term_collapses() {
        // 120 s / 3 atoms / 10 = 4 s, below the 6 s floor — the 5 s atom still goes.
        let atoms = vec![atom(0.0, 5.0), atom(5.0, 62.5), atom(62.5, 120.0)];
        let kept = filter_atoms(atoms, 120.0);
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].start, 5.0);
    }

    #[test]
    fn filter_keeps_every_atom_when_all_are_long() {
        let atoms = vec![atom(0.0, 20.0), atom(20.0, 40.0), atom(40.0, 60.0)];
        assert_eq!(filter_atoms(atoms.clone(), 60.0), atoms);
    }

    #[test]
    fn filter_on_empty_list_is_a_noop() {
        assert!(filter_atoms(Vec::new(), 0.0).is_empty());
    }

    #[tokio::test]
    async fn filter_drops_the_intro_atom_of_the_intro_fixture() {
        let path = fixture("synth_chapters_intro.m4b");
        let atoms = probe_chapters(&path).await.expect("probe");
        assert_eq!(atoms.len(), 3, "raw atoms: {atoms:?}");
        let total = super::super::probe_duration(&path).await.expect("duration");
        let kept = filter_atoms(atoms, total);
        assert_eq!(kept.len(), 2, "kept: {kept:?}");
        assert!(
            kept.iter().all(|a| a.end - a.start > 6.0),
            "short atom survived: {kept:?}"
        );
    }

    #[tokio::test]
    async fn probe_chapters_yields_three_atoms_for_generic_fixture() {
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
}
