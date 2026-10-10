#!/usr/bin/env python3
"""Sanitize the pinned i2pd debug log's inbound DeliveryStatus observation."""

from __future__ import annotations

import argparse
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
STAGE_PATTERNS = {
    "session_request_received_count": re.compile(r"(?:^|\s)NTCP2: SessionRequest (?:received|updated) \d+\s*$"),
    "session_created_received_count": re.compile(r"(?:^|\s)NTCP2: SessionCreated received \d+\s*$"),
    "session_confirmed_received_count": re.compile(r"(?:^|\s)NTCP2: SessionConfirmed received\s*$"),
    "session_confirmed_sent_count": re.compile(r"(?:^|\s)NTCP2: SessionConfirmed sent\s*$"),
    "session_request_aead_failure_count": re.compile(r"(?:^|\s)NTCP2: SessionRequest AEAD verification failed\s*$"),
    "session_created_aead_failure_count": re.compile(r"(?:^|\s)NTCP2: SessionCreated AEAD verification failed\s*$"),
    "session_confirmed_aead_failure_count": re.compile(r"(?:^|\s)NTCP2: SessionConfirmed Part[12] AEAD verification failed\s*$"),
    "session_confirmed_part2_kdf_failure_count": re.compile(r"(?:^|\s)NTCP2: SessionConfirmed Part2 KDF failed\s*$"),
    "session_confirmed_unexpected_block_count": re.compile(r"(?:^|\s)NTCP2: Unexpected block \d+ in SessionConfirmed\s*$"),
    "session_confirmed_unexpected_router_info_size_count": re.compile(r"(?:^|\s)NTCP2: Unexpected RouterInfo size \d+ in SessionConfirmed\s*$"),
    "session_confirmed_router_info_verification_failure_count": re.compile(r"(?:^|\s)NTCP2: RouterInfo verification failed in SessionConfirmed from .+$"),
    "session_confirmed_router_info_too_old_count": re.compile(r"(?:^|\s)NTCP2: RouterInfo is too old in SessionConfirmed for \d+ seconds\s*$"),
    "session_confirmed_router_info_from_future_count": re.compile(r"(?:^|\s)NTCP2: RouterInfo is from future for \d+ seconds\s*$"),
    "session_confirmed_router_version_too_old_count": re.compile(r"(?:^|\s)NTCP2: Router version \d+\.\d+\.\d+ is too old in SessionConfirmed\s*$"),
    "session_confirmed_router_info_update_failure_count": re.compile(r"(?:^|\s)NTCP2: Couldn't update RouterInfo from SessionConfirmed in netdb\s*$"),
    "session_confirmed_address_not_found_count": re.compile(r"(?:^|\s)NTCP2: Address not found in SessionConfirmed\s*$"),
    "session_confirmed_host_mismatch_count": re.compile(r"(?:^|\s)NTCP2: Host mismatch between published address .+ and actual endpoint .+$"),
    "session_confirmed_wrong_static_key_count": re.compile(r"(?:^|\s)NTCP2: Wrong static key in SessionConfirmed\s*$"),
    "session_confirmed_router_info_accepted_count": re.compile(r"(?:^|\s)NTCP2: SessionConfirmed from .+$"),
    "ntcp2_session_terminated_count": re.compile(r"(?:^|\s)NTCP2: Session with .+ terminated\s*$"),
}
MAX_LINE_BYTES = 4096
DIRECTIONS = {"i2pr-to-i2pd-ipv4", "i2pd-to-i2pr-ipv4"}


class ObservationError(ValueError):
    pass


