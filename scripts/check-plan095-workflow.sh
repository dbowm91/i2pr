#!/usr/bin/env bash
# Plan 096/097 pre-dispatch workflow audit.
#
# This script is a narrowly-named static boundary check that
# verifies the four demonstrated Plan 095 workflow execution
# defects (Plan 096) and the two narrow Plan 097 corrective-pass
# defects (artifact-path ownership and cleanup verification) have
# been corrected before any manual dispatch. The script does not
# attempt to emulate the entire GitHub Actions expression engine;
# it is a repository-specific guard for the Plan 095 workflow
# contract.
#
# Exit code 0 means the workflow is dispatch-ready; nonzero
# means a Plan 096/097 corrective commit is required before any
# live Plan 095 evidence run.

set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
workflow="$root/.github/workflows/ntcp2-interop-host-loopback-development.yml"

if [[ ! -f "$workflow" ]]; then
    echo "Plan 095 workflow is missing" >&2
    exit 1
fi

# WP2: i2pr Cargo invocation must use an explicit
# --manifest-path and an explicit --target-dir.
if ! grep -q -- "--manifest-path" "$workflow"; then
    echo "Plan 096: i2pr build is missing --manifest-path" >&2
    exit 2
fi
if ! grep -q -- "--target-dir" "$workflow"; then
    echo "Plan 096: i2pr build is missing --target-dir" >&2
    exit 3
fi
if grep -Eq "^[[:space:]]+cp[[:space:]]+target/release/i2pr-interop\b" "$workflow"; then
    echo "Plan 096: i2pr binary is copied from a relative target path" >&2
    exit 4
fi

# Plan 097 WP2: the producer, manifest, verifier, and uploader must
# all resolve to the canonical absolute $BUILD_OUTPUT path. A
# CWD-relative ``output/...`` operation is only allowed when the
# step explicitly proves ``pwd == $BUILD_DIR`` first. The Plan
# 096 relative-path checks are superseded by these Plan 097
# absolute-path checks.
if ! grep -q 'BUILD_OUTPUT="$BUILD_DIR/output"' "$workflow"; then
    echo "Plan 097: canonical BUILD_OUTPUT variable must be defined" >&2
    exit 5
fi
# The producer must copy to the canonical absolute path. Reject
# any producer that writes to ``output/i2pr-interop`` unless the
# step explicitly establishes BUILD_DIR as its working directory
# first.
if grep -Eq 'cp[[:space:]]+"\$I2PR_TARGET_DIR/release/i2pr-interop"[[:space:]]+output/i2pr-interop' "$workflow"; then
    echo "Plan 097: i2pr producer must not write to a relative output path" >&2
    exit 6
fi
# The producer step must explicitly include ``mkdir -p "$BUILD_OUTPUT"``
# rather than ``mkdir -p output``.
if ! grep -Eq '^[[:space:]]+mkdir -p "\$BUILD_OUTPUT"' "$workflow"; then
    echo "Plan 097: i2pr producer must create the canonical absolute output directory" >&2
    exit 7
fi
# The producer must verify the canonical artifact is regular,
# executable, and non-symlink at the canonical absolute path.
if ! grep -q 'test -f "$BUILD_OUTPUT/i2pr-interop"' "$workflow"; then
    echo "Plan 097: i2pr binary existence must be asserted at the canonical path" >&2
    exit 8
fi
if ! grep -q 'test -x "$BUILD_OUTPUT/i2pr-interop"' "$workflow"; then
    echo "Plan 097: i2pr binary executability must be asserted at the canonical path" >&2
    exit 9
fi
if ! grep -q 'test ! -L "$BUILD_OUTPUT/i2pr-interop"' "$workflow"; then
    echo "Plan 097: i2pr binary must be asserted non-symlink at the canonical path" >&2
    exit 10
fi
# The manifest hash input must reference the canonical absolute
# path.
if ! grep -q 'sha256sum "$BUILD_OUTPUT/i2pr-interop"' "$workflow"; then
    echo "Plan 097: i2pr manifest must hash the canonical absolute artifact" >&2
    exit 11
