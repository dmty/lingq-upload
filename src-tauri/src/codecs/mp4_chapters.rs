use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use crate::core::audio::{AudioError, ChapterAtom};

/// Read MP4 chapter atoms from a file. Prefers QuickTime chapter tracks
/// (`tref/chap` → referenced text trak); falls back to Nero `chpl`. Returns
/// an empty vec if the container has no chapters or is not MP4.
pub fn read_chapters(path: &Path) -> Result<Vec<ChapterAtom>, AudioError> {
    let f = File::open(path).map_err(|e| AudioError::Io(e.to_string()))?;
    let size = f
        .metadata()
        .map_err(|e| AudioError::Io(e.to_string()))?
        .len();
    let mut reader = BufReader::new(f);

    let moov = match find_top_level_box(&mut reader, size, *b"moov")? {
        Some(b) => b,
        None => return Ok(Vec::new()),
    };

    if let Some(chapters) = read_qt_chapters(&mut reader, &moov)? {
        return Ok(chapters);
    }
    if let Some(chapters) = read_nero_chpl(&mut reader, &moov)? {
        return Ok(chapters);
    }
    Ok(Vec::new())
}

#[derive(Clone, Copy, Debug)]
struct Box {
    /// File offset of the box header.
    header_offset: u64,
    /// File offset of the first byte of the body (after the 8- or 16-byte header).
    body_offset: u64,
    /// Total size including header.
    size: u64,
    kind: [u8; 4],
}

impl Box {
    fn body_end(&self) -> u64 {
        self.header_offset + self.size
    }
    fn body_size(&self) -> u64 {
        self.body_end() - self.body_offset
    }
}

fn read_box_header<R: Read + Seek>(r: &mut R) -> Result<Option<Box>, AudioError> {
    let header_offset = r
        .stream_position()
        .map_err(|e| AudioError::Io(e.to_string()))?;
    let mut hdr = [0u8; 8];
    let n = r.read(&mut hdr).map_err(|e| AudioError::Io(e.to_string()))?;
    if n < 8 {
        return Ok(None);
    }
    let size32 = u32::from_be_bytes(hdr[0..4].try_into().expect("len ok")) as u64;
    let kind: [u8; 4] = hdr[4..8].try_into().expect("len ok");
    let (size, body_offset) = if size32 == 1 {
        let mut ext = [0u8; 8];
        r.read_exact(&mut ext)
            .map_err(|e| AudioError::Io(e.to_string()))?;
        (u64::from_be_bytes(ext), header_offset + 16)
    } else {
        (size32, header_offset + 8)
    };
    Ok(Some(Box {
        header_offset,
        body_offset,
        size,
        kind,
    }))
}

fn find_top_level_box<R: Read + Seek>(
    r: &mut R,
    end: u64,
    kind: [u8; 4],
) -> Result<Option<Box>, AudioError> {
    r.seek(SeekFrom::Start(0))
        .map_err(|e| AudioError::Io(e.to_string()))?;
    while r.stream_position().map_err(|e| AudioError::Io(e.to_string()))? < end {
        let Some(b) = read_box_header(r)? else { break };
        if b.kind == kind {
            return Ok(Some(b));
        }
        if b.size < 8 {
            return Ok(None);
        }
        r.seek(SeekFrom::Start(b.body_end()))
            .map_err(|e| AudioError::Io(e.to_string()))?;
    }
    Ok(None)
}

/// Iterate children of `parent`, yielding each child box.
fn children<R: Read + Seek>(
    r: &mut R,
    parent: &Box,
) -> Result<Vec<Box>, AudioError> {
    let mut out = Vec::new();
    r.seek(SeekFrom::Start(parent.body_offset))
        .map_err(|e| AudioError::Io(e.to_string()))?;
    let end = parent.body_end();
    while r.stream_position().map_err(|e| AudioError::Io(e.to_string()))? < end {
        let Some(b) = read_box_header(r)? else { break };
        if b.size < 8 || b.body_end() > end {
            break;
        }
        out.push(b);
        r.seek(SeekFrom::Start(b.body_end()))
            .map_err(|e| AudioError::Io(e.to_string()))?;
    }
    Ok(out)
}

