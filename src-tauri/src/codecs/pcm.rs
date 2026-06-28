#[derive(Debug, Clone)]
pub struct PcmFrame {
    pub samples: Vec<f32>,
    pub frames: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StreamInfo {
    pub sample_rate: u32,
    pub channels: u8,
    pub duration_sec: f64,
    pub codec: &'static str,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcm_frame_holds_interleaved_samples() {
        let f = PcmFrame {
            samples: vec![0.1, -0.1, 0.2, -0.2],
            frames: 2,
        };
        assert_eq!(f.samples.len(), f.frames * 2);
    }

    #[test]
    fn stream_info_roundtrip() {
        let s = StreamInfo {
            sample_rate: 44_100,
            channels: 2,
            duration_sec: 1.5,
            codec: "wav",
        };
        assert_eq!(s.sample_rate, 44_100);
        assert_eq!(s.channels, 2);
    }
}
