#!/usr/bin/env bash
# Plan 046 rootless sandbox capability probe.
#
# Runs a real bounded create/configure/connect/teardown cycle inside a fresh
# process-scoped user/network namespace without starting any router. Emits a
# single strict JSON status line and an optional sanitized attestation file.
#
# Allowed outcomes (every distinct capability failure is its own code):
#   rootless_sandbox_available
#   blocked_unprivileged_user_namespace
#   blocked_uid_map
#   blocked_gid_map
#   blocked_setgroups_contract
#   blocked_network_namespace
#   blocked_namespace_local_net_admin
#   blocked_mount_namespace
#   blocked_private_proc
#   blocked_no_new_privs
#   blocked_loopback_configuration
#   blocked_synthetic_address_configuration
#   blocked_external_route_present
#   blocked_external_connect_possible
#   blocked_rootless_cleanup
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo_root=$(cd "$script_dir/../.." && pwd)

attestation_path=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --attestation-path)
      attestation_path="${2:-}"
      shift 2
      ;;
    --help|-h)
      cat <<EOF
usage: bash scripts/interop/probe-rootless-sandbox.sh [--attestation-path <path>]
EOF
      exit 0
      ;;
    *)
      echo "unknown-argument:$1" >&2
      exit 1
      ;;
  esac
done

if ! command -v unshare >/dev/null 2>&1; then
  python3 -c 'import json,sys; print(json.dumps({"schema":1,"type":"rootless-sandbox-probe","outcome":"blocked_unprivileged_user_namespace"}))'
  exit 1
fi

# Quick check that the host allows unprivileged user namespaces before we
# even attempt unshare. We do not check `sudo`, only the unprivileged path.
if [[ -r /proc/sys/kernel/unprivileged_userns_clone ]]; then
  if [[ "$(cat /proc/sys/kernel/unprivileged_userns_clone 2>/dev/null || echo 1)" == "0" ]]; then
    python3 -c 'import json,sys; print(json.dumps({"schema":1,"type":"rootless-sandbox-probe","outcome":"blocked_unprivileged_user_namespace"}))'
    exit 1
  fi
fi

probe_status=0
parent_digest_pre=$(python3 - <<'PYEOF' 2>/dev/null || echo "0000000000000000000000000000000000000000000000000000000000000000"
import hashlib
import json
import os
import socket

try:
    hostname = socket.gethostname()
except OSError:
    hostname = ""

try:
    uid = os.getuid()
    gid = os.getgid()
    groups = os.getgroups()
except (AttributeError, OSError):
    uid = gid = -1
    groups = []

payload = {
    "hostname": hostname,
    "uid": uid,
    "gid": gid,
    "groups_count": len(groups),
}
print(hashlib.sha256(json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()).hexdigest())
PYEOF
)
probe_command=(
  unshare
  --user
  --net
  --mount
  --pid
  --fork
  --propagation
  private
  --mount-proc
  --map-root-user
  /usr/bin/env
  "I2PR_INTEROP_ROOTLESS_INNER=1"
  "I2PR_INTEROP_ROOTLESS_PARENT_DIGEST_PRE=$parent_digest_pre"
  "I2PR_INTEROP_PARENT_USER_NS_INODE=$(stat -Lc %i /proc/self/ns/user)"
  "I2PR_INTEROP_PARENT_NET_NS_INODE=$(stat -Lc %i /proc/self/ns/net)"
  "I2PR_INTEROP_PARENT_MNT_NS_INODE=$(stat -Lc %i /proc/self/ns/mnt)"
  "I2PR_INTEROP_PARENT_PID_NS_INODE=$(stat -Lc %i /proc/self/ns/pid)"
  python3 "$repo_root/tests/integration/ntcp2/harness/rootless_supervisor.py" --probe
)
if [[ -n "$attestation_path" ]]; then
  probe_command+=(--attestation-output "$attestation_path")
fi
if probe_outcome=$("${probe_command[@]}" 2>&1); then
  :
else
  probe_status=$?
  # Surface the supervised typed blocker regardless of success-path status.
  printf '%s\n' "$probe_outcome"
  if [[ -n "$attestation_path" && ! -f "$attestation_path" ]]; then
    mkdir -p "$(dirname "$attestation_path")" 2>/dev/null || true
    first_outcome=$(printf '%s\n' "$probe_outcome" | grep -o '"outcome":"[a-z_]*"' | head -1 | cut -d'"' -f4 || true)
    if [[ -z "$first_outcome" ]]; then
      first_outcome="blocked_unprivileged_user_namespace"
    fi
    python3 -c "
import json, sys
print(json.dumps({
    'schema': 1,
    'type': 'rootless-sandbox-probe',
    'outcome': '$first_outcome',
}))
" > "$attestation_path" 2>/dev/null || true
  fi
  if [[ $probe_status -eq 1 ]]; then
    exit 1
  fi
  exit "$probe_status"
fi

if [[ -n "$attestation_path" ]]; then
  mkdir -p "$(dirname "$attestation_path")" 2>/dev/null || true
  cat > "$attestation_path" <<'EOF'
{"schema":1,"type":"rootless-sandbox-probe","outcome":"rootless_sandbox_available"}
EOF
fi

printf '%s\n' '{"schema":1,"type":"rootless-sandbox-probe","outcome":"rootless_sandbox_available"}'
exit 0