fn find_child<R: Read + Seek>(
    r: &mut R,
    parent: &Box,
    kind: [u8; 4],
) -> Result<Option<Box>, AudioError> {
    for c in children(r, parent)? {
        if c.kind == kind {
            return Ok(Some(c));
        }
    }
    Ok(None)
}

fn read_body<R: Read + Seek>(r: &mut R, b: &Box) -> Result<Vec<u8>, AudioError> {
    r.seek(SeekFrom::Start(b.body_offset))
        .map_err(|e| AudioError::Io(e.to_string()))?;
    let mut buf = vec![0u8; b.body_size() as usize];
    r.read_exact(&mut buf)
        .map_err(|e| AudioError::Io(e.to_string()))?;
    Ok(buf)
}

// ---------------------------------------------------------------------------
// QuickTime chapter track: tref/chap → referenced text trak → samples in mdat
// ---------------------------------------------------------------------------

fn read_qt_chapters<R: Read + Seek>(
    r: &mut R,
    moov: &Box,
) -> Result<Option<Vec<ChapterAtom>>, AudioError> {
    let trak_boxes: Vec<Box> = children(r, moov)?
        .into_iter()
        .filter(|b| b.kind == *b"trak")
        .collect();

    // Find the audio track and its chapter-trak reference list.
    let mut audio_chap_refs: Vec<u32> = Vec::new();
    for trak in &trak_boxes {
        if !is_audio_trak(r, trak)? {
            continue;
        }
        if let Some(tref) = find_child(r, trak, *b"tref")? {
            if let Some(chap) = find_child(r, &tref, *b"chap")? {
                let body = read_body(r, &chap)?;
                for chunk in body.chunks_exact(4) {
                    audio_chap_refs.push(u32::from_be_bytes(chunk.try_into().expect("len ok")));
                }
                break;
            }
        }
    }

    // Pick the chapter trak: prefer the one referenced via tref/chap; else any
    // text-handler trak that isn't the audio.
    let chap_trak = trak_boxes.iter().find(|trak| {
        track_id(r, trak)
            .ok()
            .flatten()
            .map(|id| audio_chap_refs.contains(&id))
            .unwrap_or(false)
    });
    let chap_trak = match chap_trak {
        Some(t) => *t,
        None => {
            let mut fallback = None;
            for trak in &trak_boxes {
                if is_text_trak(r, trak)? {
                    fallback = Some(*trak);
                    break;
                }
            }
            match fallback {
                Some(t) => t,
                None => return Ok(None),
            }
        }
    };

    let samples = read_chapter_samples(r, &chap_trak)?;
    if samples.is_empty() {
        return Ok(None);
    }

    let out: Vec<ChapterAtom> = samples
        .iter()
        .enumerate()
        .map(|(i, s)| ChapterAtom {
            start: s.start_sec,
            end: samples
                .get(i + 1)
                .map(|n| n.start_sec)
                .unwrap_or(s.start_sec + s.duration_sec),
            title: Some(s.title.clone()),
        })
        .collect();
    Ok(Some(out))
}

fn descend<R: Read + Seek>(
    r: &mut R,
    parent: &Box,
    path: &[[u8; 4]],
) -> Result<Option<Box>, AudioError> {
    let mut cur = *parent;
    for kind in path {
        match find_child(r, &cur, *kind)? {
            Some(c) => cur = c,
            None => return Ok(None),
        }
    }
    Ok(Some(cur))
}

fn is_audio_trak<R: Read + Seek>(r: &mut R, trak: &Box) -> Result<bool, AudioError> {
    Ok(trak_handler(r, trak)? == Some(*b"soun"))
}

fn is_text_trak<R: Read + Seek>(r: &mut R, trak: &Box) -> Result<bool, AudioError> {
    let h = trak_handler(r, trak)?;
    Ok(matches!(h, Some(t) if &t == b"text" || &t == b"sbtl"))
}

fn trak_handler<R: Read + Seek>(r: &mut R, trak: &Box) -> Result<Option<[u8; 4]>, AudioError> {
    let Some(hdlr) = descend(r, trak, &[*b"mdia", *b"hdlr"])? else {
        return Ok(None);
    };
    let body = read_body(r, &hdlr)?;
    // FullBox header (1 + 3) + 4 bytes pre_defined + 4 bytes handler_type
    if body.len() < 12 {
        return Ok(None);
    }
    Ok(Some(body[8..12].try_into().expect("len ok")))
}

