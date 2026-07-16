#!/usr/bin/env bash
# Plan 050: invoke the in-guest verify-base-environment and emit a typed
# sanitized status. This script is a host-side wrapper that:
#  1. requires complete ownership proof for the bound run/generation;
#  2. calls the in-guest helper through the established guest_exec path;
#  3. parses the bounded JSON output;
#  4. writes a sanitized host-side status record that the lifecycle,
#     post-verify, and resume paths can consult.
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_dir/common.sh"

output_path=""
while (($#)); do
  case "$1" in
    --output)
      [[ -z "$output_path" && $# -ge 2 ]] || die "duplicate or incomplete --output"
      output_path=$2
      shift
      ;;
    --help|-h) printf 'usage: verify-base.sh [--output <path>]\n'; exit 0 ;;
    *) die "unknown argument: $1" ;;
  esac
  shift
done

require_owned_instance
require_command python3
ensure_dirs
acquire_lifecycle_lock
require_owned_instance

verify_status=0
verify_output=$(guest_root_exec bash /usr/local/sbin/i2pr-multipass-verify-base) || verify_status=$?

if [[ "$verify_status" != 0 || -z "$verify_output" ]]; then
  if [[ -n "$output_path" ]]; then
    python3 - "$output_path" "$run_id" "$instance_generation" "$environment_manifest_sha256" "$cloud_init_sha256" <<'PY'
import datetime as dt, json, os, sys
from pathlib import Path
value = {
    "schema_version": 1,
    "type": "multipass-base-environment-verify",
    "run_id": sys.argv[2],
    "instance_generation": int(sys.argv[3]),
    "environment_manifest_sha256": sys.argv[4],
    "cloud_init_sha256": sys.argv[5],
    "outcome": "blocked_cloud_init_post_verify_failure",
    "completed_at_utc": dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
}
Path(sys.argv[1]).parent.mkdir(parents=True, exist_ok=True)
Path(sys.argv[1]).write_text(json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
os.chmod(sys.argv[1], 0o600)
PY
  fi
  write_environment_blocker blocked_cloud_init_post_verify_failure post-verify operator-inspection-required
  typed_blocker blocked_cloud_init_post_verify_failure
  exit 2
fi

python3 - "$verify_output" "$run_id" "$instance_generation" "$environment_manifest_sha256" "$cloud_init_sha256" "$output_path" <<'PY'
import datetime as dt, json, os, sys
from pathlib import Path
raw = sys.argv[1]
try:
    value = json.loads(raw)
except json.JSONDecodeError:
    sys.stderr.write("verify-base-environment output is not JSON\n")
    raise SystemExit(2)
if not isinstance(value, dict):
    raise SystemExit(2)
value.setdefault("schema_version", 1)
value["type"] = "multipass-base-environment-verify"
value["run_id"] = sys.argv[2]
value["instance_generation"] = int(sys.argv[3])
value["environment_manifest_sha256"] = sys.argv[4]
value["cloud_init_sha256"] = sys.argv[5]
value["completed_at_utc"] = dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z")
def cap_zero(value):
    return value in (None, "", "0000000000000000")
caps_clean = all(cap_zero(value.get("i2ptest_cap_" + n)) for n in ("inh", "prm", "eff", "amb"))
value["execution_user_capabilities_zero"] = caps_clean
value["execution_user_uid_is_non_root"] = value.get("i2ptest_uid", 0) != 0
expected_groups = {"i2ptest"}
groups = set(value.get("i2ptest_groups") or [])
value["execution_user_privileged"] = (
    value.get("i2ptest_sudo") is True
    or not value["execution_user_uid_is_non_root"]
    or not caps_clean
    or bool(groups - expected_groups)
)
out = sys.argv[6]
if out:
    Path(out).parent.mkdir(parents=True, exist_ok=True)
    Path(out).write_text(json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
    os.chmod(out, 0o600)
print(json.dumps(value, sort_keys=True, separators=(",", ":")))
PY

if [[ "$(guest_root_exec stat -c '%U:%G:%a' /var/lib/i2pr-interop/environment.json 2>/dev/null || true)" != "root:root:600" ]] || \
   [[ "$(guest_root_exec stat -c '%U:%G:%a' /var/lib/i2pr-interop/ownership-token 2>/dev/null || true)" != "root:root:600" ]]; then
  write_environment_blocker blocked_cloud_init_ownership_contract_failure post-verify operator-inspection-required
  typed_blocker blocked_cloud_init_ownership_contract_failure
  exit 2
fi
