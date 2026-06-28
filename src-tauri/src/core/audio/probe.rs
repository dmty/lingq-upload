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