fn track_id<R: Read + Seek>(r: &mut R, trak: &Box) -> Result<Option<u32>, AudioError> {
    let Some(tkhd) = find_child(r, trak, *b"tkhd")? else {
        return Ok(None);
    };
    let body = read_body(r, &tkhd)?;
    if body.is_empty() {
        return Ok(None);
    }
    let version = body[0];
    // FullBox(4) + creation(4 or 8) + modification(4 or 8) + track_id(4)
    let track_id_offset = if version == 1 { 4 + 8 + 8 } else { 4 + 4 + 4 };
    if body.len() < track_id_offset + 4 {
        return Ok(None);
    }
    Ok(Some(u32::from_be_bytes(
        body[track_id_offset..track_id_offset + 4].try_into().expect("len ok"),
    )))
}

struct ChapterSample {
    start_sec: f64,
    duration_sec: f64,
    title: String,
}

fn read_chapter_samples<R: Read + Seek>(
    r: &mut R,
    trak: &Box,
) -> Result<Vec<ChapterSample>, AudioError> {
    let Some(mdhd) = descend(r, trak, &[*b"mdia", *b"mdhd"])? else {
        return Ok(Vec::new());
    };
    let timescale = parse_mdhd_timescale(&read_body(r, &mdhd)?);
    if timescale == 0 {
        return Ok(Vec::new());
    }

    let Some(stbl) = descend(r, trak, &[*b"mdia", *b"minf", *b"stbl"])? else {
        return Ok(Vec::new());
    };

    let stts_box = find_child(r, &stbl, *b"stts")?;
    let stsz_box = find_child(r, &stbl, *b"stsz")?;
    let stsc_box = find_child(r, &stbl, *b"stsc")?;
    let stco_box = find_child(r, &stbl, *b"stco")?;
    let co64_box = find_child(r, &stbl, *b"co64")?;

    let stts = stts_box.map(|b| read_body(r, &b)).transpose()?;
    let stsz = stsz_box.map(|b| read_body(r, &b)).transpose()?;
    let stsc = stsc_box.map(|b| read_body(r, &b)).transpose()?;
    let stco = stco_box.map(|b| read_body(r, &b)).transpose()?;
    let co64 = co64_box.map(|b| read_body(r, &b)).transpose()?;

    let (Some(stts), Some(stsz), Some(stsc)) = (stts, stsz, stsc) else {
        return Ok(Vec::new());
    };
    let chunk_offsets = match (stco, co64) {
        (Some(b), _) => parse_stco(&b),
        (_, Some(b)) => parse_co64(&b),
        _ => return Ok(Vec::new()),
    };

    let durations = parse_stts(&stts);
    let sample_sizes = parse_stsz(&stsz);
    let sample_to_chunk = parse_stsc(&stsc);
    let sample_count = sample_sizes.len();
    if sample_count == 0 {
        return Ok(Vec::new());
    }

    let file_offsets = flatten_sample_offsets(&chunk_offsets, &sample_to_chunk, &sample_sizes);
    if file_offsets.len() != sample_count {
        return Ok(Vec::new());
    }

    let mut samples = Vec::with_capacity(sample_count);
    let mut cumulative_units: u64 = 0;
    for (i, &(off, sz)) in file_offsets.iter().enumerate() {
        let dur_units = durations.get(i).copied().unwrap_or(0);
        let start_sec = cumulative_units as f64 / timescale as f64;
        let dur_sec = dur_units as f64 / timescale as f64;
        cumulative_units += dur_units as u64;
        if sz < 2 {
            samples.push(ChapterSample {
                start_sec,
                duration_sec: dur_sec,
                title: String::new(),
            });
            continue;
        }
        r.seek(SeekFrom::Start(off))
            .map_err(|e| AudioError::Io(e.to_string()))?;
        let mut buf = vec![0u8; sz as usize];
        r.read_exact(&mut buf)
            .map_err(|e| AudioError::Io(e.to_string()))?;
        let text_len = u16::from_be_bytes(buf[0..2].try_into().expect("len ok")) as usize;
        let title_end = (2 + text_len).min(buf.len());
        let title = decode_qt_text(&buf[2..title_end]);
        samples.push(ChapterSample {
            start_sec,
            duration_sec: dur_sec,
            title,
        });
    }
    Ok(samples)
}

