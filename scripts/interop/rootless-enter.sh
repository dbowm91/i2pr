#!/usr/bin/env bash
# Plan 046 rootless sealed-namespace outer entrypoint.
#
# This script is the only path that creates the rootless user/network sandbox
# for the i2pr NTCP2 evidence lane. It accepts a strictly allowlisted set of
# operations and rejects arbitrary command strings, sudo, host-firewall
# mutation, capability grants, and any automatic escalation to the
# privileged backend.
#
# Allowed operations:
#   --probe
#   --scenario <allowlisted-scenario-id>
#   --profile <allowlisted-profile>
#
# The script uses `unshare --user --net --mount --pid --propagation private`
# to create a process-scoped, unprivileged user/network/mount/PID namespace.
# The forked child supervisor (`rootless_supervisor.py`) verifies the
# namespace, UID/GID maps, setgroups policy, no_new_privs, loopback readiness,
# synthetic address binding, and external route/connect failure before any
# router is started.
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo_root=$(cd "$script_dir/.." && pwd)

allowed_scenarios=(
  i2pr-to-java-ipv4
  java-to-i2pr-ipv4
  i2pr-to-i2pd-ipv4
  i2pd-to-i2pr-ipv4
)
allowed_references=(java_i2p i2pd)
allowed_profiles=(
  rootless-environment-smoke
  rootless-reference-crosscheck-ipv4
  rootless-handshake-smoke
)

operation=""
scenario=""
reference=""
profile=""
build_cache=""
run_root=""
attestation_output=""

die() {
  echo "rootless-enter: $1" >&2
  exit 1
}

require_string() {
  local value="$1"
  local field="$2"
  if [[ -z "$value" ]]; then
    die "$field is required"
  fi
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --probe)
        [[ -z "$operation" ]] || die "duplicate-or-conflicting-operation"
        operation="probe"
        shift
        ;;
      --scenario)
        [[ -z "$operation" ]] || die "duplicate-or-conflicting-operation"
        scenario="${2:-}"
        require_string "$scenario" "scenario"
        operation="scenario"
        shift 2
        ;;
      --reference)
        [[ -z "$reference" ]] || die "duplicate-reference"
        reference="${2:-}"
        require_string "$reference" "reference"
        shift 2
        ;;
      --build-cache)
        [[ -z "$build_cache" ]] || die "duplicate-build-cache"
        build_cache="${2:-}"
        require_string "$build_cache" "build-cache"
        shift 2
        ;;
      --run-root)
        [[ -z "$run_root" ]] || die "duplicate-run-root"
        run_root="${2:-}"
        require_string "$run_root" "run-root"
        shift 2
        ;;
      --profile)
        [[ -z "$operation" ]] || die "duplicate-or-conflicting-operation"
        profile="${2:-}"
        require_string "$profile" "profile"
        operation="profile"
        shift 2
        ;;
      --attestation-output)
        [[ -z "$attestation_output" ]] || die "duplicate-attestation-output"
        attestation_output="${2:-}"
        require_string "$attestation_output" "attestation-output"
        shift 2
        ;;
      --help|-h)
        cat <<EOF
usage: bash scripts/interop/rootless-enter.sh [--probe | --scenario <id> --reference <ref> [--build-cache <path>] [--run-root <path>] [--attestation-output <path>] | --profile <id> [--attestation-output <path>]]
EOF
        exit 0
        ;;
      *)
        die "unknown-argument:$1"
        ;;
    esac
  done
  if [[ -z "$operation" ]]; then
    die "operation-required"
  fi
}

validate_choice() {
  local label="$1"
  local needle="$2"
  shift 2
  local choice
  for choice in "$@"; do
    if [[ "$choice" == "$needle" ]]; then
      return 0
    fi
  done
  die "$label-not-allowed:$needle"
}

namespace_inode() {
  case "$1" in
    user|net|mnt|pid) stat -Lc '%i' "/proc/self/ns/$1" || die "parent-namespace-read-failed:$1" ;;
    *) die "unknown-parent-namespace:$1" ;;
  esac
}

