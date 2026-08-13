use std::fs::File;
use std::path::Path;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{Decoder, DecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;

use super::{AudioDecoder, AudioMetadata, PcmFrame, StreamInfo};
use crate::core::audio::AudioError;

pub struct SymphoniaDecoder {
    reader: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    info: StreamInfo,
    // First decoded packet when stsd/ESDS omit channel config; replayed by next_frame.
    prebuffered: Option<PcmFrame>,
}

pub struct SymphoniaMetadata;

impl AudioDecoder for SymphoniaDecoder {
    fn open(path: &Path) -> Result<Self, AudioError> {
        let file = File::open(path).map_err(|e| AudioError::Io(e.to_string()))?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            hint.with_extension(ext);
        }
        let probed = symphonia::default::get_probe()
            .format(
                &hint,
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .map_err(|e| AudioError::Decode(format!("probe: {e}")))?;
        let mut reader = probed.format;
        let track = reader
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)
            .ok_or_else(|| AudioError::Decode("no audio track".into()))?;
        let track_id = track.id;
        let codec_params_sample_rate = track.codec_params.sample_rate;
        let codec_params_channels = track.codec_params.channels;
        let codec_params_n_frames = track.codec_params.n_frames;
        let extra_data = track.codec_params.extra_data.clone();
        let codec_label = match track.codec_params.codec {
            symphonia::core::codecs::CODEC_TYPE_MP3 => "mp3",
            symphonia::core::codecs::CODEC_TYPE_AAC => "aac",
            symphonia::core::codecs::CODEC_TYPE_FLAC => "flac",
            symphonia::core::codecs::CODEC_TYPE_VORBIS => "vorbis",
            symphonia::core::codecs::CODEC_TYPE_PCM_S16LE
            | symphonia::core::codecs::CODEC_TYPE_PCM_S24LE
            | symphonia::core::codecs::CODEC_TYPE_PCM_F32LE => "wav",
            _ => "unknown",
        };
        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|e| AudioError::Decode(format!("decoder init: {e}")))?;

        let (sample_rate, channels, prebuffered) = {
            let from_params =
                codec_params_sample_rate.zip(codec_params_channels.map(|c| c.count() as u8));
            let from_asc = extra_data
                .as_deref()
                .and_then(parse_aac_asc)
                .map(|(sr, ch)| (codec_params_sample_rate.unwrap_or(sr), ch));
            match from_params.or(from_asc) {
                Some((sr, ch)) => (sr, ch, None),
                None => {
                    let (spec, first_frame) = loop {
                        let pkt = reader
                            .next_packet()
                            .map_err(|e| AudioError::Decode(format!("packet: {e}")))?;
                        if pkt.track_id() != track_id {
                            continue;
                        }
                        let decoded = decoder
                            .decode(&pkt)
                            .map_err(|e| AudioError::Decode(format!("decode: {e}")))?;
                        let spec = *decoded.spec();
                        let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
                        buf.copy_interleaved_ref(decoded);
                        let frames = buf.samples().len() / (spec.channels.count().max(1));
                        break (
                            spec,
                            PcmFrame {
                                samples: buf.samples().to_vec(),
                                frames,
                            },
                        );
                    };
                    let sr = codec_params_sample_rate.unwrap_or(spec.rate);
                    (sr, spec.channels.count() as u8, Some(first_frame))
                }
            }
        };

        let duration_sec = codec_params_n_frames
            .map(|n| n as f64 / sample_rate as f64)
            .unwrap_or(0.0);

        Ok(Self {
            reader,
            decoder,
            track_id,
            info: StreamInfo {
                sample_rate,
                channels,
                duration_sec,
                codec: codec_label,
            },
            prebuffered,
        })
    }

    fn info(&self) -> StreamInfo {
        self.info
    }

    fn seek(&mut self, sec: f64) -> Result<(), AudioError> {
        self.reader
            .seek(
                SeekMode::Coarse,
                SeekTo::Time {
                    time: Time::from(sec),
                    track_id: Some(self.track_id),
                },
            )
            .map_err(|e| AudioError::Decode(format!("seek: {e}")))?;
        Ok(())
    }

    fn next_frame(&mut self) -> Result<Option<PcmFrame>, AudioError> {
        if let Some(frame) = self.prebuffered.take() {
            return Ok(Some(frame));
        }
        loop {
            let packet = match self.reader.next_packet() {
                Ok(p) => p,
                Err(SymphoniaError::IoError(e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    return Ok(None);
                }
                Err(e) => return Err(AudioError::Decode(format!("packet: {e}"))),
            };
            if packet.track_id() != self.track_id {
                continue;
            }
            let decoded = self
                .decoder
                .decode(&packet)
                .map_err(|e| AudioError::Decode(format!("decode: {e}")))?;
            let spec = *decoded.spec();
            let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
            buf.copy_interleaved_ref(decoded);
            let frames = buf.samples().len() / (spec.channels.count().max(1));
            return Ok(Some(PcmFrame {
                samples: buf.samples().to_vec(),
                frames,
            }));
        }
    }
}

