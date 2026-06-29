use std::fs::File;
use std::io::Write;
use std::path::Path;

use mp3lame_encoder::{Bitrate, Builder, FlushNoGap, InterleavedPcm, MonoPcm, Quality};

use super::{AudioDecoder, PcmFrame, StreamInfo};
use crate::codecs::symphonia_impl::SymphoniaDecoder;
use crate::core::audio::{
    AudioError, EncoderSettings, TranscodeReport, DURATION_DELTA_THRESHOLD_SEC,
};

/// Encode an interleaved-PCM stream to MP3.
///
/// Callers must ensure `info` describes the frames yielded by `stream`.
/// If `enc.sample_rate` / `enc.channels` differ from the source, frames are
/// adapted (downmix + linear resample) before encoding.
pub fn encode_mp3<S: Iterator<Item = PcmFrame>>(
    stream: S,
    info: &StreamInfo,
    dst: &Path,
    enc: &EncoderSettings,
) -> Result<TranscodeReport, AudioError> {
    let bitrate = parse_bitrate(&enc.bitrate)?;
    let mut encoder = Builder::new()
        .ok_or_else(|| AudioError::Encode("lame init failed".into()))?
        .with_num_channels(enc.channels)
        .map_err(|e| AudioError::Encode(format!("channels: {e}")))?
        .with_sample_rate(enc.sample_rate)
        .map_err(|e| AudioError::Encode(format!("sample_rate: {e}")))?
        .with_brate(bitrate)
        .map_err(|e| AudioError::Encode(format!("brate: {e}")))?
        .with_quality(Quality::Good)
        .map_err(|e| AudioError::Encode(format!("quality: {e}")))?
        .build()
        .map_err(|e| AudioError::Encode(format!("build: {e}")))?;

    let in_channels = info.channels as usize;
    let out_channels = enc.channels as usize;
    let in_sr = info.sample_rate as usize;
    let out_sr = enc.sample_rate as usize;
    let mut mp3_buf: Vec<u8> = Vec::new();
    let mut total_out_frames: u64 = 0;

    for frame in stream {
        let adapted = adapt_frame(&frame, in_channels, out_channels, in_sr, out_sr);
        total_out_frames += adapted.frames as u64;
        let required = mp3lame_encoder::max_required_buffer_size(adapted.frames);
        mp3_buf.reserve(required);
        if out_channels == 1 {
            encoder
                .encode_to_vec(MonoPcm(adapted.samples.as_slice()), &mut mp3_buf)
                .map_err(|e| AudioError::Encode(format!("encode: {e:?}")))?;
        } else {
            encoder
                .encode_to_vec(InterleavedPcm(adapted.samples.as_slice()), &mut mp3_buf)
                .map_err(|e| AudioError::Encode(format!("encode: {e:?}")))?;
        }
    }

    encoder
        .flush_to_vec::<FlushNoGap>(&mut mp3_buf)
        .map_err(|e| AudioError::Encode(format!("flush: {e:?}")))?;

    let mut out = File::create(dst).map_err(|e| AudioError::Io(e.to_string()))?;
    out.write_all(&mp3_buf)
        .map_err(|e| AudioError::Io(e.to_string()))?;
    out.sync_all().map_err(|e| AudioError::Io(e.to_string()))?;
    drop(out);

    let src_duration = total_out_frames as f64 / out_sr as f64;
    let dst_duration = SymphoniaDecoder::open(dst)?.info().duration_sec;
    let delta = dst_duration - src_duration;
    if delta.abs() > DURATION_DELTA_THRESHOLD_SEC {
        return Err(AudioError::DurationMismatch {
            delta_sec: delta,
            threshold_sec: DURATION_DELTA_THRESHOLD_SEC,
        });
    }

    Ok(TranscodeReport {
        src_duration_sec: src_duration,
        dst_duration_sec: dst_duration,
        delta_sec: delta,
    })
}

/// Parse a bitrate string like "96k" or "96" into a `Bitrate` enum value.
fn parse_bitrate(s: &str) -> Result<Bitrate, AudioError> {
    let kbps: u32 = s
        .trim_end_matches('k')
        .parse()
        .map_err(|_| AudioError::Encode(format!("invalid bitrate: {s}")))?;
    match kbps {
        8 => Ok(Bitrate::Kbps8),
        16 => Ok(Bitrate::Kbps16),
        24 => Ok(Bitrate::Kbps24),
        32 => Ok(Bitrate::Kbps32),
        40 => Ok(Bitrate::Kbps40),
        48 => Ok(Bitrate::Kbps48),
        64 => Ok(Bitrate::Kbps64),
        80 => Ok(Bitrate::Kbps80),
        96 => Ok(Bitrate::Kbps96),
        112 => Ok(Bitrate::Kbps112),
        128 => Ok(Bitrate::Kbps128),
        160 => Ok(Bitrate::Kbps160),
        192 => Ok(Bitrate::Kbps192),
        224 => Ok(Bitrate::Kbps224),
        256 => Ok(Bitrate::Kbps256),
        320 => Ok(Bitrate::Kbps320),
        _ => Err(AudioError::Encode(format!("unsupported bitrate: {kbps}k"))),
    }
}

struct AdaptedFrame {
    samples: Vec<f32>,
    frames: usize,
}