fn parse_mdhd_timescale(body: &[u8]) -> u32 {
    if body.is_empty() {
        return 0;
    }
    let version = body[0];
    // FullBox(4) + creation + modification + timescale
    let off = if version == 1 { 4 + 8 + 8 } else { 4 + 4 + 4 };
    if body.len() < off + 4 {
        return 0;
    }
    u32::from_be_bytes(body[off..off + 4].try_into().expect("len ok"))
}

fn parse_stts(body: &[u8]) -> Vec<u32> {
    if body.len() < 8 {
        return Vec::new();
    }
    let count = u32::from_be_bytes(body[4..8].try_into().expect("len ok")) as usize;
    let mut out = Vec::new();
    let mut off = 8;
    for _ in 0..count {
        if off + 8 > body.len() {
            break;
        }
        let sample_count = u32::from_be_bytes(body[off..off + 4].try_into().expect("len ok"));
        let delta = u32::from_be_bytes(body[off + 4..off + 8].try_into().expect("len ok"));
        for _ in 0..sample_count {
            out.push(delta);
        }
        off += 8;
    }
    out
}

#[derive(Clone, Copy)]
struct StscEntry {
    first_chunk: u32,
    samples_per_chunk: u32,
}

fn parse_stsc(body: &[u8]) -> Vec<StscEntry> {
    if body.len() < 8 {
        return Vec::new();
    }
    let count = u32::from_be_bytes(body[4..8].try_into().expect("len ok")) as usize;
    let mut out = Vec::with_capacity(count);
    let mut off = 8;
    for _ in 0..count {
        if off + 12 > body.len() {
            break;
        }
        let first_chunk = u32::from_be_bytes(body[off..off + 4].try_into().expect("len ok"));
        let samples_per_chunk = u32::from_be_bytes(body[off + 4..off + 8].try_into().expect("len ok"));
        out.push(StscEntry {
            first_chunk,
            samples_per_chunk,
        });
        off += 12;
    }
    out
}

fn parse_stsz(body: &[u8]) -> Vec<u32> {
    if body.len() < 12 {
        return Vec::new();
    }
    let default_size = u32::from_be_bytes(body[4..8].try_into().expect("len ok"));
    let count = u32::from_be_bytes(body[8..12].try_into().expect("len ok")) as usize;
    if default_size != 0 {
        return vec![default_size; count];
    }
    let mut out = Vec::with_capacity(count);
    let mut off = 12;
    for _ in 0..count {
        if off + 4 > body.len() {
            break;
        }
        out.push(u32::from_be_bytes(body[off..off + 4].try_into().expect("len ok")));
        off += 4;
    }
    out
}

fn parse_stco(body: &[u8]) -> Vec<u64> {
    if body.len() < 8 {
        return Vec::new();
    }
    let count = u32::from_be_bytes(body[4..8].try_into().expect("len ok")) as usize;
    let mut out = Vec::with_capacity(count);
    let mut off = 8;
    for _ in 0..count {
        if off + 4 > body.len() {
            break;
        }
        out.push(u32::from_be_bytes(body[off..off + 4].try_into().expect("len ok")) as u64);
        off += 4;
    }
    out
}

fn parse_co64(body: &[u8]) -> Vec<u64> {
    if body.len() < 8 {
        return Vec::new();
    }
    let count = u32::from_be_bytes(body[4..8].try_into().expect("len ok")) as usize;
    let mut out = Vec::with_capacity(count);
    let mut off = 8;
    for _ in 0..count {
        if off + 8 > body.len() {
            break;
        }
        out.push(u64::from_be_bytes(body[off..off + 8].try_into().expect("len ok")));
        off += 8;
    }
    out
}

