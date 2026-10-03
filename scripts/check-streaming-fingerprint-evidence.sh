#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -ne 1 ]]; then
  echo "usage: $0 <plan312-evidence-dir>" >&2
  exit 2
fi

python3 - "$1" <<'PY'
import csv
import sys
from pathlib import Path

root = Path(sys.argv[1])
expected_pin = "635b013a612ff47278ef02acf8580a28e10e26c5"
header = (
    "scenario", "index", "time_10ms", "direction", "flags", "payload_len",
    "seq_delta", "ack_delta", "retransmit_ordinal", "max_payload",
    "advertised_window", "choked", "terminal",
)
roles = {
    "i2pr-client": "to_destination",
    "i2pd-client": "to_destination",
    "i2pr-server": "from_destination",
    "i2pd-server": "from_destination",
}
failures = []

manifest_path = root / "fingerprint-manifest.tsv"
try:
    manifest_rows = list(csv.reader(manifest_path.open(encoding="utf-8"), delimiter="\t"))
except OSError:
    manifest_rows = []
if not manifest_rows or manifest_rows[0] != ["key", "value"]:
    failures.append("manifest missing or has a non-canonical header")
manifest = {}
for row in manifest_rows[1:]:
    if len(row) != 2 or row[0] in manifest:
        failures.append("manifest has malformed or duplicate fields")
        continue
    manifest[row[0]] = row[1]
required_manifest = {
    "i2pd_version": "2.61.0",
    "i2pd_revision": expected_pin,
    "scenario": "clean_handshake_default_port",
    "dimensions": "flags,from_included,max_payload,payload_length",
    "raw_packet_bytes": "0",
    "destination_or_stream_ids": "0",
}
for key, value in required_manifest.items():
    if manifest.get(key) != value:
        failures.append(f"manifest {key} mismatch")

traces = {}
for role, direction in roles.items():
    path = root / f"fingerprint-{role}.tsv"
    try:
        with path.open(encoding="utf-8", newline="") as stream:
            rows = list(csv.reader(stream, delimiter="\t"))
    except OSError:
        failures.append(f"missing trace: {role}")
        continue
    if not rows or tuple(rows[0]) != header:
        failures.append(f"non-canonical trace header: {role}")
        continue
    data = rows[1:]
    if len(data) != 1:
        failures.append(f"{role} must contain exactly one clean-handshake packet")
        continue
    row = data[0]
    if len(row) != len(header):
        failures.append(f"malformed trace row: {role}")
        continue
    try:
        scenario, index, event_direction = row[0], row[1], row[3]
        time_bucket = int(row[2])
        flags = int(row[4])
        payload_len = int(row[5])
        seq_delta = int(row[6])
        ack_delta = int(row[7])
        retransmit_ordinal = int(row[8])
        max_payload = None if row[9] == "-" else int(row[9])
    except ValueError:
        failures.append(f"non-numeric trace value: {role}")
        continue
    if scenario != "clean_handshake" or index != "0" or event_direction != direction:
        failures.append(f"scenario/index/direction mismatch: {role}")
    if flags & 0x0001 == 0:
        failures.append(f"trace is not a SYN/SYN-ACK: {role}")
    if not 0 <= time_bucket <= 60_000 or not 0 <= payload_len <= 65_535:
        failures.append(f"trace value outside bounds: {role}")
    if max_payload is None or not 1 <= max_payload <= 65_535:
        failures.append(f"MAX_PACKET_SIZE is absent or invalid: {role}")
    if seq_delta != 0 or ack_delta != 0 or retransmit_ordinal != 0:
        failures.append(f"handshake normalization mismatch: {role}")
    if row[10] != "-" or row[11] != "-" or row[12] != "-":
        failures.append(f"unregistered field contains a value: {role}")
    traces[role] = {
        "flags": flags,
        "from_included": bool(flags & 0x0020),
        "max_payload": max_payload,
        "payload_length": payload_len,
    }

if not failures and len(traces) == 4:
    matrix = root / "fingerprint-matrix.tsv"
    with matrix.open("w", encoding="utf-8", newline="") as stream:
        writer = csv.writer(stream, delimiter="\t", lineterminator="\n")
        writer.writerow(("role", "dimension", "i2pr", "i2pd", "result"))
        for role in ("client", "server"):
            left = traces[f"i2pr-{role}"]
            right = traces[f"i2pd-{role}"]
            for dimension in ("flags", "from_included", "max_payload", "payload_length"):
                first, second = left[dimension], right[dimension]
                writer.writerow((role, dimension, first, second,
                                 "match" if first == second else "different"))

if failures:
    for failure in failures:
        print(f"evidence check failed: {failure}", file=sys.stderr)
    sys.exit(1)
print("Plan 312 directional Streaming fingerprint evidence passed (4 role traces, 8 compared dimensions, exact i2pd pin)")
PY
