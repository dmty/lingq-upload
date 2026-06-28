use criterion::{criterion_group, criterion_main, Criterion};
use hound::{SampleFormat, WavSpec, WavWriter};
use lingq_upload_lib::codecs::mp3_encoder::encode_mp3;
use lingq_upload_lib::codecs::{symphonia_impl::SymphoniaDecoder, AudioDecoder, PcmFrame};
use lingq_upload_lib::core::audio::EncoderSettings;
use std::path::PathBuf;

fn write_sine(path: &std::path::Path, seconds: u32, sr: u32) {
    let spec = WavSpec {
        channels: 2,
        sample_rate: sr,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut w = WavWriter::create(path, spec).expect("w");
    let total_frames = seconds * sr;
    for i in 0..total_frames {
        let t = i as f32 / sr as f32;
        let s = (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.5;
        let q = (s * i16::MAX as f32) as i16;
        w.write_sample(q).unwrap();
        w.write_sample(q).unwrap();
    }
    w.finalize().unwrap();
}

fn bench_decode_encode(c: &mut Criterion) {
    let dir = tempfile::tempdir().expect("tmp");
    let src: PathBuf = dir.path().join("sine_60s.wav");
    let dst: PathBuf = dir.path().join("sine_60s.mp3");
    write_sine(&src, 60, 44_100);
    let enc = EncoderSettings::default();

    c.bench_function("decode_encode_60s_sine", |b| {
        b.iter(|| {
            let mut dec = SymphoniaDecoder::open(&src).expect("open");
            let info = dec.info();
            let mut frames: Vec<PcmFrame> = Vec::new();
            while let Some(f) = dec.next_frame().expect("frame") {
                frames.push(f);
            }
            encode_mp3(frames.into_iter(), &info, &dst, &enc).expect("encode");
        });
    });
}

criterion_group!(benches, bench_decode_encode);
criterion_main!(benches);
