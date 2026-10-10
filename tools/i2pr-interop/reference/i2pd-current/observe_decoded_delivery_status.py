#!/usr/bin/env python3
"""Sanitize the pinned i2pd debug log's inbound DeliveryStatus observation."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import sys
import tempfile

MAX_LOG_BYTES = 8 * 1024 * 1024
PIN = "635b013a612ff47278ef02acf8580a28e10e26c5"
TYPE_10 = re.compile(r"(?:^|\s)I2NP: Handling message with type 10\s*$")
DECRYPTED = re.compile(r"(?:^|\s)NTCP2: Received message decrypted\s*$")
I2NP_BLOCK = re.compile(r"(?:^|\s)NTCP2: I2NP\s*$")
DIRECTIONS = {"i2pr-to-i2pd-ipv4", "i2pd-to-i2pr-ipv4"}


class ObservationError(ValueError):
    pass


def observe(raw: bytes) -> dict[str, object]:
    if not raw or len(raw) > MAX_LOG_BYTES:
        raise ObservationError("log-size-invalid")
    try:
        text = raw.decode("utf-8", errors="strict")
    except UnicodeDecodeError as exc:
        raise ObservationError("log-encoding-invalid") from exc

    lines = text.splitlines()
    type10_lines = [index for index, line in enumerate(lines) if TYPE_10.search(line)]
    decrypted = sum(bool(DECRYPTED.search(line)) for line in lines)
    block_lines = [index for index, line in enumerate(lines) if I2NP_BLOCK.search(line)]
    type10 = len(type10_lines)
    if type10 != 1:
        raise ObservationError("decoded-delivery-status-count-not-one")
    if decrypted < 1 or len(block_lines) != 1 or block_lines[0] >= type10_lines[0]:
        raise ObservationError("ntcp2-decode-context-missing")

    return {
        "schema": "i2pr-stock-i2pd-decoded-i2np-observation-v1",
        "reference_revision": PIN,
        "i2np_type": 10,
        "decoded_delivery_status_count": 1,
        "decrypted_frame_count": min(decrypted, 0xFFFF),
        "i2np_block_count": len(block_lines),
        "log_sha256": hashlib.sha256(raw).hexdigest(),
        "result": "observed",
    }


def self_test() -> None:
    fixture = (
        b"2026-10-10 12:00:00 [Debug] NTCP2: Received message decrypted\n"
        b"2026-10-10 12:00:00 [Debug] NTCP2: I2NP\n"
        b"2026-10-10 12:00:00 [Debug] I2NP: Handling message with type 10\n"
    )
    result = observe(fixture)
    assert result["result"] == "observed"
    assert result["decoded_delivery_status_count"] == 1
    assert result["i2np_type"] == 10

    invalid = [
        b"",
        fixture.replace(b"type 10", b"type 2"),
        fixture + fixture,
        fixture.replace(b"NTCP2: I2NP", b"NTCP2: Options"),
        fixture + b"\xff",
        b"x" * (MAX_LOG_BYTES + 1),
    ]
    for candidate in invalid:
        try:
            observe(candidate)
        except ObservationError:
            continue
        raise AssertionError("invalid observation fixture was accepted")

    with tempfile.TemporaryDirectory(prefix="plan414-observer-") as raw_root:
        root = Path(raw_root)
        log = root / "i2pd.log"
        evidence = root.parent / (root.name + ".json")
        log.write_bytes(fixture)
        consume_log(log, evidence, root, "i2pr-to-i2pd-ipv4")
        assert not log.exists()
        record = json.loads(evidence.read_text(encoding="utf-8"))
        assert record["direction"] == "i2pr-to-i2pd-ipv4"
        evidence.unlink()

        bad_log = root / "bad.log"
        bad_log.write_bytes(fixture + fixture)
        try:
            consume_log(bad_log, evidence, root, "i2pr-to-i2pd-ipv4")
        except ObservationError:
            pass
        else:
            raise AssertionError("ambiguous log was accepted")
        assert bad_log.exists(), "failed parse must not consume source before classification"


def consume_log(log_path: Path, output_path: Path, owned_root: Path, direction: str) -> None:
    if direction not in DIRECTIONS:
        raise ObservationError("direction-not-allowlisted")
    root = owned_root.resolve(strict=True)
    if log_path.is_symlink():
        raise ObservationError("log-path-not-owned")
    log = log_path.resolve(strict=True)
    if not log.is_file() or not log.is_relative_to(root):
        raise ObservationError("log-path-not-owned")
    output = output_path.resolve(strict=False)
    if output == log or output.is_relative_to(root) or output_path.is_symlink():
        raise ObservationError("output-path-not-separated")
    raw = log.read_bytes()
    record = observe(raw)
    record["direction"] = direction
    encoded = (json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n").encode()
    temp = output.with_name(output.name + ".tmp")
    temp.write_bytes(encoded)
    temp.replace(output)
    log.unlink()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--consume-log", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--owned-root", type=Path)
    parser.add_argument("--direction")
    args = parser.parse_args()

    try:
        if args.self_test:
            if any((args.consume_log, args.output, args.owned_root, args.direction)):
                raise ObservationError("self-test-arguments-conflict")
            self_test()
            print("plan414 observer self-test passed")
            return 0
        if not all((args.consume_log, args.output, args.owned_root, args.direction)):
            raise ObservationError("consume-arguments-required")
        consume_log(args.consume_log, args.output, args.owned_root, args.direction)
        return 0
    except (OSError, ObservationError) as exc:
        # Emit only a closed reason category; never echo paths or source bytes.
        reason = str(exc) if isinstance(exc, ObservationError) else "io-failed"
        print(json.dumps({"result": "rejected", "reason_code": reason}, separators=(",", ":")))
        return 2


if __name__ == "__main__":
    sys.exit(main())

