#!/usr/bin/env bash
# Plan 295 §11 — static evidence-integrity check for the I2PControl
# differential lane (`tests/integration/i2pcontrol/run-differential.sh`).
#
# The lane's counted rows derive from the executed Rust corpus
# (`crates/i2pr-daemon/tests/i2pcontrol_differential.rs`), which asserts
# every disposition and prints exactly one `PLAN295-CORPUS` line with
# sanitized counts plus a response-shape hash (tokens/passwords
# redacted before hashing). The lane records that line into the
# evidence file; this checker rejects known dangerous bookkeeping:
# literal pass rows, `|| true` forgiveness, fake target env, and
# secret-carrying evidence.
#
# Guarded local rows (exact counts from the corpus assertions):
#   local-answered-28, local-gapped-8, local-errors-4.
# External rows stay `blocked-env-absent` until the provisioned lane
# sets both target env vars; a `recorded` external block must name
# its target and carry its own corpus line.
#
# Usage: bash scripts/check-i2pcontrol-acceptance-evidence.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LANE="${REPO_ROOT}/tests/integration/i2pcontrol/run-differential.sh"
EVIDENCE="${REPO_ROOT}/tests/integration/i2pcontrol/evidence/i2pcontrol-differential.json"
CORPUS="${REPO_ROOT}/crates/i2pr-daemon/tests/i2pcontrol_differential.rs"

fail() {
  echo "check-i2pcontrol-acceptance-evidence: $1" >&2
  exit 1
}

# Lane hygiene: no forgiveness, no fake env, no literal pass rows.
grep -q "|| true" "${LANE}" && fail "lane forgives failures with || true"
grep -q "continue-on-error" "${LANE}" && fail "lane tolerates errors"
grep -Eq '"(state|result)": *"passed"' "${LANE}" && fail "lane records a literal passed row"
grep -q "I2PR_I2PCONTROL_TARGET:-[^}]" "${LANE}" && fail "lane defaults the external target"
grep -q "I2PR_I2PCONTROL_PASSWORD:-[^}]" "${LANE}" && fail "lane defaults the external password"

# Corpus hygiene: external mode is ignore-gated and fails loudly on
# missing env; shape hashing redacts secrets first.
grep -q '#\[ignore' "${CORPUS}" || fail "external corpus is not ignore-gated"
grep -q "I2PR_I2PCONTROL_TARGET" "${CORPUS}" || fail "external corpus ignores the target env"
grep -q "I2PR_I2PCONTROL_PASSWORD" "${CORPUS}" || fail "external corpus ignores the password env"
grep -q "sanitized" "${CORPUS}" || fail "corpus hashes unsanitized responses"
grep -q "PLAN295-CORPUS" "${CORPUS}" || fail "corpus emits no evidence line"

# Evidence file hygiene.
[ -f "${EVIDENCE}" ] || fail "missing evidence file ${EVIDENCE}"
python3 - "${EVIDENCE}" <<'EOF'
import json, re, sys
evidence = json.load(open(sys.argv[1]))
assert evidence["plan"] == 295, "plan tag"
assert re.fullmatch(r"[0-9a-f]{40}", evidence["head"]), "head hash"
local = evidence["local"]
assert local["state"] == "passed", "local state"
assert local["answered"] == 28, "answered count"
assert local["gapped"] == 8, "gapped count"
assert local["errors"] == 4, "error count"
assert re.fullmatch(r"[0-9a-f]{16}", local["shape"]), "shape hash"
external = evidence["external"]
assert external["state"] in ("blocked-env-absent", "recorded"), "external state"
if external["state"] == "blocked-env-absent":
    assert external["target"] is None and external["corpus"] is None
else:
    assert external["target"] and "PLAN295-CORPUS" in external["corpus"]
EOF

# No secret-carrying values in the evidence (shape hashes only).
grep -qi '"password"' "${EVIDENCE}" && fail "evidence carries a password field"
grep -qi '"token": *"[A-Za-z0-9]\{40,\}"' "${EVIDENCE}" && fail "evidence carries a token value"

echo "check-i2pcontrol-acceptance-evidence: ok"
