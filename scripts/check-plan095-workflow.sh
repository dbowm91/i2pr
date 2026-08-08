#!/usr/bin/env bash
# Plan 096 pre-dispatch workflow audit.
#
# This script is a narrowly-named static boundary check that
# verifies the four demonstrated Plan 095 workflow execution
# defects have been corrected before any manual dispatch. The
# script does not attempt to emulate the entire GitHub Actions
# expression engine; it is a repository-specific guard for the
# Plan 095 workflow contract.
#
# Exit code 0 means the workflow is dispatch-ready; nonzero
# means a Plan 096 corrective commit is required before any
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
if ! grep -q 'test -f output/i2pr-interop' "$workflow"; then
    echo "Plan 096: i2pr binary existence must be asserted before hashing" >&2
    exit 5
fi
if ! grep -q 'test -x output/i2pr-interop' "$workflow"; then
    echo "Plan 096: i2pr binary executability must be asserted before hashing" >&2
    exit 6
fi
if ! grep -q 'test ! -L output/i2pr-interop' "$workflow"; then
    echo "Plan 096: i2pr binary must be asserted as a non-symlink before hashing" >&2
    exit 7
fi

# WP3: sanitized evidence must be disjoint from disposable run
# roots.
if grep -q "target/interop/plan095-instrumented/sanitized" "$workflow"; then
    echo "Plan 096: instrumented sanitized path is nested in disposable run root" >&2
    exit 8
fi
if grep -q "target/interop/plan095-control/sanitized" "$workflow"; then
    echo "Plan 096: control sanitized path is nested in disposable run root" >&2
    exit 9
fi
if ! grep -q "target/interop/plan095-evidence/instrumented" "$workflow"; then
    echo "Plan 096: instrumented sanitized path must live under plan095-evidence/" >&2
    exit 10
fi
if ! grep -q "target/interop/plan095-evidence/control" "$workflow"; then
    echo "Plan 096: control sanitized path must live under plan095-evidence/" >&2
    exit 11
fi
# The delete-raw-run-state steps must verify the sanitized tree
# survives the destructive cleanup.
delete_count=$(grep -c "delete-raw-run-state" "$workflow" || true)
if [[ "$delete_count" -lt 2 ]]; then
    echo "Plan 096: expected instrumented + control delete-raw-run-state steps" >&2
    exit 12
fi
# Both delete steps must include the post-cleanup existence check.
for line in $(grep -n "delete-raw-run-state" "$workflow" | cut -d: -f1); do
    body=$(sed -n "${line},/^      - name:\|^  [a-z]/{/^      - name:\|^  [a-z]/!p;}" "$workflow" | head -n 20)
    if ! grep -q "sanitized evidence tree was lost" <<<"$body"; then
        echo "Plan 096: delete-raw-run-state step at line $line must verify sanitized tree survives" >&2
        exit 13
    fi
done

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
    sys.exit(14)
PY

# WP5: i2pd source digest must enumerate tracked files only.
if grep -q "find i2pd -type f" "$workflow"; then
    echo "Plan 096: i2pd digest must not use 'find i2pd -type f'" >&2
    exit 15
fi
if ! grep -q "git -C i2pd ls-files" "$workflow"; then
    echo "Plan 096: i2pd digest must use 'git ls-files'" >&2
    exit 16
fi
if ! grep -q "git -C i2pd rev-parse HEAD" "$workflow"; then
    echo "Plan 096: pinned i2pd revision must be verified before digest/build" >&2
    exit 17
fi
if ! grep -q "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e" "$workflow"; then
    echo "Plan 096: pinned i2pd revision marker is missing" >&2
    exit 18
fi

# WP6: dependency graph, fail-closed live attempts, and no retry.
if ! grep -q "needs: \[build, forward-instrumented, forward-control\]" "$workflow"; then
    echo "Plan 096: validate-gate must depend on build + both live jobs" >&2
    exit 19
fi
# Live attempt must propagate the exit explicitly, never via
# ``|| echo``.
if grep -nE "^\s*[^#]*\|\| echo" "$workflow" >/dev/null; then
    echo "Plan 096: live attempt must not use '|| echo' to mask failure" >&2
    exit 20
fi
if ! grep -q "instrumented_rc=\$?" "$workflow"; then
    echo "Plan 096: instrumented live attempt must capture its exit" >&2
    exit 21
fi
if ! grep -q "control_rc=\$?" "$workflow"; then
    echo "Plan 096: control live attempt must capture its exit" >&2
    exit 22
fi
if grep -q "nick-fields/retry" "$workflow"; then
    echo "Plan 096: workflow must not introduce a retry action" >&2
    exit 23
fi
if grep -q "i2pd-to-i2pr-ipv4" "$workflow"; then
    echo "Plan 096: forward-only workflow must not invoke the reverse direction" >&2
    exit 24
fi
if grep -q "plan084_runner" "$workflow"; then
    echo "Plan 096: forward-only workflow must not import the reverse runner" >&2
    exit 25
fi
if ! grep -q "workflow_dispatch" "$workflow"; then
    echo "Plan 096: workflow must keep workflow_dispatch as the only trigger" >&2
    exit 26
fi
if ! grep -q "ubuntu-24.04" "$workflow"; then
    echo "Plan 096: workflow must keep ubuntu-24.04 as the runner label" >&2
    exit 27
fi
if ! grep -q "experimental-non-advertised" "$workflow"; then
    echo "Plan 096: workflow must mark NTCP2 as experimental-non-advertised" >&2
    exit 28
fi
if ! grep -q '"network_id": 99' "$workflow"; then
    echo "Plan 096: workflow must declare network id 99" >&2
    exit 29
fi
if ! grep -q "host-loopback-development" "$workflow"; then
    echo "Plan 096: workflow must declare the host-loopback-development topology" >&2
    exit 30
fi

echo "Plan 096 workflow audit: OK (workflow is dispatch-ready)"
