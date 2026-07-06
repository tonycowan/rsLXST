use codec2::Codec2 as PureCodec2;
use thiserror::Error;

use crate::{AudioCodec, Codec2Mode, CodecKind, Frame, Profile, RawAudioFrame};

const TYPE_MAP_FACTOR: f32 = i16::MAX as f32;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Codec2CodecError {
    #[error("profile {0:?} does not use Codec2")]
    NonCodec2Profile(Profile),
    #[error("Codec2 mode {0:?} requires the libcodec2 feature and system libcodec2")]
    UnsupportedMode(Codec2Mode),
    #[error("Codec2 frame channel count {actual} does not match profile channel count {expected}")]
    ChannelMismatch { expected: u8, actual: u8 },
    #[error("Codec2 frame sample count {actual} does not match profile sample count {expected}")]
    SampleFrameMismatch { expected: usize, actual: usize },
    #[error("invalid Codec2 frame codec {0:?}")]
    InvalidFrameCodec(CodecKind),
    #[error("Codec2 payload is empty")]
    EmptyPayload,
    #[error("Codec2 payload length {payload_len} is not aligned to frame size {bytes_per_frame}")]
    MisalignedPayload {
        payload_len: usize,
        bytes_per_frame: usize,
    },
    #[error("Codec2 codec error: {0}")]
    Codec(String),
    #[error("LXST wire error: {0}")]
    Wire(#[from] crate::Error),
}

pub struct Codec2EncoderState {
    profile: Profile,
    mode: Codec2Mode,
    channels: u8,
    sample_frames: usize,
    samples_per_frame: usize,
    bytes_per_frame: usize,
    codec: Codec2Engine,
}

pub struct Codec2DecoderState {
    profile: Profile,
    mode: Codec2Mode,
    channels: u8,
    sample_frames: usize,
    samples_per_frame: usize,
    bytes_per_frame: usize,
    codec: Codec2Engine,
}

enum Codec2Engine {
    PureRust(PureCodec2),
    #[cfg(feature = "libcodec2")]
    Native(NativeCodec2),
}

#[cfg(feature = "libcodec2")]
struct NativeCodec2 {
    handle: *mut codec2_sys::CODEC2,
    samples_per_frame: usize,
    bytes_per_frame: usize,
}

#[cfg(feature = "libcodec2")]
impl NativeCodec2 {
    fn new(mode: Codec2Mode) -> Result<Self, Codec2CodecError> {
        let ffi_mode = match mode {
            Codec2Mode::Mode700C => codec2_sys::CODEC2_MODE_700C,
            Codec2Mode::Mode1200 => codec2_sys::CODEC2_MODE_1200,
            Codec2Mode::Mode1300 => codec2_sys::CODEC2_MODE_1300,
            Codec2Mode::Mode1400 => codec2_sys::CODEC2_MODE_1400,
            Codec2Mode::Mode1600 => codec2_sys::CODEC2_MODE_1600,
            Codec2Mode::Mode2400 => codec2_sys::CODEC2_MODE_2400,
            Codec2Mode::Mode3200 => codec2_sys::CODEC2_MODE_3200,
        };
        let handle = unsafe { codec2_sys::codec2_create(ffi_mode) };
        if handle.is_null() {
            return Err(Codec2CodecError::Codec(format!(
                "codec2_create failed for {mode:?}"
            )));
        }
        let samples_per_frame = unsafe { codec2_sys::codec2_samples_per_frame(handle) as usize };
        let bytes_per_frame = unsafe { codec2_sys::codec2_bytes_per_frame(handle) as usize };
        Ok(Self {
            handle,
            samples_per_frame,
            bytes_per_frame,
        })
    }

    fn encode(&self, speech: &[i16], bits: &mut [u8]) {
        debug_assert_eq!(speech.len(), self.samples_per_frame);
        debug_assert_eq!(bits.len(), self.bytes_per_frame);
        unsafe {
            codec2_sys::codec2_encode(self.handle, bits.as_mut_ptr(), speech.as_ptr() as *mut i16);
        }
    }

    fn decode(&self, speech: &mut [i16], bits: &[u8]) {
        debug_assert_eq!(speech.len(), self.samples_per_frame);
        debug_assert_eq!(bits.len(), self.bytes_per_frame);
        unsafe {
            codec2_sys::codec2_decode(self.handle, speech.as_mut_ptr(), bits.as_ptr() as *mut u8);
        }
    }
}

#[cfg(feature = "libcodec2")]
impl Drop for NativeCodec2 {
    fn drop(&mut self) {
        unsafe { codec2_sys::codec2_destroy(self.handle) };
    }
}

impl Codec2Engine {
    fn new(mode: Codec2Mode) -> Result<Self, Codec2CodecError> {
        if mode == Codec2Mode::Mode700C {
            #[cfg(feature = "libcodec2")]
            {
                return Ok(Self::Native(NativeCodec2::new(mode)?));
            }
            #[cfg(not(feature = "libcodec2"))]
            {
                return Err(Codec2CodecError::UnsupportedMode(mode));
            }
        }

        let pure_mode = pure_rust_mode(mode).ok_or(Codec2CodecError::UnsupportedMode(mode))?;
        Ok(Self::PureRust(PureCodec2::new(pure_mode)))
    }

    fn samples_per_frame(&self) -> usize {
        match self {
            Self::PureRust(codec) => codec.samples_per_frame(),
            #[cfg(feature = "libcodec2")]
            Self::Native(codec) => codec.samples_per_frame,
        }
    }

    fn bytes_per_frame(&self) -> usize {
        match self {
            Self::PureRust(codec) => codec.bits_per_frame().div_ceil(8),
            #[cfg(feature = "libcodec2")]
            Self::Native(codec) => codec.bytes_per_frame,
        }
    }

    fn encode(&mut self, speech: &[i16], bits: &mut [u8]) {
        match self {
            Self::PureRust(codec) => codec.encode(bits, speech),
            #[cfg(feature = "libcodec2")]
            Self::Native(codec) => codec.encode(speech, bits),
        }
    }

    fn decode(&mut self, speech: &mut [i16], bits: &[u8]) {
        match self {
            Self::PureRust(codec) => codec.decode(speech, bits),
            #[cfg(feature = "libcodec2")]
            Self::Native(codec) => codec.decode(speech, bits),
        }
    }
}

impl Codec2EncoderState {
    pub fn new(profile: Profile) -> Result<Self, Codec2CodecError> {
        let mode = codec2_mode_for_profile(profile)?;
        let codec = Codec2Engine::new(mode)?;
        let samples_per_frame = codec.samples_per_frame();
        let bytes_per_frame = codec.bytes_per_frame();
        Ok(Self {
            profile,
            mode,
            channels: profile.channels(),
            sample_frames: profile.sample_frames_per_packet(),
            samples_per_frame,
            bytes_per_frame,
            codec,
        })
    }

    pub const fn profile(&self) -> Profile {
        self.profile
    }

    pub const fn mode(&self) -> Codec2Mode {
        self.mode
    }

    pub const fn channels(&self) -> u8 {
        self.channels
    }

    pub const fn sample_frames(&self) -> usize {
        self.sample_frames
    }

    pub fn encode_frame(&mut self, frame: &RawAudioFrame) -> Result<Frame, Codec2CodecError> {
        self.validate_frame_shape(frame)?;
        let pcm = frame_to_pcm(frame, self.channels)?;
        let subframe_count = self
            .sample_frames
            .checked_div(self.samples_per_frame)
            .filter(|count| count * self.samples_per_frame == self.sample_frames)
            .ok_or_else(|| Codec2CodecError::SampleFrameMismatch {
                expected: self.sample_frames,
                actual: frame.sample_frames(),
            })?;

        let mut payload = Vec::with_capacity(1 + subframe_count * self.bytes_per_frame);
        payload.push(self.mode.header());
        let mut encoded = vec![0u8; self.bytes_per_frame];
        for subframe_index in 0..subframe_count {
            let start = subframe_index * self.samples_per_frame;
            let end = start + self.samples_per_frame;
            self.codec.encode(&pcm[start..end], &mut encoded);
            payload.extend_from_slice(&encoded);
        }

        Ok(Frame::new(CodecKind::Codec2, payload))
    }

    fn validate_frame_shape(&self, frame: &RawAudioFrame) -> Result<(), Codec2CodecError> {
        if frame.channels < self.channels {
            return Err(Codec2CodecError::ChannelMismatch {
                expected: self.channels,
                actual: frame.channels,
            });
        }
        if frame.sample_frames() != self.sample_frames {
            return Err(Codec2CodecError::SampleFrameMismatch {
                expected: self.sample_frames,
                actual: frame.sample_frames(),
            });
        }
        Ok(())
    }
}

impl Codec2DecoderState {
    pub fn new(profile: Profile) -> Result<Self, Codec2CodecError> {
        let mode = codec2_mode_for_profile(profile)?;
        let codec = Codec2Engine::new(mode)?;
        let samples_per_frame = codec.samples_per_frame();
        let bytes_per_frame = codec.bytes_per_frame();
        Ok(Self {
            profile,
            mode,
            channels: profile.channels(),
            sample_frames: profile.sample_frames_per_packet(),
            samples_per_frame,
            bytes_per_frame,
            codec,
        })
    }

    pub const fn profile(&self) -> Profile {
        self.profile
    }

    pub const fn mode(&self) -> Codec2Mode {
        self.mode
    }

    pub fn decode_frame(&mut self, frame: &Frame) -> Result<RawAudioFrame, Codec2CodecError> {
        if frame.codec != CodecKind::Codec2 {
            return Err(Codec2CodecError::InvalidFrameCodec(frame.codec));
        }
        if frame.payload.is_empty() {
            return Err(Codec2CodecError::EmptyPayload);
        }

        let header = frame.payload[0];
        let mode = Codec2Mode::from_header(header).unwrap_or(self.mode);
        if mode != self.mode {
            self.mode = mode;
            self.codec = Codec2Engine::new(mode)?;
            self.samples_per_frame = self.codec.samples_per_frame();
            self.bytes_per_frame = self.codec.bytes_per_frame();
        }

        let encoded = &frame.payload[1..];
        if encoded.len() % self.bytes_per_frame != 0 {
            return Err(Codec2CodecError::MisalignedPayload {
                payload_len: encoded.len(),
                bytes_per_frame: self.bytes_per_frame,
            });
        }

        let subframe_count = encoded.len() / self.bytes_per_frame;
        let mut pcm = Vec::with_capacity(subframe_count * self.samples_per_frame);
        let mut speech = vec![0i16; self.samples_per_frame];
        let mut bits = vec![0u8; self.bytes_per_frame];
        for subframe_index in 0..subframe_count {
            let start = subframe_index * self.bytes_per_frame;
            let end = start + self.bytes_per_frame;
            bits.copy_from_slice(&encoded[start..end]);
            self.codec.decode(&mut speech, &bits);
            pcm.extend_from_slice(&speech);
        }

        if pcm.len() != self.sample_frames {
            return Err(Codec2CodecError::SampleFrameMismatch {
                expected: self.sample_frames,
                actual: pcm.len(),
            });
        }

        RawAudioFrame::new(self.channels, pcm_to_float(&pcm)).map_err(Codec2CodecError::Wire)
    }
}

fn codec2_mode_for_profile(profile: Profile) -> Result<Codec2Mode, Codec2CodecError> {
    match profile.audio_codec() {
        AudioCodec::Codec2(mode) => Ok(mode),
        AudioCodec::Opus(_) => Err(Codec2CodecError::NonCodec2Profile(profile)),
    }
}

fn pure_rust_mode(mode: Codec2Mode) -> Option<codec2::Codec2Mode> {
    match mode {
        Codec2Mode::Mode1200 => Some(codec2::Codec2Mode::MODE_1200),
        Codec2Mode::Mode1300 => Some(codec2::Codec2Mode::MODE_1300),
        Codec2Mode::Mode1400 => Some(codec2::Codec2Mode::MODE_1400),
        Codec2Mode::Mode1600 => Some(codec2::Codec2Mode::MODE_1600),
        Codec2Mode::Mode2400 => Some(codec2::Codec2Mode::MODE_2400),
        Codec2Mode::Mode3200 => Some(codec2::Codec2Mode::MODE_3200),
        Codec2Mode::Mode700C => None,
    }
}

fn frame_to_pcm(frame: &RawAudioFrame, channels: u8) -> Result<Vec<i16>, Codec2CodecError> {
    let samples = if frame.channels > channels {
        frame
            .samples
            .chunks_exact(usize::from(frame.channels))
            .map(|chunk| chunk[1])
            .collect::<Vec<_>>()
    } else {
        frame.samples.clone()
    };
    Ok(samples
        .iter()
        .map(|sample| (sample.clamp(-1.0, 1.0) * TYPE_MAP_FACTOR) as i16)
        .collect())
}

fn pcm_to_float(samples: &[i16]) -> Vec<f32> {
    samples
        .iter()
        .map(|sample| f32::from(*sample) / TYPE_MAP_FACTOR)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SyntheticSourceKind;

    fn source_for(profile: Profile) -> crate::SyntheticSource {
        crate::SyntheticSource::new(
            profile.channels(),
            profile.sample_rate_hz(),
            profile.sample_frames_per_packet(),
            SyntheticSourceKind::Sine {
                frequency_hz: 440.0,
                amplitude: 0.25,
            },
        )
        .unwrap()
    }

    #[test]
    fn codec2_encoder_rejects_opus_profiles() {
        assert!(matches!(
            Codec2EncoderState::new(Profile::QualityMedium),
            Err(Codec2CodecError::NonCodec2Profile(Profile::QualityMedium))
        ));
    }

    #[test]
    fn codec2_roundtrip_decodes_profile_shaped_pcm() {
        for profile in [Profile::BandwidthVeryLow, Profile::BandwidthLow] {
            let mut source = source_for(profile);
            let frame = source.next_raw_frame().unwrap();
            let mut encoder = Codec2EncoderState::new(profile).unwrap();
            let mut decoder = Codec2DecoderState::new(profile).unwrap();

            let encoded = encoder.encode_frame(&frame).unwrap();
            assert_eq!(encoded.codec, CodecKind::Codec2);
            assert_eq!(encoded.payload[0], encoder.mode().header());

            let decoded = decoder.decode_frame(&encoded).unwrap();
            assert_eq!(decoded.channels, profile.channels());
            assert_eq!(decoded.sample_frames(), profile.sample_frames_per_packet());
            assert_eq!(decoder.profile(), profile);
        }
    }

    #[test]
    fn codec2_payload_includes_mode_header_and_subframes() {
        let profile = Profile::BandwidthLow;
        let mut encoder = Codec2EncoderState::new(profile).unwrap();
        let frame = source_for(profile).next_raw_frame().unwrap();
        let encoded = encoder.encode_frame(&frame).unwrap();
        let mode = profile.audio_codec();
        let AudioCodec::Codec2(mode) = mode else {
            panic!("expected codec2 profile");
        };
        let expected_len = 1
            + (profile.sample_frames_per_packet() / mode.samples_per_frame())
                * mode.bytes_per_frame();
        assert_eq!(encoded.payload.len(), expected_len);
    }

    #[cfg(feature = "libcodec2")]
    #[test]
    fn codec2_700c_roundtrip_with_libcodec2() {
        let profile = Profile::BandwidthUltraLow;
        let mut source = source_for(profile);
        let frame = source.next_raw_frame().unwrap();
        let mut encoder = Codec2EncoderState::new(profile).unwrap();
        let mut decoder = Codec2DecoderState::new(profile).unwrap();

        let encoded = encoder.encode_frame(&frame).unwrap();
        let decoded = decoder.decode_frame(&encoded).unwrap();
        assert_eq!(decoded.sample_frames(), profile.sample_frames_per_packet());
    }

    #[cfg(not(feature = "libcodec2"))]
    #[test]
    fn codec2_700c_requires_libcodec2_feature() {
        assert!(matches!(
            Codec2EncoderState::new(Profile::BandwidthUltraLow),
            Err(Codec2CodecError::UnsupportedMode(Codec2Mode::Mode700C))
        ));
    }
}
