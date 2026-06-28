use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use crate::core::audio::{AudioError, ChapterAtom};

/// Read MP4 chapter atoms from a file. Tries Nero `chpl` first; falls back to
/// a QuickTime chapter-track walk (stub). Returns an empty vec if the
/// container has no chapters or is not MP4.
pub fn read_chapters(path: &Path) -> Result<Vec<ChapterAtom>, AudioError> {
    let f = File::open(path).map_err(|e| AudioError::Io(e.to_string()))?;
    let size = f
        .metadata()
        .map_err(|e| AudioError::Io(e.to_string()))?
        .len();
    let reader = BufReader::new(f);
    let mp4 = match mp4::Mp4Reader::read_header(reader, size) {
        Ok(m) => m,
        Err(_) => return Ok(Vec::new()),
    };
    let total = mp4.duration().as_secs_f64();

    // mp4 0.14 UdtaBox skips chpl — parse it from the raw file.
    // ponytail: scan raw bytes; chpl boxes are ≤ a few KB, always in moov/udta.
    let mut raw = File::open(path).map_err(|e| AudioError::Io(e.to_string()))?;
    if let Some(chpl) = read_nero_chpl(&mut raw, size, total)? {
        return Ok(chpl);
    }
    // ponytail: QT chap-track stub — all encountered audiobooks use Nero chpl.
    Ok(Vec::new())
}

/// Scan `reader` (up to `file_size` bytes) for a Nero `chpl` box and parse it.
fn read_nero_chpl<R: Read + Seek>(
    reader: &mut R,
    file_size: u64,
    total: f64,
) -> Result<Option<Vec<ChapterAtom>>, AudioError> {
    // Scan the file for the 'chpl' FourCC. The box lives inside moov/udta so
    // it's always well before mdat. A simple linear scan is fine; these boxes
    // are tiny compared to the audio payload.
    let mut buf = [0u8; 4096];
    let mut file_offset: u64 = 0;
    let mut window: Vec<u8> = Vec::with_capacity(8192);

    reader
        .seek(SeekFrom::Start(0))
        .map_err(|e| AudioError::Io(e.to_string()))?;

    while file_offset < file_size {
        let n = reader
            .read(&mut buf)
            .map_err(|e| AudioError::Io(e.to_string()))?;
        if n == 0 {
            break;
        }
        // Keep a small overlap so the marker can't straddle a read boundary.
        window.extend_from_slice(&buf[..n]);
        file_offset += n as u64;

        // Search for 'chpl' (0x6368706c).
        if let Some(pos) = find_fourcc(&window, b"chpl") {
            // Box layout (FullBox):
            //   [box_start..pos-4] = 4-byte size (BE u32)  ← pos points to 'chpl'
            //   [pos]              = 'chpl' (4 bytes)
            //   [pos+4]            = version (u8)
            //   [pos+5..pos+7]     = flags (3 bytes)
            //   [pos+8..pos+11]    = 4 bytes reserved (only present in version 1)
            //   [pos+12]           = entry count (u8) for version 0
            //   [pos+16]           = entry count (u8) for version 1
            //   entries: [8-byte timestamp (BE u64, 100 ns ticks)][1-byte title len][title UTF-8]
            if pos < 4 {
                // Can't read the size field; shouldn't happen in valid MP4.
                return Ok(None);
            }
            let data = &window[pos - 4..];
            if data.len() < 21 {
                return Ok(None);
            }
            let version = data[8];
            let count_offset = if version == 0 { 12 } else { 16 };
            if data.len() <= count_offset {
                return Ok(None);
            }
            let count = data[count_offset] as usize;
            let mut offset = count_offset + 1;
            let mut out: Vec<ChapterAtom> = Vec::with_capacity(count);
            for i in 0..count {
                if offset + 9 > data.len() {
                    break;
                }
                let ts_bytes: [u8; 8] = data[offset..offset + 8].try_into().unwrap();
                let ts = u64::from_be_bytes(ts_bytes);
                let title_len = data[offset + 8] as usize;
                offset += 9;
                if offset + title_len > data.len() {
                    break;
                }
                let title = String::from_utf8_lossy(&data[offset..offset + title_len]).into_owned();
                offset += title_len;
                let start = ts as f64 / 10_000_000.0;
                let end = if i + 1 < count {
                    // Peek next entry's timestamp for the end of this atom.
                    // We re-parse it below, so use a sentinel here and fix up
                    // after the loop.
                    0.0
                } else {
                    total
                };
                out.push(ChapterAtom {
                    start,
                    end,
                    title: Some(title),
                });
            }
            // Fix up end times: end[i] = start[i+1].
            for i in 0..out.len().saturating_sub(1) {
                let next_start = out[i + 1].start;
                out[i].end = next_start;
            }
            return Ok(Some(out));
        }

        // Keep only the last 8 bytes of the window to catch boundary-spanning markers.
        if window.len() > 8 {
            let keep = window.len() - 8;
            window.drain(..keep);
        }

        // chpl is always in the first ~64 KB of a well-formed MP4 (inside moov).
        if file_offset > 1024 * 1024 {
            break;
        }
    }
    Ok(None)
}

/// Returns the index of the first byte of `needle` in `haystack`, or `None`.
fn find_fourcc(haystack: &[u8], needle: &[u8; 4]) -> Option<usize> {
    haystack.windows(4).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        // CARGO_MANIFEST_DIR = src-tauri/; fixtures live one level up.
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/audio")
            .join(name)
    }

    #[test]
    fn generic_fixture_yields_three_atoms() {
        let p = fixture("synth_chapters_generic.m4b");
        let atoms = read_chapters(&p).expect("read");
        assert_eq!(atoms.len(), 3, "atoms = {atoms:?}");
        let titles: Vec<_> = atoms
            .iter()
            .map(|a| a.title.as_deref().unwrap_or(""))
            .collect();
        assert_eq!(titles, vec!["Chapter 1", "Chapter 2", "Chapter 3"]);
        for a in &atoms {
            assert!((a.end - a.start - 20.0).abs() < 0.05, "span off: {a:?}");
        }
    }

    #[test]
    fn narrative_fixture_yields_cjk_titles() {
        let p = fixture("synth_chapters_narrative.m4b");
        let atoms = read_chapters(&p).expect("read");
        assert_eq!(atoms.len(), 3);
        let titles: Vec<_> = atoms
            .iter()
            .map(|a| a.title.as_deref().unwrap_or(""))
            .collect();
        assert_eq!(titles, vec!["序章", "第一章", "第二章"]);
    }

    #[test]
    fn non_mp4_returns_empty() {
        let dir = tempfile::tempdir().expect("tmp");
        let p = dir.path().join("not_an_mp4.bin");
        std::fs::write(&p, b"definitely not mp4").expect("write");
        let atoms = read_chapters(&p).expect("read");
        assert!(atoms.is_empty());
    }
}
