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
      printf 'profiles: environment-smoke reference-crosscheck-ipv4 handshake-smoke handshake-smoke-rootless full evidence-validation cleanup-verification\n'
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
  environment-smoke|reference-crosscheck-ipv4|handshake-smoke|handshake-smoke-rootless|full|evidence-validation|cleanup-verification) ;;
  *) die "unknown profile: $profile" ;;
esac

require_command multipass
multipass info "$instance_name" --format json >/dev/null 2>&1 || die "instance not found: $instance_name"

acquire_lifecycle_lock

guest_repo_root="$guest_repo_root"
guest_target="$guest_repo_root/target/interop"
guest_scripts="$guest_repo_root/scripts/interop"

guest_exec_root() {
  multipass exec "$instance_name" -- sudo -n \
    env "CARGO_HOME=/home/$guest_execution_user/.cargo" "RUSTUP_HOME=/home/$guest_execution_user/.rustup" "$@"
}
guest_exec_user() {
  multipass exec "$instance_name" -- sudo -n -u "$guest_execution_user" \
    env "CARGO_HOME=/home/$guest_execution_user/.cargo" "RUSTUP_HOME=/home/$guest_execution_user/.rustup" "$@"
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
  local cache_root="$guest_target/cache"
  local build_root="$guest_target/build"
  local exec_user="$guest_execution_user"
  if guest_exec_root bash -c "
    set +e
    chown -R '${exec_user}:${exec_user}' '${cache_root}'
    chmod -R u+rwX,g+rX,o+rX '${cache_root}'
    for f in '${build_root}'/*.json '${build_root}'/*.txt; do
      [[ -f \"\$f\" ]] && chown '${exec_user}:${exec_user}' \"\$f\" && chmod 0644 \"\$f\"
    done
    exit 0
  " >"$instance_state_dir/$profile-make-cache-user-readable.log" 2>&1; then
    printf '  %s ok\n' "make-cache-user-readable"
    return 0
  fi
  local status=$?
  printf '  %s failed (exit %d)\n' "make-cache-user-readable" "$status"
  return "$status"
}

reset_reference_artifacts() {
  printf '[%s] %s\n' "$profile" "reset-reference-artifacts"
  if guest_exec_root bash -c "rm -rf '$guest_target/build/sources' '$guest_target/cache' '$guest_target/build/reference-cache-manifest.json' '$guest_target/build/reference-build-summary.json' '$guest_target/build/host-metadata.json' '$guest_target/build/java-i2p-summary.txt' '$guest_target/build/i2pd-summary.txt' '$guest_target/build/objects' '$guest_target/build/install' '$guest_target/build/downloads' '$guest_target/build/logs' '$guest_target/build/tools' '$guest_repo_root/tests/integration/ntcp2/harness/__pycache__'" >"$instance_state_dir/$profile-reset-reference-artifacts.log" 2>&1; then
    printf '  %s ok\n' "reset-reference-artifacts"
    return 0
  fi
  local status=$?
  printf '  %s failed (exit %d)\n' "reset-reference-artifacts" "$status"
  return "$status"
}

install_guest_rust_toolchain() {
  printf '[%s] %s\n' "$profile" "install-guest-rust-toolchain"
  if guest_exec_root bash -c "set -euo pipefail; if ! command -v cargo >/dev/null 2>&1; then install -d -m 0755 /usr/local/bin; ln -sf /home/$guest_execution_user/.cargo/bin/cargo /usr/local/bin/cargo; ln -sf /home/$guest_execution_user/.cargo/bin/rustc /usr/local/bin/rustc; install -d -m 0755 /etc/sudoers.d; printf 'Defaults env_keep += \"CARGO_HOME RUSTUP_HOME\"\n' > /etc/sudoers.d/i2pr-rust; chmod 0440 /etc/sudoers.d/i2pr-rust; visudo -c -f /etc/sudoers.d/i2pr-rust >/dev/null; fi" >"$instance_state_dir/$profile-install-guest-rust-toolchain.log" 2>&1; then
    printf '  %s ok\n' "install-guest-rust-toolchain"
    return 0
  fi
  local status=$?
  printf '  %s failed (exit %d)\n' "install-guest-rust-toolchain" "$status"
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
    install_guest_rust_toolchain || exit $?
    reset_reference_artifacts || exit $?
    profile_step reference-build build-references.sh --force-rebuild || exit $?
    profile_step cache-manifest cache-manifest.py --verify || exit $?
    profile_step offline-reuse offline-reuse.sh || exit $?
    make_cache_user_readable || exit $?
    profile_step reference-crosscheck-ipv4 run-gate.sh --profile reference-crosscheck-ipv4 --offline || exit $?
    ;;
  handshake-smoke)
    install_guest_rust_toolchain || exit $?
    reset_reference_artifacts || exit $?
    profile_step reference-build build-references.sh --force-rebuild || exit $?
    profile_step cache-manifest cache-manifest.py --verify || exit $?
    profile_step offline-reuse offline-reuse.sh || exit $?
    make_cache_user_readable || exit $?
    profile_step reference-crosscheck-ipv4 run-gate.sh --profile reference-crosscheck-ipv4 --offline || exit $?
    profile_step handshake-smoke run-gate.sh --profile handshake-smoke --offline || exit $?
    ;;
  handshake-smoke-rootless)
    install_guest_rust_toolchain || exit $?
    reset_reference_artifacts || exit $?
    profile_step reference-build build-references.sh --force-rebuild || exit $?
    profile_step cache-manifest cache-manifest.py --verify || exit $?
    profile_step offline-reuse offline-reuse.sh || exit $?
    make_cache_user_readable || exit $?
    profile_step prepare-offline multipass/prepare-offline.sh || exit $?
    for scenario in i2pr-to-java-ipv4 java-to-i2pr-ipv4 i2pr-to-i2pd-ipv4 i2pd-to-i2pr-ipv4; do
      printf '[%s] %s\n' "$profile" "direction-$scenario"
      if bash "$script_dir/run-direction.sh" --scenario "$scenario" >"$instance_state_dir/$profile-direction-$scenario.log" 2>&1; then
        printf '  %s ok\n' "direction-$scenario"
      else
        printf '  %s failed\n' "direction-$scenario"
        cat "$instance_state_dir/$profile-direction-$scenario.log" >&2 || true
      fi
    done
    profile_step export-evidence export-evidence.sh || exit $?
    ;;
  full)
    profile_step pre-install ubuntu/check-host.sh --pre-install \
      --metadata "$guest_target/build/host-metadata.json" || exit $?
    profile_step post-install ubuntu/check-host.sh --post-install \
      --metadata "$guest_target/build/host-metadata.json" || exit $?
    profile_step setup-host ubuntu/setup-host.sh || exit $?
    profile_step record-baseline verify-clean-host.sh --record-baseline || exit $?
    install_guest_rust_toolchain || exit $?
    reset_reference_artifacts || exit $?
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