fi
# The verifier must consume the canonical absolute path rather
# than a bare ``output/i2pr-interop`` after a step-local cd.
if grep -q 'test -x output/i2pr-interop' "$workflow"; then
    echo "Plan 097: i2pr verifier must consume the canonical absolute artifact" >&2
    exit 12
fi
if grep -q 'test -f output/i2pr-interop' "$workflow"; then
    echo "Plan 097: i2pr verifier must consume the canonical absolute artifact" >&2
    exit 13
fi

# WP3: sanitized evidence must be disjoint from disposable run
# roots.
if grep -q "target/interop/plan095-instrumented/sanitized" "$workflow"; then
    echo "Plan 096: instrumented sanitized path is nested in disposable run root" >&2
    exit 14
fi
if grep -q "target/interop/plan095-control/sanitized" "$workflow"; then
    echo "Plan 096: control sanitized path is nested in disposable run root" >&2
    exit 15
fi
if ! grep -q "target/interop/plan095-evidence/instrumented" "$workflow"; then
    echo "Plan 096: instrumented sanitized path must live under plan095-evidence/" >&2
    exit 16
fi
if ! grep -q "target/interop/plan095-evidence/control" "$workflow"; then
    echo "Plan 096: control sanitized path must live under plan095-evidence/" >&2
    exit 17
fi
# The delete-raw-run-state steps must verify the sanitized tree
# survives the destructive cleanup.
delete_count=$(grep -c "delete-raw-run-state" "$workflow" || true)
if [[ "$delete_count" -lt 2 ]]; then
    echo "Plan 096: expected instrumented + control delete-raw-run-state steps" >&2
    exit 18
fi
# Both delete steps must include the post-cleanup existence check.
for line in $(grep -n "delete-raw-run-state" "$workflow" | cut -d: -f1); do
    body=$(sed -n "${line},/^      - name:\|^  [a-z]/{/^      - name:\|^  [a-z]/!p;}" "$workflow" | head -n 20)
    if ! grep -q "sanitized evidence tree was lost" <<<"$body"; then
        echo "Plan 096: delete-raw-run-state step at line $line must verify sanitized tree survives" >&2
        exit 19
    fi
done

# Plan 097 WP6: the disposable run-root cleanup must use strict
# recursive root deletion with an exact path guard, an unsuppressed
# absence assertion, and no descendant-only ``find -mindepth 1 -delete``.
if grep -q 'find "$PLAN095_RUN_ROOT" -mindepth 1 -delete' "$workflow"; then
    echo "Plan 097: cleanup must not use descendant-only deletion" >&2
    exit 20
fi
if ! grep -q 'rm -rf -- "$PLAN095_RUN_ROOT"' "$workflow"; then
    echo "Plan 097: cleanup must use 'rm -rf --' on the disposable run root" >&2
    exit 21
fi
if grep -Eq 'test[[:space:]]+![[:space:]]*-e[[:space:]]+"\$PLAN095_RUN_ROOT"[[:space:]]*\|\|[[:space:]]*true' "$workflow"; then
    echo "Plan 097: cleanup must not suppress the absence assertion with || true" >&2
    exit 22
fi
if ! grep -q 'refusing unexpected PLAN095_RUN_ROOT' "$workflow"; then
    echo "Plan 097: cleanup must include a refusal message for unexpected paths" >&2
    exit 23
fi
if ! grep -q '"${{ github.workspace }}/target/interop/plan095-instrumented") ;;' "$workflow"; then
    echo "Plan 097: instrumented cleanup path guard must allow the instrumented root" >&2
    exit 24
fi
if ! grep -q '"${{ github.workspace }}/target/interop/plan095-control") ;;' "$workflow"; then
    echo "Plan 097: control cleanup path guard must allow the control root" >&2
    exit 25
fi

# WP4: every embedded Python heredoc must import every module it
# uses. We use python3 to do the structural check so the rules
# stay aligned with the test_plan096.py contract.
WORKFLOW_PATH_FOR_PY="$workflow" python3 - <<'PY'
import os
import re
import sys

