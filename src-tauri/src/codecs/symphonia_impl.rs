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
        let reader = probed.format;
        let track = reader
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)
            .ok_or_else(|| AudioError::Decode("no audio track".into()))?;
        let track_id = track.id;
        let sample_rate = track
            .codec_params
            .sample_rate
            .ok_or_else(|| AudioError::Decode("missing sample_rate".into()))?;
        let channels = track
            .codec_params
            .channels
            .map(|c| c.count() as u8)
            .ok_or_else(|| AudioError::Decode("missing channels".into()))?;
        let duration_sec = track
            .codec_params
            .n_frames
            .map(|n| n as f64 / sample_rate as f64)
            .unwrap_or(0.0);
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
        let decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|e| AudioError::Decode(format!("decoder init: {e}")))?;
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
}
