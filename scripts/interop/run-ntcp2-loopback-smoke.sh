#!/usr/bin/env bash
# Plan 069 host-compatible NTCP2 loopback smoke entry point.
#
# Thin shell wrapper around ``tests/integration/ntcp2/harness/loopback_smoke.py``.
# The script locates the repository root, validates the required inputs
# are present, invokes the Python runner, and forwards its exit status.
# It never builds i2pd, never modifies cryptography, never fetches sources,
# and never touches sudo/namespaces/VMs/public network.
set -euo pipefail

usage='usage: run-ntcp2-loopback-smoke.sh --direction <i2pr-to-i2pd-ipv4|i2pd-to-i2pr-ipv4> --reference-driver <path> --reference-build-manifest <path> --reference-source-lock <path> --output <path> --source-commit <40-hex> [--network-audit-mode auto|strace|configuration-only] [--diagnostics-mode off|sanitized]'

direction=""
reference_driver=""
reference_build_manifest=""
reference_source_lock=""
output=""
source_commit=""
network_audit_mode="auto"
diagnostics_mode="off"
run_timeout_seconds=""
readiness_timeout_seconds=""
handshake_timeout_seconds=""
data_timeout_seconds=""

while (($#)); do
  case "$1" in
    --direction)
      (($# >= 2)) || { printf '%s\n' "$usage" >&2; exit 2; }
      direction=$2
      shift
      ;;
    --reference-driver)
      (($# >= 2)) || { printf '%s\n' "$usage" >&2; exit 2; }
      reference_driver=$2
      shift
      ;;
    --reference-build-manifest)
      (($# >= 2)) || { printf '%s\n' "$usage" >&2; exit 2; }
      reference_build_manifest=$2
      shift
      ;;
    --reference-source-lock)
      (($# >= 2)) || { printf '%s\n' "$usage" >&2; exit 2; }
      reference_source_lock=$2
      shift
      ;;
    --output)
      (($# >= 2)) || { printf '%s\n' "$usage" >&2; exit 2; }
      output=$2
      shift
      ;;
    --source-commit)
      (($# >= 2)) || { printf '%s\n' "$usage" >&2; exit 2; }
      source_commit=$2
      shift
      ;;
    --network-audit-mode)
      (($# >= 2)) || { printf '%s\n' "$usage" >&2; exit 2; }
      network_audit_mode=$2
      shift
      ;;
    --diagnostics-mode)
      (($# >= 2)) || { printf '%s\n' "$usage" >&2; exit 2; }
      diagnostics_mode=$2
      shift
      ;;
    --run-timeout-seconds)
      (($# >= 2)) || { printf '%s\n' "$usage" >&2; exit 2; }
      run_timeout_seconds=$2
      shift
      ;;
    --readiness-timeout-seconds)
      (($# >= 2)) || { printf '%s\n' "$usage" >&2; exit 2; }
      readiness_timeout_seconds=$2
      shift
      ;;
    --handshake-timeout-seconds)
      (($# >= 2)) || { printf '%s\n' "$usage" >&2; exit 2; }
      handshake_timeout_seconds=$2
      shift
      ;;
    --data-timeout-seconds)
      (($# >= 2)) || { printf '%s\n' "$usage" >&2; exit 2; }
      data_timeout_seconds=$2
      shift
      ;;
    -h|--help)
      printf '%s\n' "$usage"
      exit 0
      ;;
    *) printf 'unknown option: %s\n%s\n' "$1" "$usage" >&2; exit 2 ;;
  esac
  shift
done

[[ -n "$direction" ]] || { printf 'missing --direction\n%s\n' "$usage" >&2; exit 2; }
[[ -n "$reference_driver" ]] || { printf 'missing --reference-driver\n%s\n' "$usage" >&2; exit 2; }
[[ -n "$reference_build_manifest" ]] || { printf 'missing --reference-build-manifest\n%s\n' "$usage" >&2; exit 2; }
[[ -n "$reference_source_lock" ]] || { printf 'missing --reference-source-lock\n%s\n' "$usage" >&2; exit 2; }
[[ -n "$output" ]] || { printf 'missing --output\n%s\n' "$usage" >&2; exit 2; }
[[ -n "$source_commit" ]] || { printf 'missing --source-commit\n%s\n' "$usage" >&2; exit 2; }

case "$direction" in
  i2pr-to-i2pd-ipv4|i2pd-to-i2pr-ipv4) ;;
  *) printf 'invalid --direction: %s\n' "$direction" >&2; exit 2 ;;
esac

case "$diagnostics_mode" in
  off|sanitized) ;;
  *) printf 'invalid --diagnostics-mode: %s\n' "$diagnostics_mode" >&2; exit 2 ;;
esac

case "$network_audit_mode" in
  auto|strace|configuration-only) ;;
  *) printf 'invalid --network-audit-mode: %s\n' "$network_audit_mode" >&2; exit 2 ;;
esac

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)

[[ -f "$reference_driver" ]] || { printf 'reference driver binary not found: %s\n' "$reference_driver" >&2; exit 2; }
[[ -f "$reference_build_manifest" ]] || { printf 'reference build manifest not found: %s\n' "$reference_build_manifest" >&2; exit 2; }
[[ -f "$reference_source_lock" ]] || { printf 'reference source lock not found: %s\n' "$reference_source_lock" >&2; exit 2; }

mkdir -p "$(dirname "$output")"

declare -a python_args=(
  "$root/tests/integration/ntcp2/harness/loopback_smoke.py"
  --direction "$direction"
  --reference-driver "$reference_driver"
  --reference-build-manifest "$reference_build_manifest"
  --reference-source-lock "$reference_source_lock"
  --output "$output"
  --source-commit "$source_commit"
  --network-audit-mode "$network_audit_mode"
  --diagnostics-mode "$diagnostics_mode"
)

if [[ -n "$run_timeout_seconds" ]]; then
  python_args+=(--run-timeout-seconds "$run_timeout_seconds")
fi
if [[ -n "$readiness_timeout_seconds" ]]; then
  python_args+=(--readiness-timeout-seconds "$readiness_timeout_seconds")
fi
if [[ -n "$handshake_timeout_seconds" ]]; then
  python_args+=(--handshake-timeout-seconds "$handshake_timeout_seconds")
fi
if [[ -n "$data_timeout_seconds" ]]; then
  python_args+=(--data-timeout-seconds "$data_timeout_seconds")
fi

exec python3 "${python_args[@]}"