/// Flatten the (stco, stsc, stsz) tuple into `(file_offset, size)` per sample.
fn flatten_sample_offsets(
    chunk_offsets: &[u64],
    stsc: &[StscEntry],
    sample_sizes: &[u32],
) -> Vec<(u64, u32)> {
    let mut per_chunk_samples: Vec<u32> = vec![0; chunk_offsets.len()];
    for (i, entry) in stsc.iter().enumerate() {
        let first = entry.first_chunk.saturating_sub(1) as usize;
        let next_first = stsc
            .get(i + 1)
            .map(|n| n.first_chunk.saturating_sub(1) as usize)
            .unwrap_or(chunk_offsets.len());
        let end = next_first.min(chunk_offsets.len());
        for slot in &mut per_chunk_samples[first..end] {
            *slot = entry.samples_per_chunk;
        }
    }

    let mut out = Vec::with_capacity(sample_sizes.len());
    let mut sample_idx = 0usize;
    for (chunk_idx, &chunk_off) in chunk_offsets.iter().enumerate() {
        let n = per_chunk_samples[chunk_idx] as usize;
        let mut off = chunk_off;
        for _ in 0..n {
            if sample_idx >= sample_sizes.len() {
                return out;
            }
            let size = sample_sizes[sample_idx];
            out.push((off, size));
            off += size as u64;
            sample_idx += 1;
        }
    }
    out
}

fn decode_qt_text(bytes: &[u8]) -> String {
    // QuickTime chapter text samples are usually plain UTF-8. Some files
    // prepend a BOM or a 3-byte encoding marker (`encd` atom follows). We
    // tolerate either by stripping a UTF-8 BOM and trimming trailing nulls.
    let trimmed = strip_bom(bytes);
    // Stop at the first NUL or at the first byte that looks like a TLV header
    // for an optional `encd` (encoding) atom — those start with a u32 size + "encd".
    let end = trimmed
        .windows(4)
        .position(|w| w == b"encd")
        .map(|p| p.saturating_sub(4))
        .unwrap_or(trimmed.len());
    let slice = &trimmed[..end];
    let slice = slice.split(|&b| b == 0).next().unwrap_or(slice);
    String::from_utf8_lossy(slice).into_owned()
}

fn strip_bom(b: &[u8]) -> &[u8] {
    if b.len() >= 3 && &b[..3] == b"\xef\xbb\xbf" {
        &b[3..]
    } else {
        b
    }
}

// ---------------------------------------------------------------------------
// Nero `chpl` fallback (some non-Audible m4b)
// ---------------------------------------------------------------------------

fn read_nero_chpl<R: Read + Seek>(
    r: &mut R,
    moov: &Box,
) -> Result<Option<Vec<ChapterAtom>>, AudioError> {
    let udta = match find_child(r, moov, *b"udta")? {
        Some(b) => b,
        None => return Ok(None),
    };
    let chpl = match find_child(r, &udta, *b"chpl")? {
        Some(b) => b,
        None => return Ok(None),
    };
    let body = read_body(r, &chpl)?;
    if body.len() < 5 {
        return Ok(None);
    }
    let version = body[0];
    // version 1 reserves 4 extra bytes before the entry count.
    let count_offset = if version == 0 { 4 } else { 8 };
    if body.len() <= count_offset {
        return Ok(None);
    }
    let count = body[count_offset] as usize;
    let mut entries: Vec<(f64, String)> = Vec::with_capacity(count);
    let mut off = count_offset + 1;
    for _ in 0..count {
        if off + 9 > body.len() {
            break;
        }
        let ts = u64::from_be_bytes(body[off..off + 8].try_into().expect("len ok"));
        let title_len = body[off + 8] as usize;
        off += 9;
        if off + title_len > body.len() {
            break;
        }
        let title = String::from_utf8_lossy(&body[off..off + title_len]).into_owned();
        off += title_len;
        entries.push((ts as f64 / 10_000_000.0, title));
    }
    if entries.is_empty() {
        return Ok(None);
    }

    // Use the next entry's start as the previous entry's end; the last
    // entry's end is left unset — callers usually fill from container duration.
    let mut out: Vec<ChapterAtom> = entries
        .iter()
        .enumerate()
        .map(|(i, (start, title))| ChapterAtom {
            start: *start,
            end: entries.get(i + 1).map(|(s, _)| *s).unwrap_or(*start),
            title: Some(title.clone()),
        })
        .collect();
    if let Some(last) = out.last_mut() {
        if last.end <= last.start {
            last.end = last.start;
        }
    }
    Ok(Some(out))
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
