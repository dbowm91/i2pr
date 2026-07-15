#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
run_root="${1:-$root/target/interop/runs}"
[[ "$run_root" == "$root/target/interop/runs"* ]] || { printf 'cleanup refuses a path outside target/interop/runs\n' >&2; exit 2; }
for pid_file in "$run_root"/*/pids/*.pid; do
  [[ -f "$pid_file" ]] || continue
  pid=$(sed -n '1p' "$pid_file")
  [[ "$pid" =~ ^[0-9]+$ ]] || continue
  kill "$pid" >/dev/null 2>&1 || true
done
if command -v ip >/dev/null 2>&1; then
  prefix=()
  [[ "$EUID" -eq 0 ]] || prefix=(sudo -n)
  for namespace in $("${prefix[@]}" ip netns list 2>/dev/null | awk '$1 ~ /^i2pr-/ || $1 ~ /^ref-/ {print $1}'); do
    "${prefix[@]}" ip netns del "$namespace" >/dev/null 2>&1 || true
  done
fi
printf '{"schema":1,"cleanup":"completed","run_root":"target/interop/runs"}\n'
