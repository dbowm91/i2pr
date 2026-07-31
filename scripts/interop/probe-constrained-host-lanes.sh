#!/usr/bin/env bash
# Plan 077 read-only constrained-host capability probe.
#
# The probe never installs packages, changes host policy/networking, escalates,
# starts a router, or treats a capability as a qualification.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
probe_output="${root}/target/interop/lane/probe.json"
qualification_output="${root}/target/interop/lane/qualification.json"

while (($#)); do
  case "$1" in
    --output)
      (($# >= 2)) || { echo "--output requires a path" >&2; exit 2; }
      probe_output=$2
      shift
      ;;
    --qualification-output)
      (($# >= 2)) || { echo "--qualification-output requires a path" >&2; exit 2; }
      qualification_output=$2
      shift
      ;;
    --repo-root)
      (($# >= 2)) || { echo "--repo-root requires a path" >&2; exit 2; }
      root=$2
      shift
      ;;
    -h|--help)
      echo "usage: $0 [--output <probe.json>] [--qualification-output <qualification.json>] [--repo-root <path>]"
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
  esac
  shift
done

exec python3 "$root/tests/integration/ntcp2/harness/execution_lane.py" \
  --repo-root "$root" \
  --output "$probe_output" \
  --qualification-output "$qualification_output"
