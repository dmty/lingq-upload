use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use crate::error::AppError;

fn has_audio_extension(p: &Path) -> bool {
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    matches!(ext.as_deref(), Some("m4b" | "m4a" | "mp3"))
}

/// Natural-order comparator: case-insensitive, numeric-aware. Mirrors what
/// `Intl.Collator(undefined, { numeric: true })` does in the browser so the
/// folder-drop ordering on disk matches what users see when they sort the
/// folder in Finder / Explorer.
fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(ca), Some(cb)) => {
                if ca.is_ascii_digit() && cb.is_ascii_digit() {
                    let na: String =
                        std::iter::from_fn(|| ai.next_if(|c| c.is_ascii_digit())).collect();
                    let nb: String =
                        std::iter::from_fn(|| bi.next_if(|c| c.is_ascii_digit())).collect();
                    // Compare numerically by trimming leading zeros, then by raw
                    // length so "01" < "1" (shorter padded form wins ties as a
                    // stable secondary key).
                    let na_trim = na.trim_start_matches('0');
                    let nb_trim = nb.trim_start_matches('0');
                    let ord = na_trim
                        .len()
                        .cmp(&nb_trim.len())
                        .then_with(|| na_trim.cmp(nb_trim));
                    if ord != Ordering::Equal {
                        return ord;
                    }
                    let ord = na.len().cmp(&nb.len());
                    if ord != Ordering::Equal {
                        return ord;
                    }
                } else {
                    ai.next();
                    bi.next();
                    let la = ca.to_ascii_lowercase();
                    let lb = cb.to_ascii_lowercase();
                    let ord = la.cmp(&lb);
                    if ord != Ordering::Equal {
                        return ord;
                    }
                    let ord = ca.cmp(&cb);
                    if ord != Ordering::Equal {
                        return ord;
                    }
                }
            }
        }
    }
}

/// Expand a dropped directory into the ordered list of top-level audio files
/// it contains. Non-recursive; extensions `m4b` / `m4a` / `mp3` (case-insens).
/// Sorted by natural order on the file name so "1, 2, 10" reads as 1, 2, 10
/// rather than 1, 10, 2.
#[tauri::command]
#[specta::specta]
pub async fn cmd_expand_audio_dir(dir_path: String) -> Result<Vec<String>, AppError> {
    let dir = PathBuf::from(&dir_path);
    if !dir.is_dir() {
        return Err(AppError::Unsupported(format!(
            "not a directory: {}",
            dir.display()
        )));
    }
    let mut out: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(&dir)?.flatten() {
        let p = entry.path();
        if p.is_file() && has_audio_extension(&p) {
            out.push(p);
        }
    }
    out.sort_by(|a, b| {
        let an = a.file_name().and_then(|s| s.to_str()).unwrap_or_default();
        let bn = b.file_name().and_then(|s| s.to_str()).unwrap_or_default();
        natural_cmp(an, bn)
    });
    Ok(out
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn expands_top_level_audio_natural_sorted() {
        let dir = tempdir().unwrap();
        for name in [
            "10.m4b",
            "01.m4b",
            "02.m4b",
            "09.m4b",
            "11.m4b",
            "cover.jpg",
        ] {
            fs::write(dir.path().join(name), b"x").unwrap();
        }
        let nested = dir.path().join("inner");
        fs::create_dir(&nested).unwrap();
        fs::write(nested.join("hidden.m4b"), b"x").unwrap();

        let got = cmd_expand_audio_dir(dir.path().to_string_lossy().into_owned())
            .await
            .unwrap();
        let names: Vec<_> = got
            .iter()
            .map(|p| {
                Path::new(p)
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert_eq!(
            names,
            vec!["01.m4b", "02.m4b", "09.m4b", "10.m4b", "11.m4b"]
        );
    }

    #[tokio::test]
    async fn natural_sort_orders_1_2_10_correctly() {
        let dir = tempdir().unwrap();
        for name in ["chapter-10.m4a", "chapter-2.m4a", "chapter-1.m4a"] {
            fs::write(dir.path().join(name), b"x").unwrap();
        }
        let got = cmd_expand_audio_dir(dir.path().to_string_lossy().into_owned())
            .await
            .unwrap();
        let names: Vec<_> = got
            .iter()
            .map(|p| {
                Path::new(p)
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert_eq!(
            names,
            vec!["chapter-1.m4a", "chapter-2.m4a", "chapter-10.m4a"]
        );
    }

    #[tokio::test]
    async fn empty_dir_returns_empty_vec() {
        let dir = tempdir().unwrap();
        let got = cmd_expand_audio_dir(dir.path().to_string_lossy().into_owned())
            .await
            .unwrap();
        assert!(got.is_empty());
    }

    #[tokio::test]
    async fn dir_with_no_audio_returns_empty_vec() {
        let dir = tempdir().unwrap();
        for name in ["cover.jpg", "info.txt"] {
            fs::write(dir.path().join(name), b"x").unwrap();
        }
        let got = cmd_expand_audio_dir(dir.path().to_string_lossy().into_owned())
            .await
            .unwrap();
        assert!(got.is_empty());
    }

    #[tokio::test]
    async fn non_existent_path_returns_unsupported_error() {
        let res = cmd_expand_audio_dir("/definitely/not/a/real/dir".to_string()).await;
        assert!(matches!(res, Err(AppError::Unsupported(_))));
    }

    #[tokio::test]
    async fn path_pointing_at_a_file_returns_unsupported_error() {
        let dir = tempdir().unwrap();
        let f = dir.path().join("just-a-file.m4b");
        fs::write(&f, b"x").unwrap();
        let res = cmd_expand_audio_dir(f.to_string_lossy().into_owned()).await;
        assert!(matches!(res, Err(AppError::Unsupported(_))));
    }
}
