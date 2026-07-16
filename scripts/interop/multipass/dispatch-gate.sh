#!/usr/bin/env bash
# Plan 051 (troubleshooting): dispatch canonical Plan 040/043 gate scripts
# inside an owned Multipass guest via `multipass exec`. The canonical gate
# order is the Plan 043 sequence (contract, reference-build,
# reference-offline-reuse, environment-smoke, reference-crosscheck-ipv4,
# i2pr-handshake-smoke-ipv4, full-matrix, evidence-validation,
# cleanup-verification). Each step is `multipass exec`-wrapped.
#
# This script does not advertise NTCP2 support, does not satisfy Plan 045
# directional predicates, and does not close Milestone 3. It is the
# troubleshooting bridge that lets the Plan 040/043 gate order run inside a
# disposable Multipass guest whose kernel policy is permissive and whose
# non-interactive sudo is available.
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_dir/common.sh"

profile=""
run_id=""
instance_name=""
offline=1
while (($#)); do
  case "$1" in
    --profile) (($# >= 2)) || die "--profile requires a value"; profile=$2; shift ;;
    --run-id) (($# >= 2)) || die "--run-id requires a value"; run_id=$2; shift ;;
    --instance-name) (($# >= 2)) || die "--instance-name requires a value"; instance_name=$2; shift ;;
    --online) offline=0 ;;
    --help|-h)
      printf 'usage: dispatch-gate.sh --profile <name> --run-id <safe-id> --instance-name <name> [--online]\n'
      printf 'profiles: environment-smoke reference-crosscheck-ipv4 handshake-smoke full evidence-validation cleanup-verification\n'
      exit 0
      ;;
    *) die "unknown argument: $1" ;;
  esac
  shift
done

[[ -n "$profile" ]] || die "--profile is required"
[[ -n "$run_id" ]] || die "--run-id is required"
[[ -n "$instance_name" ]] || die "--instance-name is required"

case "$profile" in
  environment-smoke|reference-crosscheck-ipv4|handshake-smoke|full|evidence-validation|cleanup-verification) ;;
  *) die "unknown profile: $profile" ;;
esac

require_command multipass
multipass info "$instance_name" --format json >/dev/null 2>&1 || die "instance not found: $instance_name"

acquire_lifecycle_lock

guest_repo_root="$guest_repo_root"
guest_target="$guest_repo_root/target/interop"
guest_scripts="$guest_repo_root/scripts/interop"

guest_exec_root() {
  multipass exec "$instance_name" -- sudo -n "$@"
}
guest_exec_user() {
  multipass exec "$instance_name" -- sudo -n -u "$guest_execution_user" "$@"
}

profile_step() {
  local step_name=$1
  local gate_script=$2
  shift 2
  printf '[%s] %s\n' "$profile" "$step_name"
  local interpreter
  case "$gate_script" in
    *.py) interpreter="python3" ;;
    *) interpreter="bash" ;;
  esac
  if guest_exec_root "$interpreter" "$guest_scripts/$gate_script" "$@" >"$instance_state_dir/$profile-$step_name.log" 2>&1; then
    printf '  %s ok\n' "$step_name"
    return 0
  fi
  local status=$?
  printf '  %s failed (exit %d)\n' "$step_name" "$status"
  return "$status"
}

make_cache_user_readable() {
  printf '[%s] %s\n' "$profile" "make-cache-user-readable"
  if guest_exec_root bash -c "chown -R '$guest_execution_user:$guest_execution_user' '$guest_target/cache' '$guest_target/build' && chmod -R u+rwX,g+rX,o+rX '$guest_target/cache' '$guest_target/build'" >"$instance_state_dir/$profile-make-cache-user-readable.log" 2>&1; then
    printf '  %s ok\n' "make-cache-user-readable"
    return 0
  fi
  local status=$?
  printf '  %s failed (exit %d)\n' "make-cache-user-readable" "$status"
  return "$status"
}

case "$profile" in
  environment-smoke)
    profile_step pre-install ubuntu/check-host.sh --pre-install \
      --metadata "$guest_target/build/host-metadata.json" || exit $?
    profile_step post-install ubuntu/check-host.sh --post-install \
      --metadata "$guest_target/build/host-metadata.json" || exit $?
    ;;
  reference-crosscheck-ipv4)
    profile_step reference-build build-references.sh --force-rebuild || exit $?
    make_cache_user_readable || exit $?
    profile_step cache-manifest cache-manifest.py --verify || exit $?
    profile_step offline-reuse offline-reuse.sh || exit $?
    profile_step reference-crosscheck-ipv4 run-gate.sh --profile reference-crosscheck-ipv4 --offline || exit $?
    ;;
  handshake-smoke)
    profile_step reference-build build-references.sh --force-rebuild || exit $?
    make_cache_user_readable || exit $?
    profile_step cache-manifest cache-manifest.py --verify || exit $?
    profile_step offline-reuse offline-reuse.sh || exit $?
    profile_step reference-crosscheck-ipv4 run-gate.sh --profile reference-crosscheck-ipv4 --offline || exit $?
    profile_step handshake-smoke run-gate.sh --profile handshake-smoke --offline || exit $?
    ;;
  full)
    profile_step pre-install ubuntu/check-host.sh --pre-install \
      --metadata "$guest_target/build/host-metadata.json" || exit $?
    profile_step post-install ubuntu/check-host.sh --post-install \
      --metadata "$guest_target/build/host-metadata.json" || exit $?
    profile_step setup-host ubuntu/setup-host.sh || exit $?
    profile_step record-baseline verify-clean-host.sh --record-baseline || exit $?
    profile_step reference-build build-references.sh --force-rebuild || exit $?
    make_cache_user_readable || exit $?
    profile_step cache-manifest cache-manifest.py --verify || exit $?
    profile_step offline-reuse offline-reuse.sh || exit $?
    profile_step environment-smoke run-gate.sh --profile environment-smoke --offline || exit $?
    profile_step reference-crosscheck-ipv4 run-gate.sh --profile reference-crosscheck-ipv4 --offline || exit $?
    profile_step handshake-smoke run-gate.sh --profile handshake-smoke --offline || exit $?
    profile_step full-matrix run-gate.sh --profile full --offline || exit $?
    ;;
  evidence-validation)
    guest_exec_root python3 "$guest_scripts/validate-evidence.py" >"$instance_state_dir/$profile-validate.log" 2>&1 \
      || exit $?
    guest_exec_root python3 "$guest_scripts/aggregate-evidence.py" --profile handshake-smoke \
      >"$instance_state_dir/$profile-aggregate.log" 2>&1 || exit $?
    ;;
  cleanup-verification)
    guest_exec_root bash "$guest_scripts/cleanup.sh" >"$instance_state_dir/$profile-cleanup.log" 2>&1 || exit $?
    guest_exec_root bash "$guest_scripts/verify-clean-host.sh" --verify \
      >"$instance_state_dir/$profile-verify.log" 2>&1 || exit $?
    ;;
esac

printf '[%s] gate dispatch complete\n' "$profile"