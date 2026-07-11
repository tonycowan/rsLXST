<div align="center">

# rsLXST

**Rust LXST telephony and media streaming for Reticulum.**

[![License: AGPL-3.0-or-later](https://img.shields.io/badge/license-AGPL--3.0--or--later-blue.svg)](LICENSE)
[![Rust 1.85+](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)
[![LXST 0.4.5](https://img.shields.io/badge/target-LXST%200.4.5-success.svg)](https://github.com/markqvist/LXST)
[![Status](https://img.shields.io/badge/status-library-yellow.svg)](#feature-status)

[rsLXMF](https://github.com/ratspeak/rsLXMF) |
[Ratspeak](https://github.com/ratspeak/Ratspeak) |
[rsReticulum](https://github.com/ratspeak/rsReticulum) |
[Reticulum Manual](https://reticulum.network/manual/) 

</div>

---

rsLXST is a Rust implementation of [LXST](https://github.com/markqvist/LXST), the Lightweight Extensible Signal
Transport used for real-time voice calls and other media streams over Reticulum. This is not a
fork of LXST; it is LXST written in a different language with interoperability
as the primary focus. Python LXST remains the source-of-truth
implementation, do not treat this repository as one.

The current rsLXST is experimental and incomplete. It provides LXST wire codecs,
Reticulum link media packet boundaries, a telephony runtime, Opus and Codec2
stream integration, and consensus-based adaptive profile negotiation for
applications (such as Ratspeak).

The first public target is interoperable voice telephony over Reticulum links —
Codec2 for narrowband mesh paths and Opus for higher-quality links — not
complete feature parity with the reference implementation LXST.

## Contents

- [Release Scope](#release-scope)
- [Voice Profiles](#voice-profiles)
- [Adaptive Profile Negotiation](#adaptive-profile-negotiation)
- [Build It](#build-it)
- [Test It](#test-it)
- [Crate Layout](#crate-layout)
- [Using Telephony](#using-telephony)
- [Contributing](#contributing)
- [License](#license)

## Release Scope

The experimental release covers basic voice calls over Reticulum links:

- Opus and Codec2 encode/decode, stream packetization, and jitter buffers.
- LXST telephony signalling, call state, and profile switches mid-call.
- Consensus-based profile upgrades (`UpgradeProposal` / `UpgradeAccept`) and
  immediate downgrades (`PreferredProfile`).
- Python LXST wire parity and live headless interop tests.

Still unsupported or incomplete:

- `rnphone` parity and `rnphone-rs` usage.
- Deeper audio support: microphone/source backends, filters, AGC, etc.
- Broadcast, stream, and non-telephony LXST primitives.
- Application-side adaptive negotiation policy (Ratspeak implements this on
  top of the telephony controls documented below).

Those gaps are expected future work. They should not be implied by the first
public voice telephony release.

## Voice Profiles

LXST telephony profiles map to a codec, frame time, and bandwidth target. The
profiles most relevant to mesh voice are:

| Profile | Abbrev. | Codec | Frame time |
| --- | --- | --- | --- |
| `BandwidthUltraLow` | ULBW | Codec2 700C | 400 ms |
| `BandwidthVeryLow` | VLBW | Codec2 1600 | 320 ms |
| `BandwidthLow` | LBW | Codec2 3200 | 200 ms |
| `QualityMedium` | MQ | Opus voice (mono) | 60 ms |
| `QualityHigh` | HQ | Opus voice (mono, higher bitrate) | 60 ms |
| `QualityMax` | SHQ | Opus voice (stereo) | 60 ms |

Latency-oriented Opus profiles (`LatencyLow`, `LatencyUltraLow`) exist for
reference parity but are not part of the adaptive climb ladder Ratspeak uses
today.

Codec2 modes 1200–3200 use the pure-Rust `codec2` crate. Mode 700C requires the
optional `libcodec2` feature and a system `libcodec2` install (see [Using
Telephony](#using-telephony)).

## Adaptive Profile Negotiation

rsLXST exposes the wire protocol and telephony controls for mid-call quality
changes. **Policy** — when to climb, when to fall back, and health thresholds
— lives in the application. [Ratspeak](https://github.com/ratspeak/Ratspeak)
is the reference consumer.

### Signalling

Three LXST wire signals drive negotiation:

| Signal | Direction | Purpose |
| --- | --- | --- |
| `PreferredProfile` | Either peer, immediate | Switch to a lower (or equal) profile. Used for congestion downgrades. |
| `UpgradeProposal` | Proposer → peer | Offer a single-step climb to the next profile on the ladder. |
| `UpgradeAccept` | Peer → proposer | Accept a pending proposal; both sides switch together. |

Wire values are `0x180 + profile` for proposals and `0x210 + profile` for
accepts, where `profile` is the LXST profile wire ID (for example `0x20` for
`BandwidthVeryLow` / Codec2 1600).

The legacy `UpgradePermission` signal (`0xFE`) is still decoded for reference
parity but is not used by current Ratspeak builds.

### Telephony API

Applications drive negotiation through `TelephonyControl` and observe it on
`TelephonyServiceEvent`:

```rust
// Downgrade (or equal-profile switch) — sends PreferredProfile
TelephonyControl::SwitchProfile { profile }

// Consensus upgrade — proposer sends UpgradeProposal, peer replies AcceptUpgrade
TelephonyControl::SendUpgradeProposal { profile }
TelephonyControl::AcceptUpgrade { profile }
```

Matching events: `SwitchProfile`, `UpgradeProposalReceived`, and
`UpgradeAcceptReceived`. After an accepted upgrade or a `PreferredProfile`
downgrade, the service tears down the active encode/decode stream and expects
the application to restart capture/playback at the new profile.

### Ratspeak adaptive ladder (reference policy)

Ratspeak starts calls at **Codec2 1600** (`BandwidthVeryLow`) and climbs only
when both sides agree the path is healthy:

```text
Codec2 1600 → Codec2 3200 → Opus MQ → Opus HQ
```

The adaptive path stops at Opus HQ. Opus Max remains available for manual or
environment overrides (`RATSPEAK_VOICE_PROFILE`) but is not proposed
automatically — HQ is mono 16 kbps and a better fit for telephony and mesh
airtime than stereo Opus Max.

| Behaviour | Detail |
| --- | --- |
| **Upgrades** | Consensus only. The side holding the upgrade token proposes after ~3 s stable at the current tier; the peer accepts if its local path health is OK. ~2 s cooldown between switches; proposals time out after 10 s. |
| **Downgrades** | Either side can drop immediately via `PreferredProfile` when it sees sustained congestion: ≥4 dropped frames, transport queue pressure, or playback underruns (~50 ms @ 48 kHz) after a 2 s grace period following a profile switch. |
| **Token** | Callee holds the upgrade token at call start. After a downgrade, the downgrading side holds it; after a successful upgrade, it returns to the callee. |

On fast WiFi links, calls typically reach Opus HQ within ~15 s. On constrained
LoRa paths, the ladder may attempt higher tiers but usually settles back at
Codec2 1600 when Opus playback cannot keep up — that is expected path
behaviour, not a protocol failure. There is no per-link-type configuration;
the same policy runs on every interface.

## Build It

The current development layout requires `rsReticulum` as a sibling checkout
because rsLXST uses the Rust Reticulum crates directly:

```text
ratspeak-src/
|-- rsReticulum/
`-- rsLXST/
```

If you're starting fresh:

```bash
mkdir ratspeak-src
cd ratspeak-src
git clone https://github.com/ratspeak/rsReticulum
git clone https://github.com/ratspeak/rsLXST
cd rsLXST
```

### macOS

Install Rust with `rustup`, then install Apple's command-line build tools:

```bash
xcode-select --install
```

Build the workspace:

```bash
cd rsLXST
cargo build --release
```

### Linux / Raspberry Pi

Install Rust with `rustup`, then install the usual build packages.

Debian, Ubuntu, and Raspberry Pi OS:

```bash
sudo apt update
sudo apt install -y build-essential pkg-config
```

Fedora:

```bash
sudo dnf install gcc make pkgconf-pkg-config
```

Arch:

```bash
sudo pacman -S --needed base-devel pkgconf
```

Build the workspace:

```bash
cd rsLXST
cargo build --release
```

### Windows

Install Rust with the MSVC toolchain. If Rust or Cargo asks for Visual Studio
Build Tools, install the "Desktop development with C++" workload.

Build from PowerShell:

```powershell
cd rsLXST
cargo build --release
```

## Test It

Run the workspace test gate:

```bash
cargo test --workspace
```

Run the local CI gate:

```bash
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

The test gate covers wire codecs, telephony state, profile metadata, Opus
and Codec2 stream boundaries, malformed-input handling, the local service runtime, Python
LXST wire parity, Reticulum destination parity, and live headless LXST
Telephone interop. The Python tests expect upstream LXST at `../upstream/LXST`
or `LXST_UPSTREAM_DIR`, and upstream Reticulum at `../upstream/Reticulum`,
`RETICULUM_UPSTREAM_DIR`, or sibling `../rsReticulum`.

For full Python Opus media interop, install a native Opus runtime as well as
the Python reference dependencies:

```bash
python -m pip install numpy cryptography pyserial cffi
```

## Crate Layout

| Crate | Purpose |
| --- | --- |
| `lxst-core` | LXST constants, telephony profiles, signalling values, codec IDs, MessagePack packets, Raw audio frames, Opus and Codec2 encode/decode state, stream packetization, synthetic sources, and jitter buffers. This crate has no Reticulum runtime dependency. |
| `lxst-rns` | The Reticulum link-packet boundary for no-receipt LXST signalling and media over active links. It packs outbound LXST packets and decodes inbound link plaintext into typed LXST packet/frame events. |
| `lxst-telephony` | The telephony runtime and service layer. It owns call state, caller policy, Reticulum destination registration, announce discovery, outgoing link establishment, typed control/event channels, Opus and Codec2 transmit/receive stream boundaries, timeout handling, and shutdown teardown. |

## Using Telephony

Applications normally use `lxst-telephony` through `TelephonyService`, not by
manually translating Reticulum events. The service registers the local
`lxst.telephony` destination, emits startup/periodic announces, owns the call
runtime, and exposes typed control and event channels.

```rust
use lxst_core::Profile;
use lxst_telephony::{TelephonyControl, TelephonyService};
use tokio::time::Duration;

let parts = TelephonyService::registered(transport_tx, &identity)?;
let control_tx = parts.control_tx.clone();
let mut event_rx = parts.event_rx;

tokio::spawn(parts.service.run());

control_tx
    .send(TelephonyControl::Call {
        remote_identity,
        profile: Some(Profile::QualityMedium),
        discovery_timeout: Duration::from_secs(8),
    })
    .await?;
```

The service event stream is the app-facing state source. Use
`TelephonyServiceEvent::Snapshot`, `IncomingCall`, `OutgoingCallPending`,
`OutgoingCallStarted`, `OutgoingCallFailed`, `CallTerminated`, stream lifecycle
events, profile negotiation events (`SwitchProfile`, `UpgradeProposalReceived`,
`UpgradeAcceptReceived`), and media events instead of inferring call state from
raw Reticulum traffic. Outgoing announce/path discovery runs asynchronously
inside the service, so an unreachable or non-LXST peer does not block hangup,
announce, media, or shutdown controls while discovery times out.

For Opus calls, applications supply and receive `RawAudioFrame` values through
`StartOpusStream` and `StartOpusReceiveStream`. For bandwidth profiles that use
Codec2, use `StartCodec2Stream` and `StartCodec2ReceiveStream` (or the one-shot
`SendCodec2Frames` control). rsLXST enforces the negotiated LXST profile and
reports profile changes, source/sink closure, frame drops, and call-end stream
shutdown explicitly.

Codec2 modes 1200–3200 use the pure-Rust `codec2` crate. Mode 700C requires the
optional `libcodec2` feature and a system `libcodec2` install:

```bash
# Debian/Ubuntu/Raspberry Pi OS
sudo apt install libcodec2-dev pkg-config

# macOS
brew install codec2 pkg-config

cargo test -p lxst-core --features libcodec2
```

If headers or libraries live outside the usual paths, set `CODEC2_INCLUDE_DIR`
and `CODEC2_LIBRARY_DIR` before building.

Applications still own platform integration:

- contact or peer lookup
- UI and call controls
- microphone/camera/speaker permissions
- device selection
- audio session lifecycle
- capture/playback and resampling into `RawAudioFrame`
- settings persistence
- mobile foreground/background behavior

Ratspeak uses this boundary for its native voice-call feature.

## Contributing

If the issue or contribution belongs upstream as well, start there. Python LXST
and Reticulum remain the reference implementations.

PRs are closed for now until I have time to catch up on everything.

## License

Licensed under the GNU Affero General
Public License v3.0 or later. See [LICENSE](LICENSE).
