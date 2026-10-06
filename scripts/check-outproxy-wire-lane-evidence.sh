#!/usr/bin/env bash
# Plan 342 static evidence-integrity check for the self-composed loopback
# outproxy wire lane.
#
# ## What this asserts
#
# The lane in `crates/i2pr-daemon/tests/outproxy_loopback_wire.rs` is
# Plan 342's *runtime* evidence: the request-path guard
# (`scripts/check-outproxy-request-path.sh`) pins the shape of the code, and
# this pins the properties of the evidence itself, so a green lane cannot be
# produced by something weaker than it looks.
#
#   1. every required row exists in the lane source, by name;
#   2. no required row is `#[ignore]`d, `#[should_panic]`d, or gated behind an
#      environment variable — an environment-gated row is a row that silently
#      skips in an ordinary run, which is how this repo's external lanes fail
#      closed and why an in-tree lane must not be allowed to borrow the idiom;
#   3. the lane opens no non-loopback socket and resolves no name, so
#      "a clearnet request succeeded" cannot mean "the test process held a
#      clearnet capability" — which would invert Plan 342 invariant 1 at the
#      layer meant to enforce it;
#   4. no dynamic library loading and no command execution anywhere in the lane
#      *or* in the production modules it exercises, which is Plan 342's
#      "no plugin loading and no command execution from control input" row
#      stated as a checked property;
#   5. the lane's own failure idioms are absent — `|| true`,
#      `continue-on-error`, `--nocapture` used as a pass condition, or an
#      `assert` that accepts `""` as evidence;
#   6. the static request-path guard still passes, because a green lane over a
#      mutated request path proves nothing.
#
# ## Why the row list is repeated here rather than discovered
#
# Discovering the rows from the source would make this check tautological: it
# would verify whatever happens to be there. The list is the *requirement*, and
# a row that is deleted, renamed, or downgraded makes this script fail.
#
# ## Why there is no `record ... passed` idiom here
#
# The external lanes record rows into an evidence file from a harness script,
# which is why `check-service-tunnel-acceptance-evidence.sh` has to hunt for
# hard-coded `record` calls. This lane is an ordinary `cargo test` binary: its
# evidence is the test runner's own exit status and per-row output. There is no
# pass/fail bookkeeping for this check to police, which is strictly better, and
# it is why the in-tree lane is preferred over a harness wherever one will do.
#
# Usage: bash scripts/check-outproxy-wire-lane-evidence.sh
#
# Run with `--mutation-table` to reproduce the negative test.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
exec python3 "${root}/scripts/check-outproxy-wire-lane-evidence.py" "$@"