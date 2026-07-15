#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_dir/lib/common.sh"

run_root=${1:-$RUNS_ROOT}
if [[ -d "$run_root" ]]; then
  run_root=$(cd "$run_root" && pwd)
elif [[ "$run_root" == "$RUNS_ROOT" ]]; then
  install -d -m 0755 "$RUNS_ROOT"
else
  die "cleanup run root does not exist"
fi
[[ "$run_root" == "$RUNS_ROOT" || "$run_root" == "$RUNS_ROOT"/* ]] \
  || die "cleanup refuses a path outside target/interop/runs"
[[ -d "$run_root" ]] || die "cleanup run root does not exist"

started=0
terminated=0
forced=0
namespace_deleted=0
interface_deleted=0
failures=0
prefix=()
[[ "$EUID" -eq 0 ]] || prefix=(sudo -n)

kill_pid() {
  local pid=$1
  [[ "$pid" =~ ^[0-9]+$ ]] || return 0
  if kill -0 "$pid" >/dev/null 2>&1; then
    started=$((started + 1))
    kill -TERM "$pid" >/dev/null 2>&1 || true
    terminated=$((terminated + 1))
    for _ in $(seq 1 20); do
      kill -0 "$pid" >/dev/null 2>&1 || return 0
      sleep 0.1
    done
    kill -KILL "$pid" >/dev/null 2>&1 || true
    forced=$((forced + 1))
    for _ in $(seq 1 20); do
      kill -0 "$pid" >/dev/null 2>&1 || return 0
      sleep 0.1
    done
    failures=$((failures + 1))
  fi
}

declare -A seen_pids=()
for pid_file in "$run_root"/*/pids/*.pid; do
  [[ -f "$pid_file" ]] || continue
  pid=$(sed -n '1p' "$pid_file")
  if [[ -n "${seen_pids[$pid]+x}" ]]; then continue; fi
  seen_pids[$pid]=1
  kill_pid "$pid"
done

mapfile -t namespaces < <("${prefix[@]}" ip netns list 2>/dev/null | awk '$1 ~ /^(i2pr|ref)-[A-Za-z0-9-]+$/ {print $1}')
for namespace in "${namespaces[@]}"; do
  mapfile -t namespace_pids < <("${prefix[@]}" ip netns pids "$namespace" 2>/dev/null || true)
  for pid in "${namespace_pids[@]}"; do
    [[ -n "${seen_pids[$pid]+x}" ]] || { seen_pids[$pid]=1; kill_pid "$pid"; }
  done
  if "${prefix[@]}" ip netns del "$namespace" >/dev/null 2>&1; then
    namespace_deleted=$((namespace_deleted + 1))
  else
    failures=$((failures + 1))
  fi
done

mapfile -t interfaces < <("${prefix[@]}" ip -o link show 2>/dev/null | awk -F': ' '$2 ~ /^(i2pr-v|ref-v)/ {sub(/@.*/, "", $2); print $2}')
for interface in "${interfaces[@]}"; do
  if "${prefix[@]}" ip link del "$interface" >/dev/null 2>&1; then
    interface_deleted=$((interface_deleted + 1))
  else
    failures=$((failures + 1))
  fi
done

for child in "$run_root"/*; do
  [[ -d "$child" ]] || continue
  [[ "$child" == "$run_root"/* ]] || { failures=$((failures + 1)); continue; }
  rm -rf "$child"
done

if mapfile -t remaining < <("${prefix[@]}" ip netns list 2>/dev/null | awk '$1 ~ /^(i2pr|ref)-[A-Za-z0-9-]+$/ {print $1}'); then
  ((${#remaining[@]} == 0)) || failures=$((failures + ${#remaining[@]}))
fi
if mapfile -t remaining_links < <("${prefix[@]}" ip -o link show 2>/dev/null | awk -F': ' '$2 ~ /^(i2pr-v|ref-v)/ {print $2}'); then
  ((${#remaining_links[@]} == 0)) || failures=$((failures + ${#remaining_links[@]}))
fi
if pgrep -af '(^|/)(i2pd|i2pr-interop|i2prouter)( |$)' >/dev/null 2>&1; then failures=$((failures + 1)); fi
if find "$run_root" -mindepth 1 -print -quit 2>/dev/null | grep -q .; then failures=$((failures + 1)); fi

printf '{"schema":2,"started":%d,"terminated":%d,"forced":%d,"namespaces_deleted":%d,"interfaces_deleted":%d,"failures":%d}\n' \
  "$started" "$terminated" "$forced" "$namespace_deleted" "$interface_deleted" "$failures"
((failures == 0))
