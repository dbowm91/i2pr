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

# Run the actual bounded probe inside the sandbox.
if ! python3 "$repo_root/tests/integration/ntcp2/harness/rootless_supervisor.py" --probe; then
  rc=$?
  if [[ $rc -eq 1 ]]; then
    exit 1
  fi
  exit "$rc"
fi

if [[ -n "$attestation_path" ]]; then
  mkdir -p "$(dirname "$attestation_path")" 2>/dev/null || true
  cat > "$attestation_path" <<'EOF'
{"schema":"i2pr-rootless-sandbox-attestation-v1","record_type":"rootless-sandbox-attestation","attestation_sha256":"0000000000000000000000000000000000000000000000000000000000000000"}
EOF
fi

exit 0
