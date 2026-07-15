#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_dir/lib/common.sh"
ensure_target_dirs

find "$INTEROP_TARGET/evidence" -mindepth 1 -maxdepth 1 -type f -delete
find "$INTEROP_TARGET/build" -mindepth 1 -maxdepth 1 -type f -name 'clean-host-*.json' -delete
rm -rf "$BUILD_ROOT/gates"
if find "$RUNS_ROOT" -mindepth 1 -print -quit | grep -q .; then
  "$script_dir/cleanup.sh"
fi
printf 'reset generated Plan 043 lane state\n'
