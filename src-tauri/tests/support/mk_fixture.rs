use hound::{SampleFormat, WavSpec, WavWriter};
use std::path::Path;

pub fn write_silence_wav(path: &Path, seconds: u32, sr: u32, channels: u16) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create dir");
    }
    let spec = WavSpec {
        channels,
        sample_rate: sr,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut w = WavWriter::create(path, spec).expect("wav writer");
    let total = seconds * sr * channels as u32;
    for _ in 0..total {
        w.write_sample(0_i16).expect("write");
    }
    w.finalize().expect("finalize");
}

pub fn write_sine_wav(path: &Path, seconds: u32, freq: f32, sr: u32, channels: u16) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create dir");
    }
    let spec = WavSpec {
        channels,
        sample_rate: sr,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut w = WavWriter::create(path, spec).expect("wav writer");
    let total_frames = seconds * sr;
    for i in 0..total_frames {
        let t = i as f32 / sr as f32;
        let s = (2.0 * std::f32::consts::PI * freq * t).sin() * 0.5;
        for _ in 0..channels {
            w.write_sample((s * i16::MAX as f32) as i16).expect("write");
        }
    }
    w.finalize().expect("finalize");
}

/// Drop-in replacement for the previous `ensure_silence_fixture` helper.
/// Writes a 5 s silent WAV — not strictly an `.m4a` any more, but the
/// post-flip code paths read containers through symphonia which doesn't care
/// about the extension. Existing fixtures keep their `.m4a` / `.m4b`
/// extensions where they still hold real m4a/m4b content; new fixtures use
/// `.wav` so the test runtime needs no encoder at fixture-gen time.
pub fn write_silence_m4a_like(path: &Path, seconds: u32) {
    if path.exists() {
        return;
    }
    write_silence_wav(path, seconds, 22_050, 2);
}