def observe(raw: bytes, baseline_offset: int = 0) -> dict[str, object]:
    if not raw or len(raw) > MAX_LOG_BYTES:
        raise ObservationError("log-size-invalid")
    if not isinstance(baseline_offset, int) or baseline_offset < 0 or baseline_offset > len(raw):
        raise ObservationError("baseline-offset-invalid")
    try:
        text = raw[baseline_offset:].decode("utf-8", errors="strict")
    except UnicodeDecodeError as exc:
        raise ObservationError("log-encoding-invalid") from exc

    lines = text.splitlines()
    if any(len(line.encode("utf-8")) > MAX_LINE_BYTES for line in lines):
        raise ObservationError("log-line-too-large")
    type10_lines = [index for index, line in enumerate(lines) if TYPE_10.search(line)]
    decrypted = sum(bool(DECRYPTED.search(line)) for line in lines)
    block_lines = [index for index, line in enumerate(lines) if I2NP_BLOCK.search(line)]
    counts = {name: min(sum(bool(pattern.search(line)) for line in lines), 0xFFFF)
              for name, pattern in STAGE_PATTERNS.items()}
    counts.update({
        "decoded_delivery_status_count": min(len(type10_lines), 0xFFFF),
        "decrypted_frame_count": min(decrypted, 0xFFFF),
        "i2np_block_count": min(len(block_lines), 0xFFFF),
    })
    reason = None
    if len(type10_lines) != 1:
        reason = "decoded-delivery-status-count-not-one"
    elif decrypted < 1 or len(block_lines) != 1 or block_lines[0] >= type10_lines[0]:
        reason = "ntcp2-decode-context-missing"
    return {
        "schema": "i2pr-stock-i2pd-stage-observation-v2",
        "reference_revision": PIN,
        **counts,
        "result": "rejected" if reason else "observed",
        **({"reason_code": reason} if reason else {}),
    }