/// AAC AudioSpecificConfig: 5-bit AOT, 4-bit sampling index, 4-bit channel config.
/// Accepts raw ASC or MPEG-4 ESDS wrapping (DecoderSpecificInfo tag 0x05).
fn parse_aac_asc(extra: &[u8]) -> Option<(u32, u8)> {
    const RATES: [u32; 13] = [
        96_000, 88_200, 64_000, 48_000, 44_100, 32_000, 24_000, 22_050, 16_000, 12_000, 11_025,
        8_000, 7_350,
    ];
    let asc = aac_asc_payload(extra);
    if asc.len() < 2 {
        return None;
    }
    let aot = asc[0] >> 3;
    if aot == 0 || aot == 31 {
        return None;
    }
    let sf_idx = ((asc[0] & 7) << 1) | (asc[1] >> 7);
    let sr = *RATES.get(sf_idx as usize)?;
    let ch = (asc[1] >> 3) & 0x0F;
    if ch == 0 || ch > 8 {
        return None;
    }
    Some((sr, ch))
}

fn aac_asc_payload(extra: &[u8]) -> &[u8] {
    let mut i = 0;
    while i + 1 < extra.len() {
        if extra[i] == 0x05 {
            let mut j = i + 1;
            while j < extra.len() && extra[j] & 0x80 != 0 {
                j += 1;
            }
            j += 1;
            if j < extra.len() {
                return &extra[j..];
            }
            break;
        }
        i += 1;
    }
    extra
}

impl AudioMetadata for SymphoniaMetadata {
    fn probe_chapters(path: &Path) -> Result<Vec<crate::core::audio::ChapterAtom>, AudioError> {
        super::mp4_chapters::read_chapters(path)
    }

    fn probe_duration(path: &Path) -> Result<f64, AudioError> {
        let dec = SymphoniaDecoder::open(path)?;
        Ok(dec.info.duration_sec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hound::{SampleFormat, WavSpec, WavWriter};
    use tempfile::tempdir;

    fn write_silence_wav(path: &Path, seconds: u32, sr: u32, channels: u16) {
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

    #[test]
    fn probe_duration_matches_synthetic_wav() {
        let dir = tempdir().expect("tmp");
        let p = dir.path().join("silence_5s.wav");
        write_silence_wav(&p, 5, 22_050, 1);
        let d = SymphoniaMetadata::probe_duration(&p).expect("probe");
        assert!((d - 5.0).abs() < 0.05, "duration {d}");
    }

    #[test]
    fn decoder_yields_silent_frames() {
        let dir = tempdir().expect("tmp");
        let p = dir.path().join("silence_1s.wav");
        write_silence_wav(&p, 1, 22_050, 1);
        let mut d = SymphoniaDecoder::open(&p).expect("open");
        let mut total_frames = 0usize;
        while let Some(f) = d.next_frame().expect("frame") {
            total_frames += f.frames;
            assert!(f.samples.iter().all(|s| s.abs() < 1e-3));
        }
        assert!(
            (total_frames as i32 - 22_050).abs() < 1024,
            "frames {total_frames}"
        );
    }

    #[test]
    fn metadata_probe_chapters_matches_mp4_reader() {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/audio/synth_chapters_generic.m4b");
        let atoms = SymphoniaMetadata::probe_chapters(&p).expect("probe");
        assert_eq!(atoms.len(), 3);
    }

    #[test]
    fn metadata_probe_chapters_on_wav_returns_empty() {
        let dir = tempdir().expect("tmp");
        let p = dir.path().join("silence.wav");
        let spec = WavSpec {
            channels: 1,
            sample_rate: 22_050,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        let mut w = WavWriter::create(&p, spec).expect("wav writer");
        for _ in 0..22_050 {
            w.write_sample(0_i16).expect("write");
        }
        w.finalize().expect("finalize");
        let atoms = SymphoniaMetadata::probe_chapters(&p).expect("probe");
        assert!(atoms.is_empty());
    }

    #[test]
    fn parse_aac_lc_44100_stereo_asc() {
        assert_eq!(parse_aac_asc(&[0x12, 0x10]), Some((44_100, 2)));
        // ESDS DecoderSpecificInfo tag 0x05, length 2, then ASC.
        assert_eq!(parse_aac_asc(&[0x05, 0x02, 0x12, 0x10]), Some((44_100, 2)));
        assert_eq!(parse_aac_asc(&[0x00]), None);
    }
}
