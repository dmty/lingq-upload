//! Custom `audio://` URI scheme for `<audio>` playback.
//!
//! WebKit on macOS refuses to decode media served over the `asset://`
//! protocol — even when Content-Type is set correctly — for AAC streams in
//! MP4 / m4b containers. Registering our own scheme with proper Range
//! support and an explicit audio MIME header sidesteps that.
//!
//! URL shape: `audio://localhost/<urlencoded-absolute-path>`.
//! The frontend builds it via `percent-encoding` on the bucket's
//! `audioPath` (see `src/lib/audio.ts::audioUrl`).

use std::cmp::min;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

use percent_encoding::percent_decode_str;
use tauri::http::{header, Request, Response, StatusCode};
use tauri::{Runtime, UriSchemeContext};

/// Cap on the byte count served in a single non-ranged response.
/// Browsers always send Range for media, so this only bounds an
/// accidental misuse — keeps a 1 GB audiobook from being slurped into RAM.
const MAX_FULL_BYTES: u64 = 16 * 1024 * 1024;

pub fn handler<R: Runtime>(
    _ctx: UriSchemeContext<'_, R>,
    req: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    let uri = req.uri();
    let raw_path = uri.path().trim_start_matches('/');
    let decoded = percent_decode_str(raw_path).decode_utf8_lossy().into_owned();
    let path = PathBuf::from(decoded);

    let Ok(mut file) = File::open(&path) else {
        return error(StatusCode::NOT_FOUND, "file not found");
    };
    let Ok(meta) = file.metadata() else {
        return error(StatusCode::INTERNAL_SERVER_ERROR, "stat failed");
    };
    let size = meta.len();
    let mime = mime_for(&path);

    let range = req
        .headers()
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_range);

    if let Some((start, end_opt)) = range {
        if start >= size {
            return Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header(header::CONTENT_RANGE, format!("bytes */{size}"))
                .body(Vec::new())
                .unwrap();
        }
        let end = end_opt.unwrap_or(size - 1).min(size - 1);
        let len = end - start + 1;
        let mut buf = vec![0u8; len as usize];
        if file.seek(SeekFrom::Start(start)).is_err() || file.read_exact(&mut buf).is_err() {
            return error(StatusCode::INTERNAL_SERVER_ERROR, "read failed");
        }
        return Response::builder()
            .status(StatusCode::PARTIAL_CONTENT)
            .header(header::CONTENT_TYPE, mime)
            .header(header::ACCEPT_RANGES, "bytes")
            .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{size}"))
            .header(header::CONTENT_LENGTH, len.to_string())
            .body(buf)
            .unwrap();
    }

    // No Range header — return a header probe with capped body.
    let len = min(size, MAX_FULL_BYTES);
    let mut buf = vec![0u8; len as usize];
    let _ = file.read_exact(&mut buf);
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, size.to_string())
        .body(buf)
        .unwrap()
}

fn parse_range(value: &str) -> Option<(u64, Option<u64>)> {
    let rest = value.trim().strip_prefix("bytes=")?;
    let first = rest.split(',').next()?;
    let (start_s, end_s) = first.split_once('-')?;
    let start: u64 = start_s.parse().ok()?;
    let end = if end_s.is_empty() {
        None
    } else {
        Some(end_s.parse().ok()?)
    };
    Some((start, end))
}

fn mime_for(path: &std::path::Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(str::to_ascii_lowercase);
    match ext.as_deref() {
        Some("mp3") => "audio/mpeg",
        Some("m4a" | "m4b" | "mp4" | "aac") => "audio/mp4",
        Some("ogg" | "oga" | "opus") => "audio/ogg",
        Some("flac") => "audio/flac",
        Some("wav") => "audio/wav",
        _ => "application/octet-stream",
    }
}

fn error(status: StatusCode, msg: &'static str) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain")
        .body(msg.as_bytes().to_vec())
        .unwrap()
}
