#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_dir/common.sh"

replace=0
while (($#)); do
  case "$1" in
    --replace) [[ "$replace" == 0 ]] || die "duplicate --replace"; replace=1 ;;
    --help|-h) printf 'usage: create.sh [--replace]\n'; exit 0 ;;
    *) die "unknown argument: $1" ;;
  esac
  shift
done

require_command python3
require_multipass
ensure_dirs
if ! multipass find "$image" >/dev/null 2>&1; then
  typed_blocker blocked_image_unavailable
  exit 2
fi
if instance_exists; then
  if [[ "$replace" != 1 ]]; then
    typed_blocker blocked_instance_name_collision
    exit 2
  fi
  multipass stop "$instance_name" >/dev/null 2>&1 || true
  multipass delete "$instance_name" >/dev/null 2>&1 || {
    typed_blocker blocked_launch_failed "canonical instance delete failed"
    exit 2
  }
  multipass purge >/dev/null 2>&1 || {
    typed_blocker blocked_launch_failed "explicit replacement purge failed"
    exit 2
  }
fi

if ! multipass launch "$image" --name "$instance_name" --cpus "$cpus" \
    --memory "$memory" --disk "$disk" --timeout "$launch_timeout" \
    --cloud-init "$script_dir/cloud-init.yaml" >/dev/null; then
  typed_blocker blocked_launch_failed
  exit 2
fi
deadline=$((SECONDS + launch_timeout))
while ! multipass info "$instance_name" --format json 2>/dev/null | grep -Fq '"state": "Running"'; do
  if ((SECONDS >= deadline)); then
    typed_blocker blocked_launch_failed "Multipass instance did not become ready"
    exit 2
  fi
  sleep 2
done
while ! guest_admin_exec test -f /var/lib/cloud/instance/boot-finished >/dev/null 2>&1; do
  if ((SECONDS >= deadline)); then
    typed_blocker blocked_cloud_init_failed
    exit 2
  fi
  sleep 2
done
if ! guest_admin_exec test -f /var/lib/i2pr-interop/provisioning.json >/dev/null 2>&1; then
  typed_blocker blocked_cloud_init_failed "provisioning marker is missing"
  exit 2
fi
provisioning=$(guest_root_exec cat /var/lib/i2pr-interop/provisioning.json)
if ! python3 - "$provisioning" <<'PY'
import json
import sys

value = json.loads(sys.argv[1])
if value.get("schema") != 1 or value.get("image_release") != "24.04":
    raise SystemExit("provisioning schema or image mismatch")
if value.get("architecture") != "x86_64":
    raise SystemExit("provisioning architecture mismatch")
if value.get("effective_sysctls") != {
    "kernel.unprivileged_userns_clone": 1,
    "kernel.apparmor_restrict_unprivileged_userns": 0,
}:
    raise SystemExit("provisioning sysctl mismatch")
for key in ("rust_toolchain", "java", "ant", "cmake", "compiler"):
    if not isinstance(value.get(key), str) or not value[key].strip():
        raise SystemExit(f"provisioning tool version missing: {key}")
if "1.95.0" not in value["rust_toolchain"]:
    raise SystemExit("Rust toolchain mismatch")
PY
then
  typed_blocker blocked_cloud_init_failed "provisioning marker validation failed"
  exit 2
fi
if ! guest_exec /home/i2ptest/.cargo/bin/rustc +1.95.0 --version >/dev/null 2>&1 || \
    ! guest_exec /home/i2ptest/.cargo/bin/cargo +1.95.0 --version >/dev/null 2>&1 || \
    ! guest_exec /home/i2ptest/.cargo/bin/cargo +1.95.0 fmt --version >/dev/null 2>&1 || \
    ! guest_exec /home/i2ptest/.cargo/bin/cargo +1.95.0 clippy --version >/dev/null 2>&1 || \
    ! guest_admin_exec java -version >/dev/null 2>&1 || \
    ! guest_admin_exec ant -version >/dev/null 2>&1 || \
    ! guest_admin_exec cmake --version >/dev/null 2>&1 || \
    ! guest_admin_exec cc --version >/dev/null 2>&1; then
  typed_blocker blocked_cloud_init_failed "required guest tool is unavailable"
  exit 2
fi

status_json=$(bash "$script_dir/status.sh" --json)
if ! python3 - "$status_json" <<'PY'
import json
import sys
value = json.loads(sys.argv[1])
if value.get("guest_os_id") != "ubuntu" or value.get("guest_os_version") != "24.04":
    print(json.dumps({"schema": 1, "type": "multipass-interop", "outcome": "blocked_wrong_guest_os"}))
    raise SystemExit(2)
if value.get("guest_architecture") != "x86_64":
    print(json.dumps({"schema": 1, "type": "multipass-interop", "outcome": "blocked_wrong_guest_architecture"}))
    raise SystemExit(2)
if value.get("sysctls") != {"kernel.unprivileged_userns_clone": "1", "kernel.apparmor_restrict_unprivileged_userns": "0"}:
    print(json.dumps({"schema": 1, "type": "multipass-interop", "outcome": "blocked_guest_policy_mismatch"}))
    raise SystemExit(2)
if value.get("execution_user_privileged") is not False:
    print(json.dumps({"schema": 1, "type": "multipass-interop", "outcome": "blocked_execution_user_privileged"}))
    raise SystemExit(2)
PY
then
  exit 2
fi

version=$(multipass version | head -n 1)
creation_json=$(python3 - "$version" "$status_json" "$environment_manifest" <<'PY'
import datetime as dt
import hashlib
import json
import sys
value = json.loads(sys.argv[2])
value.update({
    "schema": 1,
    "type": "multipass-interop-creation",
    "instance_name": "i2pr-interop-rootless",
    "image": "24.04",
    "resource_profile": "4cpu-8g-40g",
    "multipass_version": sys.argv[1],
    "environment_manifest_sha256": hashlib.sha256(open(sys.argv[3], "rb").read()).hexdigest(),
    "created_at": dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
})
print(json.dumps(value, sort_keys=True, separators=(",", ":")))
PY
)
write_json "$instance_state_dir/creation.json" "$creation_json"
printf '%s\n' "$status_json"