def self_test() -> None:
    fixture = (
        b"2026-10-10 12:00:00 [Debug] NTCP2: Received message decrypted\n"
        b"2026-10-10 12:00:00 [Debug] NTCP2: I2NP\n"
        b"2026-10-10 12:00:00 [Debug] I2NP: Handling message with type 10\n"
    )
    fixture = (
        b"pre-baseline NTCP2: SessionRequest received 99\n"
        b"pre-baseline NTCP2: SessionConfirmed received\n" + fixture
    )
    baseline = fixture.index(b"2026-10-10")
    result = observe(fixture, baseline)
    assert result["result"] == "observed"
    assert result["decoded_delivery_status_count"] == 1
    assert result["i2np_block_count"] == 1
    assert result["decrypted_frame_count"] == 1
    assert "log_sha256" not in result
    assert all(result[name] == 0 for name in (
        "session_request_received_count", "session_created_received_count",
        "session_confirmed_received_count", "session_confirmed_sent_count",
    ))
    stages = (
        b"2026-10-10 NTCP2: SessionRequest received 64\n"
        b"2026-10-10 NTCP2: SessionCreated received 80\n"
        b"2026-10-10 NTCP2: SessionConfirmed received\n"
        b"2026-10-10 NTCP2: SessionConfirmed sent\n"
    )
    stage_result = observe(stages)
    assert stage_result["session_request_received_count"] == 1
    assert stage_result["session_created_received_count"] == 1
    assert stage_result["session_confirmed_received_count"] == 1
    assert stage_result["session_confirmed_sent_count"] == 1
    failure_markers = (
        b"NTCP2: SessionConfirmed Part1 AEAD verification failed \n",
        b"NTCP2: SessionConfirmed Part2 KDF failed\n",
        b"NTCP2: SessionConfirmed Part2 AEAD verification failed \n",
        b"NTCP2: Unexpected block 7 in SessionConfirmed\n",
        b"NTCP2: Unexpected RouterInfo size 123 in SessionConfirmed\n",
        b"NTCP2: RouterInfo verification failed in SessionConfirmed from 127.0.0.1:1234\n",
        b"NTCP2: RouterInfo is too old in SessionConfirmed for 5401 seconds\n",
        b"NTCP2: RouterInfo is from future for 121 seconds\n",
        b"NTCP2: Router version 0.9.68 is too old in SessionConfirmed\n",
        b"NTCP2: Couldn't update RouterInfo from SessionConfirmed in netdb\n",
        b"NTCP2: Address not found in SessionConfirmed\n",
        b"NTCP2: Host mismatch between published address 127.0.0.1 and actual endpoint 127.0.0.2\n",
        b"NTCP2: Wrong static key in SessionConfirmed\n",
    )
    baseline_prefix = b"pre-baseline NTCP2: Wrong static key in SessionConfirmed\n"
    failure_result = observe(baseline_prefix + b"".join(failure_markers), len(baseline_prefix))
    failure_names = tuple(name for name in STAGE_PATTERNS if name.startswith("session_confirmed_")
                          and name not in {"session_confirmed_received_count", "session_confirmed_sent_count",
                                           "session_confirmed_router_info_accepted_count"})
    assert all(failure_result[name] == (2 if name == "session_confirmed_aead_failure_count" else 1)
               for name in failure_names)
    assert not any(value in json.dumps(failure_result) for value in ("127.0.0.1", "1234", "5401", "121"))
    progress_log = (
        b"NTCP2: SessionConfirmed from 127.0.0.1:2345 (RouterHashSecret)\n"
        b"NTCP2: Session with 127.0.0.1:2345 (RouterHashSecret) terminated\n"
    )
    progress_result = observe(progress_log)
    assert progress_result["session_confirmed_router_info_accepted_count"] == 1
    assert progress_result["ntcp2_session_terminated_count"] == 1
    assert not any(value in json.dumps(progress_result) for value in ("127.0.0.1", "2345", "RouterHashSecret"))
    assert stage_result["result"] == "rejected"
    assert stage_result["reason_code"] == "decoded-delivery-status-count-not-one"

    rejected = [
        fixture.replace(b"type 10", b"type 2"),
        fixture + fixture,
        fixture.replace(b"NTCP2: I2NP", b"NTCP2: Options"),
    ]
    for candidate in rejected:
        assert observe(candidate)["result"] == "rejected"
    malformed = [
        b"",
        fixture + b"\xff",
        b"x" * (MAX_LOG_BYTES + 1),
        b"x" * (MAX_LINE_BYTES + 1),
    ]
    for candidate in malformed:
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
        consume_log(log, evidence, root, "i2pr-to-i2pd-ipv4", baseline_offset=0)
        assert not log.exists()
        record = json.loads(evidence.read_text(encoding="utf-8"))
        assert record["direction"] == "i2pr-to-i2pd-ipv4"
        evidence.unlink()

        bad_log = root / "bad.log"
        bad_log.write_bytes(fixture + fixture)
        consume_log(bad_log, evidence, root, "i2pr-to-i2pd-ipv4", baseline_offset=0)
        assert not bad_log.exists(), "classified log should be consumed from the owned tree"
        rejected_record = json.loads(evidence.read_text(encoding="utf-8"))
        assert rejected_record["result"] == "rejected"
        evidence.unlink()


def consume_log(log_path: Path, output_path: Path, owned_root: Path, direction: str,
                baseline_offset: int = 0) -> None:
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
    record = observe(raw, baseline_offset)
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
    parser.add_argument("--baseline-offset", type=int, default=0)
    args = parser.parse_args()

    try:
        if args.self_test:
            if any((args.consume_log, args.output, args.owned_root, args.direction)):
                raise ObservationError("self-test-arguments-conflict")
            self_test()
            print("plan417 observer self-test passed")
            return 0
        if not all((args.consume_log, args.output, args.owned_root, args.direction)):
            raise ObservationError("consume-arguments-required")
        consume_log(args.consume_log, args.output, args.owned_root, args.direction,
                    args.baseline_offset)
        return 0
    except (OSError, ObservationError) as exc:
        # Emit only a closed reason category; never echo paths or source bytes.
        reason = str(exc) if isinstance(exc, ObservationError) else "io-failed"
        print(json.dumps({"result": "rejected", "reason_code": reason}, separators=(",", ":")))
        return 2


if __name__ == "__main__":
    sys.exit(main())

