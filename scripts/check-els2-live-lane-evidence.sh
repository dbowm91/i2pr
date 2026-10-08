#!/usr/bin/env bash
# Plan 381 static evidence-integrity check for the live ELS2 external driver
# lane.
#
# ## What this asserts
#
# The lane in `crates/i2pr-daemon/tests/els2_i2pd_external.rs` is Plan 381's
# *runtime* evidence: the consumer-path guard
# (`scripts/check-encrypted-service-consumer-caller.sh`) pins the shape of the
# code, and this pins the properties of the evidence itself, so a green lane
# cannot be produced by something weaker than it looks.
#
#   1. the one driver test exists and is `#[ignore]`-gated for the exact
#      pinned environment (an environment-gated external lane must skip
#      ordinarily and fail on missing env, never silently pass);
#   2. every evidence key the WP4 matrix rows record exists in the driver
#      source, by name;
#   3. the unit rows pinning lane-found defects exist and are NOT gated;
#   4. the runner's frozen pin/version/attempt-budget literals and full row
#      surface hold;
#   5. the driver holds no out-of-lane capability and stays loopback;
#   6. parked rows (reverse, i2pr-side authority) stay parked: writing one
#      fails this check until the absence list and closure record are updated;
#   7. every production property the lane depends on still holds, each named
#      with the defect its removal would reintroduce;
#   8. the static consumer-path guard still passes, because a green lane over
#      a mutated request path proves nothing.
#
# ## Why the row list is repeated here rather than discovered
#
# Discovering the rows from the source would make this check tautological: it
# would verify whatever happens to be there. The list is the *requirement*, and
# a row that is deleted, renamed, or downgraded makes this script fail.
#
# Usage: bash scripts/check-els2-live-lane-evidence.sh
#
# Run with `--self-test` for the checker's own cheap gate, `--mutation-table`
# to reproduce the negative test.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
exec python3 "${root}/scripts/check-els2-live-lane-evidence.py" "$@"