text = open(os.environ["WORKFLOW_PATH_FOR_PY"], encoding="utf-8").read()
pattern = re.compile(
    r"python3\s*-\s*<<\s*'(?P<marker>\w+)'\s*\n(?P<body>.*?)\n\s*PY\s*\n",
    re.DOTALL,
)
module_re = re.compile(r"(?<![\w.])(os|sys|json|pathlib|hashlib|argparse|re)\b")
failures = []
for match in pattern.finditer(text):
    body = match.group("body")
    used = set(module_re.findall(body))
    for module in sorted(used):
        import_pat = re.compile(
            rf"(?m)^\s*import\s+{module}\b|^\s*from\s+{module}\b"
        )
        if not import_pat.search(body):
            failures.append(
                f"heredoc uses {module!r} without importing it: {body[:80]!r}"
            )
if failures:
    print("Plan 096: embedded Python imports are incomplete:", file=sys.stderr)
    for entry in failures:
        print(f"  {entry}", file=sys.stderr)
    sys.exit(26)
PY

# WP5: i2pd source digest must enumerate tracked files only.
if grep -q "find i2pd -type f" "$workflow"; then
    echo "Plan 096: i2pd digest must not use 'find i2pd -type f'" >&2
    exit 27
fi
if ! grep -q "git -C i2pd ls-files" "$workflow"; then
    echo "Plan 096: i2pd digest must use 'git ls-files'" >&2
    exit 28
fi
if ! grep -q "git -C i2pd rev-parse HEAD" "$workflow"; then
    echo "Plan 096: pinned i2pd revision must be verified before digest/build" >&2
    exit 29
fi
if ! grep -q "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e" "$workflow"; then
    echo "Plan 096: pinned i2pd revision marker is missing" >&2
    exit 30
fi

# WP6: dependency graph, fail-closed live attempts, and no retry.
if ! grep -q "needs: \[build, forward-instrumented, forward-control\]" "$workflow"; then
    echo "Plan 096: validate-gate must depend on build + both live jobs" >&2
    exit 31
fi
# Live attempt must propagate the exit explicitly, never via
# ``|| echo``.
if grep -nE "^\s*[^#]*\|\| echo" "$workflow" >/dev/null; then
    echo "Plan 096: live attempt must not use '|| echo' to mask failure" >&2
    exit 32
fi
if ! grep -q "instrumented_rc=\$?" "$workflow"; then
    echo "Plan 096: instrumented live attempt must capture its exit" >&2
    exit 33
fi
if ! grep -q "control_rc=\$?" "$workflow"; then
    echo "Plan 096: control live attempt must capture its exit" >&2
    exit 34
fi
if grep -q "nick-fields/retry" "$workflow"; then
    echo "Plan 096: workflow must not introduce a retry action" >&2
    exit 35
fi
if grep -q "i2pd-to-i2pr-ipv4" "$workflow"; then
    echo "Plan 096: forward-only workflow must not invoke the reverse direction" >&2
    exit 36
fi
if grep -q "plan084_runner" "$workflow"; then
    echo "Plan 096: forward-only workflow must not import the reverse runner" >&2
    exit 37
fi
if ! grep -q "workflow_dispatch" "$workflow"; then
    echo "Plan 096: workflow must keep workflow_dispatch as the only trigger" >&2
    exit 38
fi
if ! grep -q "ubuntu-24.04" "$workflow"; then
    echo "Plan 096: workflow must keep ubuntu-24.04 as the runner label" >&2
    exit 39
fi
if ! grep -q "experimental-non-advertised" "$workflow"; then
    echo "Plan 096: workflow must mark NTCP2 as experimental-non-advertised" >&2
    exit 40
fi
if ! grep -q '"network_id": 99' "$workflow"; then
    echo "Plan 096: workflow must declare network id 99" >&2
    exit 41
fi
if ! grep -q "host-loopback-development" "$workflow"; then
    echo "Plan 096: workflow must declare the host-loopback-development topology" >&2
    exit 42
fi

echo "Plan 096/097 workflow audit: OK (workflow is dispatch-ready)"
