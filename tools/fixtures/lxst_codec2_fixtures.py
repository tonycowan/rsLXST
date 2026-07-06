#!/usr/bin/env python3
"""Generate and decode LXST Codec2 fixtures with the Python reference stack."""

from __future__ import annotations

import argparse
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "common"))
from lxst_reference import import_rns_lxst, json_dump  # noqa: E402

# Wire values and packet shapes mirror lxst-core Profile constants.
CASES = [
    {
        "name": "bandwidth_very_low",
        "profile_wire": 0x20,
        "codec2_mode": 1600,
        "samples_per_packet": 2560,
        "channels": 1,
    },
    {
        "name": "bandwidth_low",
        "profile_wire": 0x30,
        "codec2_mode": 3200,
        "samples_per_packet": 1600,
        "channels": 1,
    },
]


def generate_fixtures():
    _RNS, LXST, _stubbed = import_rns_lxst()
    import numpy as np

    Codec2 = LXST.Codecs.Codec2
    fixtures = []
    for case in CASES:
        codec = Codec2(case["codec2_mode"])
        pcm = np.zeros((case["samples_per_packet"], case["channels"]), dtype="float32")
        encoded = codec.encode(pcm)
        fixtures.append(
            {
                "name": case["name"],
                "profile_wire": case["profile_wire"],
                "mode_header": encoded[0],
                "payload_hex": encoded.hex(),
                "payload_len": len(encoded),
                "samples_per_packet": case["samples_per_packet"],
                "channels": case["channels"],
            }
        )

    return fixtures


def decode_payload(payload_hex: str):
    _RNS, LXST, _stubbed = import_rns_lxst()

    payload = bytes.fromhex(payload_hex)
    Codec2 = LXST.Codecs.Codec2
    codec = Codec2(Codec2.CODEC2_1600)
    decoded = codec.decode(payload)
    return {
        "channels": int(decoded.shape[1]),
        "samples": int(decoded.shape[0]),
        "sample_values": [float(value) for value in decoded[:, 0].tolist()],
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--decode-hex", metavar="PAYLOAD_HEX")
    args = parser.parse_args()

    if args.decode_hex:
        json_dump(decode_payload(args.decode_hex))
        return

    json_dump(generate_fixtures())


if __name__ == "__main__":
    main()