/// Adapt channel count and sample rate. Zero-cost pass-through when src == dst.
/// SIMPLIFY: linear resample — sufficient for narration-band 22.05 kHz target;
/// upgrade to sinc if quality issues arise with music content.
fn adapt_frame(
    frame: &PcmFrame,
    in_channels: usize,
    out_channels: usize,
    in_sr: usize,
    out_sr: usize,
) -> AdaptedFrame {
    // Step 1: channel adapt.
    let downmixed: Vec<f32> = if in_channels > 1 && out_channels == 1 {
        // SIMPLIFY: mono-downmix via explicit averaging; all other cases use generic loop.
        (0..frame.frames)
            .map(|i| {
                let sum: f32 = (0..in_channels)
                    .map(|c| {
                        frame
                            .samples
                            .get(i * in_channels + c)
                            .copied()
                            .unwrap_or(0.0)
                    })
                    .sum();
                sum / in_channels as f32
            })
            .collect()
    } else {
        // Generic loop covers: in == out (copy), mono → stereo (duplicate), and mismatches.
        let mut v = Vec::with_capacity(frame.frames * out_channels);
        for i in 0..frame.frames {
            for c in 0..out_channels {
                v.push(
                    frame
                        .samples
                        .get(i * in_channels + c.min(in_channels.saturating_sub(1)))
                        .copied()
                        .unwrap_or(0.0),
                );
            }
        }
        v
    };

    // Step 2: sample-rate adapt (linear interpolation).
    let samples = if in_sr == out_sr {
        downmixed
    } else {
        let ratio = out_sr as f64 / in_sr as f64;
        let in_frames = downmixed.len() / out_channels.max(1);
        let out_frames = ((in_frames as f64) * ratio) as usize;
        let mut v = Vec::with_capacity(out_frames * out_channels);
        for i in 0..out_frames {
            let src_pos = i as f64 / ratio;
            let lo = src_pos.floor() as usize;
            let hi = (lo + 1).min(in_frames.saturating_sub(1));
            let t = (src_pos - lo as f64) as f32;
            for c in 0..out_channels {
                let a = downmixed.get(lo * out_channels + c).copied().unwrap_or(0.0);
                let b = downmixed.get(hi * out_channels + c).copied().unwrap_or(0.0);
                v.push(a + (b - a) * t);
            }
        }
        v
    };

    let frames = samples.len() / out_channels.max(1);
    AdaptedFrame { samples, frames }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hound::{SampleFormat, WavSpec, WavWriter};
    use tempfile::tempdir;

    fn write_sine(path: &Path, seconds: u32, freq: f32, sr: u32, channels: u16) -> StreamInfo {
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
        StreamInfo {
            sample_rate: sr,
            channels: channels as u8,
            duration_sec: seconds as f64,
            codec: "wav",
        }
    }

    fn decode_all(path: &Path) -> (StreamInfo, Vec<f32>) {
        let mut dec = SymphoniaDecoder::open(path).expect("open");
        let info = dec.info();
        let mut out: Vec<f32> = Vec::new();
        while let Some(f) = dec.next_frame().expect("frame") {
            out.extend_from_slice(&f.samples);
        }
        (info, out)
    }

    #[test]
    fn encode_30s_sine_roundtrip_duration_within_threshold() {
        let dir = tempdir().expect("tmp");
        let src = dir.path().join("sine_30s.wav");
        write_sine(&src, 30, 440.0, 22_050, 2);
        let dst = dir.path().join("sine_30s.mp3");

        let (info, _) = decode_all(&src);
        let enc = EncoderSettings::default();
        let mut dec = SymphoniaDecoder::open(&src).expect("re-open");
        let mut frames: Vec<PcmFrame> = Vec::new();
        while let Some(f) = dec.next_frame().expect("frame") {
            frames.push(f);
        }
        let report = encode_mp3(frames.into_iter(), &info, &dst, &enc).expect("encode");
        assert!(report.delta_sec.abs() < 0.5, "delta {}", report.delta_sec);
        let probed = SymphoniaDecoder::open(&dst).expect("probe mp3").info();
        assert_eq!(probed.sample_rate, enc.sample_rate);
        assert_eq!(probed.channels, enc.channels);
        assert_eq!(probed.codec, "mp3");
    }

    // Decoded-PCM md5 drifts across mp3lame-sys builds (psy-acoustic
    // tuning shifts per LAME build). Cross-OS golden is unportable; gate
    // to the seeding host. Run locally with `--ignored` to re-seed.
    #[test]
    #[ignore = "mp3lame build-specific golden; runs locally, not in CI"]
    fn encode_is_deterministic_md5_pcm() {
        let dir = tempdir().expect("tmp");
        let src = dir.path().join("sine_30s.wav");
        write_sine(&src, 30, 440.0, 22_050, 2);

        let mut dec = SymphoniaDecoder::open(&src).expect("open");
        let info = dec.info();
        let mut frames: Vec<PcmFrame> = Vec::new();
        while let Some(f) = dec.next_frame().expect("frame") {
            frames.push(f);
        }
        let dst = dir.path().join("sine_30s.mp3");
        encode_mp3(frames.into_iter(), &info, &dst, &EncoderSettings::default()).expect("encode");

        let (_, pcm) = decode_all(&dst);
        let i16_pcm: Vec<i16> = pcm
            .iter()
            .map(|s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
            .collect();
        let bytes: Vec<u8> = i16_pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
        let md5 = format!("{:x}", md5::compute(&bytes));

        let golden_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/audio/sine_30s.golden.md5");
        if !golden_path.exists() {
            std::fs::write(&golden_path, &md5).expect("seed golden");
        }
        let expected = std::fs::read_to_string(&golden_path).expect("read golden");
        assert_eq!(md5, expected.trim(), "encoder output drifted from golden");
    }
}