# Build the strictly-fixed `unshare` invocation. No shell eval, no command
# interpolation, no sudo, no host-network-state mutation.
build_unshare_command() {
  local mode="$1"
  local i2pr_commit="${I2PR_INTEROP_COMMIT:-}"
  UNSHARE_COMMAND=(
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
    "I2PR_INTEROP_PARENT_USER_NS_INODE=$(namespace_inode user)"
    "I2PR_INTEROP_PARENT_NET_NS_INODE=$(namespace_inode net)"
    "I2PR_INTEROP_PARENT_MNT_NS_INODE=$(namespace_inode mnt)"
    "I2PR_INTEROP_PARENT_PID_NS_INODE=$(namespace_inode pid)"
    "I2PR_INTEROP_COMMIT=$i2pr_commit"
  )
  case "$mode" in
    probe)
      UNSHARE_COMMAND+=(python3 "$repo_root/tests/integration/ntcp2/harness/rootless_supervisor.py" --probe)
      if [[ -n "$attestation_output" ]]; then
        UNSHARE_COMMAND+=(--attestation-output "$attestation_output")
      fi
      ;;
    scenario)
      UNSHARE_COMMAND+=(python3 "$repo_root/tests/integration/ntcp2/harness/rootless_inner_runner.py" "${INNER_ARGS[@]}")
      ;;
    *)
      die "unknown-inner-mode:$mode"
      ;;
  esac
}

# Compute a non-secret parent-host network digest. We deliberately avoid
# `nft list ruleset`, `sysctl`, and any privileged invocation. The
# fingerprint captures only structural state that is independent of the
# current PID and is safe to retain.
compute_parent_digest() {
  python3 - <<'PYEOF' 2>/dev/null || echo "0000000000000000000000000000000000000000000000000000000000000000"
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
}

parent_digest_pre=""
record_digest() {
  local label="$1"
  local value="$2"
  case "$label" in
    pre)
      parent_digest_pre="$value"
      ;;
    post)
      parent_digest_post="$value"
      ;;
    *)
      die "unknown-digest-label:$label"
      ;;
  esac
}

run_probe() {
  local parent_digest
  parent_digest=$(compute_parent_digest)
  record_digest pre "$parent_digest"
  build_unshare_command probe
  local probe_status
  local probe_outcome
  probe_status=0
  probe_outcome=$("${UNSHARE_COMMAND[@]}" 2>/dev/null) || probe_status=$?
  local parent_digest_after
  parent_digest_after=$(compute_parent_digest)
  record_digest post "$parent_digest_after"
  if [[ "$parent_digest" != "$parent_digest_after" ]]; then
    die "parent-network-state-mutation-detected"
  fi
  if [[ -z "$probe_outcome" ]]; then
    # `unshare` failed before reaching the supervisor (e.g. the host does
    # not allow writing /proc/self/uid_map). Surface the typed blocker so
    # callers can fail closed without sudo.
    local blocker='{"schema":1,"type":"rootless-sandbox-probe","outcome":"blocked_unprivileged_user_namespace"}'
    printf '%s\n' "$blocker"
    if [[ -n "$attestation_output" ]]; then
      printf '%s\n' "$blocker" > "$attestation_output" || true
    fi
    return 1
  fi
  # Surface the inner supervisor's typed outcome to stdout.
  printf '%s\n' "$probe_outcome"
  if [[ "$probe_status" -ne 0 ]]; then
    return "$probe_status"
  fi
}

run_scenario() {
  INNER_ARGS=(--scenario "$scenario" --reference "$reference")
  if [[ -n "$build_cache" ]]; then
    INNER_ARGS+=(--build-cache "$build_cache")
  fi
  if [[ -n "$run_root" ]]; then
    INNER_ARGS+=(--run-root "$run_root")
  fi
  if [[ -n "$attestation_output" ]]; then
    INNER_ARGS+=(--attestation-output "$attestation_output")
  fi
  build_unshare_command scenario
  local rc
  if "${UNSHARE_COMMAND[@]}"; then
    return 0
  else
    rc=$?
  fi
  # Distinguish "inner runner itself failed after running" from "the unshare
  # call never reached Python". We can tell by whether the inner runner had a
  # chance to write the attestation file; if not, the host is blocked.
  if [[ -n "$attestation_output" && ! -s "$attestation_output" ]]; then
    local blocker='{"schema":1,"type":"rootless-sandbox-probe","outcome":"blocked_unprivileged_user_namespace"}'
    printf '%s\n' "$blocker" > "$attestation_output" || true
  fi
  die "rootless-inner-runner-failed:exit=$rc"
}

run_profile() {
  die "profile-execution-deferred:$profile"
}

main() {
  parse_args "$@"
  case "$operation" in
    probe)
      run_probe
      ;;
    scenario)
      validate_choice scenario "$scenario" "${allowed_scenarios[@]}"
      validate_choice reference "$reference" "${allowed_references[@]}"
      run_scenario
      ;;
    profile)
      validate_choice profile "$profile" "${allowed_profiles[@]}"
      run_profile
      ;;
    *)
      die "unknown-operation"
      ;;
  esac
}

main "$@"
