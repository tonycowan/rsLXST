use std::path::{Path, PathBuf};
use std::process::Command;

use lxst_core::{
    AudioCodec, Codec2DecoderState, Codec2EncoderState, Codec2Mode, CodecKind, Frame, Profile,
    RawAudioFrame,
};
use serde_json::Value;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crate is under rsLXST/crates/lxst-core")
        .to_path_buf()
}

fn fixture_script() -> PathBuf {
    repo_root().join("tools/fixtures/lxst_codec2_fixtures.py")
}

fn python_interpreter() -> String {
    std::env::var("PYTHON").unwrap_or_else(|_| {
        if cfg!(windows) {
            "python".to_string()
        } else {
            "python3".to_string()
        }
    })
}

fn python_fixtures() -> Vec<Value> {
    let output = Command::new(python_interpreter())
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .arg(fixture_script())
        .output()
        .expect("spawn Python Codec2 fixture generator");

    assert!(
        output.status.success(),
        "Python Codec2 fixture generator failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    serde_json::from_slice(&output.stdout).expect("fixture JSON")
}

fn decode_with_python(payload_hex: &str) -> Value {
    let output = Command::new(python_interpreter())
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .arg(fixture_script())
        .arg("--decode-hex")
        .arg(payload_hex)
        .output()
        .expect("spawn Python Codec2 fixture decoder");

    assert!(
        output.status.success(),
        "Python Codec2 fixture decoder failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    serde_json::from_slice(&output.stdout).expect("decode JSON")
}

fn profile_from_fixture(fixture: &Value) -> Profile {
    Profile::from_wire(fixture["profile_wire"].as_u64().expect("profile wire") as u32)
        .expect("known profile")
}

#[test]
fn python_codec2_payload_shape_matches_rust() {
    for fixture in python_fixtures() {
        let name = fixture["name"].as_str().expect("name");
        let profile = profile_from_fixture(&fixture);
        let payload_hex = fixture["payload_hex"].as_str().expect("payload hex");
        let payload = hex::decode(payload_hex).expect("payload bytes");
        let mode_header = fixture["mode_header"].as_u64().expect("mode header") as u8;

        let frame = Frame::new(CodecKind::Codec2, payload.clone());
        assert_eq!(frame.payload[0], mode_header, "{name}");

        let mode = match profile.audio_codec() {
            AudioCodec::Codec2(mode) => mode,
            other => panic!("{name} expected Codec2 profile, got {other:?}"),
        };
        let expected_len = 1
            + (profile.sample_frames_per_packet() / mode.samples_per_frame())
                * mode.bytes_per_frame();
        assert_eq!(payload.len(), expected_len, "{name}");
        assert_eq!(
            fixture["samples_per_packet"].as_u64().expect("samples"),
            profile.sample_frames_per_packet() as u64,
            "{name}"
        );
    }
}

#[test]
fn rust_codec2_roundtrip_matches_python_decode_shape() {
    for fixture in python_fixtures() {
        let name = fixture["name"].as_str().expect("name");
        let profile = profile_from_fixture(&fixture);
        let samples_per_packet = profile.sample_frames_per_packet();
        let pcm = vec![0.0f32; samples_per_packet];
        let frame = RawAudioFrame::new(profile.channels(), pcm).expect("raw frame");

        let mut encoder = Codec2EncoderState::new(profile).expect("encoder");
        let encoded = encoder.encode_frame(&frame).expect("encode");
        let decoded_python = decode_with_python(&hex::encode(&encoded.payload));

        let mut decoder = Codec2DecoderState::new(profile).expect("decoder");
        let decoded_rust = decoder.decode_frame(&encoded).expect("decode");

        assert_eq!(
            decoded_rust.channels,
            decoded_python["channels"].as_u64().expect("channels") as u8,
            "{name}"
        );
        assert_eq!(
            decoded_rust.sample_frames(),
            decoded_python["samples"].as_u64().expect("samples") as usize,
            "{name}"
        );
    }
}

#[test]
fn rust_decodes_python_codec2_payloads() {
    for fixture in python_fixtures() {
        let name = fixture["name"].as_str().expect("name");
        let profile = profile_from_fixture(&fixture);
        let payload = hex::decode(fixture["payload_hex"].as_str().expect("payload hex"))
            .expect("payload bytes");
        let frame = Frame::new(CodecKind::Codec2, payload);

        let mut decoder = Codec2DecoderState::new(profile).expect("decoder");
        let decoded = decoder.decode_frame(&frame).expect("decode");
        assert_eq!(decoded.channels, profile.channels(), "{name}");
        assert_eq!(
            decoded.sample_frames(),
            profile.sample_frames_per_packet(),
            "{name}"
        );
    }
}

#[test]
fn codec2_mode_constants_match_python_headers() {
    assert_eq!(Codec2Mode::Mode700C.header(), 0x00);
    assert_eq!(Codec2Mode::Mode1600.header(), 0x04);
    assert_eq!(Codec2Mode::Mode3200.header(), 0x06);
    assert_eq!(Codec2Mode::Mode1600.bytes_per_frame(), 8);
    assert_eq!(Codec2Mode::Mode3200.samples_per_frame(), 160);
}
