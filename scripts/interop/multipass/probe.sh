#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_dir/common.sh"

while (($#)); do
  case "$1" in
    --help|-h) printf 'usage: probe.sh\n'; exit 0 ;;
    *) die "unknown argument: $1" ;;
  esac
  shift
done
require_instance
require_command python3
guest_exec mkdir -p "$guest_evidence_root/environment-probe"

probe_status=0
probe_output=$(guest_exec bash "$guest_repo_root/scripts/interop/probe-rootless-sandbox.sh" \
  --attestation-path "$guest_evidence_root/environment-probe/probe.json") || probe_status=$?
probe_line=$(python3 - "$probe_output" <<'PY'
import json
import sys
for line in reversed(sys.argv[1].splitlines()):
    try:
        value = json.loads(line)
    except json.JSONDecodeError:
        continue
    if value.get("type") == "rootless-sandbox-probe":
        print(json.dumps(value, sort_keys=True, separators=(",", ":")))
        break
else:
    print(json.dumps({"schema": 1, "type": "rootless-sandbox-probe", "outcome": "blocked_unprivileged_user_namespace"}, separators=(",", ":")))
PY
)
probe_outcome=$(python3 - "$probe_line" <<'PY'
import json
import sys
print(json.loads(sys.argv[1]).get("outcome", "blocked_unprivileged_user_namespace"))
PY
)
if [[ "$probe_status" != 0 || "$probe_outcome" != rootless_sandbox_available ]]; then
  typed_blocker "$probe_outcome"
  exit 2
fi

attestation_path="$guest_evidence_root/environment-probe/wrapper-attestation.json"
wrapper_status=0
wrapper_output=$(guest_exec bash "$guest_repo_root/scripts/interop/rootless-enter.sh" \
  --probe --attestation-output "$attestation_path") || wrapper_status=$?
if [[ "$wrapper_status" != 0 ]]; then
  typed_blocker blocked_rootless_probe "$wrapper_status"
  exit 2
fi
wrapper_attestation=$(guest_exec cat "$attestation_path")
if ! python3 - "$wrapper_attestation" <<'PY'
import hashlib
import json
import sys
value = json.loads(sys.argv[1])
digest = value.get("attestation_sha256", "")
expected = dict(value)
expected["attestation_sha256"] = ""
actual = hashlib.sha256(json.dumps(expected, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
if digest == "0" * 64 or digest != actual:
    raise SystemExit("invalid wrapper attestation digest")
if value.get("topology_kind") != "rootless-sealed-single-netns" or value.get("privilege_model") != "unprivileged-userns":
    raise SystemExit("wrapper attestation contract mismatch")
for field in ("user_namespace_distinct", "network_namespace_distinct", "mount_namespace_distinct", "pid_namespace_distinct", "no_new_privs", "synthetic_ipv4_ready"):
    if value.get(field) is not True:
        raise SystemExit(f"wrapper attestation does not prove {field}")
if value.get("external_interface_count") != 0 or value.get("default_route_count") != 0:
    raise SystemExit("wrapper attestation reports external network state")
if value.get("external_route_probe") != "absent" or value.get("external_connect_probe") != "blocked":
    raise SystemExit("wrapper attestation external probes are not blocked")
if value.get("parent_network_state_unchanged") is not True:
    raise SystemExit("parent network state changed")
PY
then
  typed_blocker blocked_rootless_probe "wrapper attestation validation failed"
  exit 2
fi

probe_path="$instance_state_dir/probe.json"
printf '%s\n' "$probe_line" >"$probe_path"
probe_sha256=$(sha256_file "$probe_path")
wrapper_sha256=$(python3 - "$wrapper_attestation" <<'PY'
import hashlib
import sys
print(hashlib.sha256((sys.argv[1] + "\n").encode()).hexdigest())
PY
)
receipt=$(python3 - "$probe_sha256" "$wrapper_sha256" "$environment_manifest_sha256" <<'PY'
import datetime as dt
import json
import sys
print(json.dumps({
    "schema": 1,
    "type": "multipass-rootless-probe",
    "outcome": "rootless_sandbox_available",
    "probe_sha256": sys.argv[1],
    "wrapper_attestation_sha256": sys.argv[2],
    "environment_manifest_sha256": sys.argv[3],
    "completed_at": dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
}, sort_keys=True, separators=(",", ":")))
PY
)
write_json "$instance_state_dir/probe-receipt.json" "$receipt"
printf '%s\n' "$receipt"
