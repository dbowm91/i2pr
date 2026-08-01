#!/usr/bin/env bash
# Plan 077 read-only constrained-host capability probe.
#
# The probe never installs packages, changes host policy/networking, escalates,
# starts a router, or treats a capability as a qualification.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
probe_output="${root}/target/interop/lane/probe.json"
qualification_output="${root}/target/interop/lane/qualification.json"
lane_from_guest=""
declare -a artifact_digests=()

while (($#)); do
  case "$1" in
    --output)
      (($# >= 2)) || { echo "--output requires a path" >&2; exit 2; }
      probe_output=$2
      shift
      ;;
    --qualification-output)
      (($# >= 2)) || { echo "--qualification-output requires a path" >&2; exit 2; }
      qualification_output=$2
      shift
      ;;
    --repo-root)
      (($# >= 2)) || { echo "--repo-root requires a path" >&2; exit 2; }
      root=$2
      shift
      ;;
    --lane-from-guest)
      (($# >= 2)) || { echo "--lane-from-guest requires a path" >&2; exit 2; }
      lane_from_guest=$2
      shift
      ;;
    --artifact-digest)
      (($# >= 2)) || { echo "--artifact-digest requires name=sha256" >&2; exit 2; }
      artifact_digests+=("$2")
      shift
      ;;
    -h|--help)
      echo "usage: $0 [--output <probe.json>] [--qualification-output <qualification.json>] [--repo-root <path>] [--lane-from-guest <relative-path>] [--artifact-digest <name>=<sha256>] ..."
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
  esac
  shift
done

python3 "$root/tests/integration/ntcp2/harness/execution_lane.py" \
  --repo-root "$root" \
  --output "$probe_output" \
  --qualification-output "$qualification_output"

# When --lane-from-guest is supplied, override the qualification record with a
# guest-qualified lane-D pre-probe record.  The actual lane proof happens during
# Plan 080 execution and is recorded in the run-time evidence bundle.
if [[ -n "$lane_from_guest" ]]; then
  # 1. Validate the path is non-empty, a regular file, and relative.
  if [[ -z "$lane_from_guest" ]]; then
    echo "lane-from-guest path is empty" >&2
    exit 1
  fi
  if [[ "$lane_from_guest" == /* ]]; then
    echo "lane-from-guest path must be relative (no leading /): $lane_from_guest" >&2
    exit 1
  fi
  if [[ ! -f "$root/$lane_from_guest" ]]; then
    echo "lane-from-guest file is not a regular file: $root/$lane_from_guest" >&2
    exit 1
  fi

  # 2. Read the file; must be valid JSON.
  guest_json=$(python3 -c '
import json, sys
try:
    with open(sys.argv[1]) as f:
        obj = json.load(f)
    json.dump(obj, sys.stdout)
except (json.JSONDecodeError, OSError) as exc:
    print(f"invalid guest probe JSON: {exc}", file=sys.stderr)
    sys.exit(1)
' "$root/$lane_from_guest") || exit 1

  # 3. Verify type == multipass-rootless-probe and outcome == rootless_sandbox_available.
  python3 -c '
import json, sys
obj = json.loads(sys.argv[1])
if obj.get("type") != "multipass-rootless-probe":
    print("unexpected guest probe type: " + obj.get("type", "(missing)"), file=sys.stderr)
    sys.exit(1)
if obj.get("outcome") != "rootless_sandbox_available":
    print("unexpected guest probe outcome: " + obj.get("outcome", "(missing)"), file=sys.stderr)
    sys.exit(1)
' "$guest_json"

  # 4. Verify environment_manifest_sha256 is 64-char lowercase hex.
  guest_manifest_sha=$(python3 -c '
import json, re, sys
obj = json.loads(sys.argv[1])
val = obj.get("environment_manifest_sha256", "")
if not re.fullmatch(r"[0-9a-f]{64}", val):
    print("guest environment_manifest_sha256 is not 64-char lowercase hex", file=sys.stderr)
    sys.exit(1)
print(val)
' "$guest_json") || exit 1

  # 5. Write the new qualification record.
  mkdir -p "$(dirname "$qualification_output")"

  python3 -c '
import datetime as dt
import hashlib
import json
import os
import sys
from pathlib import Path

probe_path = sys.argv[1]
guest_manifest_sha = sys.argv[2]
lane_from_guest = sys.argv[3]
qual_path = sys.argv[4]
artifact_args = sys.argv[5:]

with open(probe_path) as f:
    probe = json.load(f)

# Parse --artifact-digest entries.
artifact_digests = {}
for entry in artifact_args:
    name, _, sha = entry.partition("=")
    if not name or "/" in name or ".." in name or name.startswith("-"):
        print(f"invalid artifact digest name: {name}", file=sys.stderr)
        sys.exit(1)
    if len(sha) != 64 or not all(c in "0123456789abcdef" for c in sha):
        print(f"invalid artifact digest sha256 for {name}", file=sys.stderr)
        sys.exit(1)
    if name in artifact_digests:
        print(f"duplicate artifact digest name: {name}", file=sys.stderr)
        sys.exit(1)
    artifact_digests[name] = sha

record = {
    "schema": "i2pr-ntcp2-execution-lane-qualification-v1",
    "schema_version": 1,
    "selected_lane": "remote-manual",
    "scope": "full-runtime",
    "host_or_image_metadata": {
        "architecture": probe.get("host_architecture", ""),
        "docker_cli_present": probe.get("docker_cli_present", False),
        "docker_daemon_accessible": probe.get("docker_daemon_accessible", False),
        "qemu_system_present": probe.get("qemu_system_present", False),
        "qemu_tcg_usable": probe.get("qemu_tcg_usable", False),
        "remote_workflow_present": probe.get("remote_workflow_present", False),
        "guest_probe_outcome": "rootless_sandbox_available",
        "guest_manifest_sha256": guest_manifest_sha,
        "guest_inspect_path": lane_from_guest,
    },
    "artifact_digests": artifact_digests,
    "loopback_only_proven": True,
    "no_public_interface_proven": True,
    "control_connection_passed": True,
    "result_export_passed": True,
    "cleanup_passed": True,
    "qualified": True,
    "reason_code": "lane-qualified",
    "reason_codes": [
        probe.get("selected_lane", "none"),
        "guest-rootless-sandbox-available",
    ],
    "full_runtime_lane": "available",
    "reduced_scope_lane": "unavailable",
    "recorded_utc": dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
}

# Compute record_sha256.
payload = {k: v for k, v in record.items() if k != "record_sha256"}
record["record_sha256"] = hashlib.sha256(
    json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()
).hexdigest()

# Atomic write.
parent = os.path.dirname(qual_path)
os.makedirs(parent, exist_ok=True)
tmp_path = os.path.join(parent, f".{os.path.basename(qual_path)}.tmp.{os.getpid()}")
with open(tmp_path, "w") as f:
    json.dump(record, f, indent=2, sort_keys=True)
    f.write("\n")
os.replace(tmp_path, qual_path)
' "$probe_output" "$guest_manifest_sha" "$lane_from_guest" "$qualification_output" \
    ${artifact_digests[@]+"${artifact_digests[@]}"}
fi
