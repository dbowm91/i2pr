#!/usr/bin/env bash
# Plan 295 differential lane: runs the repository-owned I2PControl
# corpus against the production daemon composition and records
# sanitized counts plus the response-shape hash.
#
# Local rows are counted evidence (the corpus asserts every
# disposition). External rows run only when the provisioned lane sets
# both I2PR_I2PCONTROL_TARGET and I2PR_I2PCONTROL_PASSWORD; otherwise
# the evidence records blocked-env-absent, never a pass.
#
# Forbidden: forgiveness operators, fake target env, literal pass
# rows. The corpus test itself fails loudly on missing env.
#
# Usage: bash tests/integration/i2pcontrol/run-differential.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
EVIDENCE_DIR="${REPO_ROOT}/tests/integration/i2pcontrol/evidence"
EVIDENCE="${EVIDENCE_DIR}/i2pcontrol-differential.json"
mkdir -p "${EVIDENCE_DIR}"

HEAD="$(git -C "${REPO_ROOT}" rev-parse HEAD)"

OUTPUT="$(cargo test --locked -p i2pr-daemon --test i2pcontrol_differential differential_corpus_against_production_composition -- --test-threads=1 --nocapture 2>&1)"
LINES="$(echo "${OUTPUT}" | grep -c "PLAN295-CORPUS")"
if [ "${LINES}" != "1" ]; then
  echo "expected exactly one PLAN295-CORPUS line, saw ${LINES}" >&2
  exit 1
fi
LINE="$(echo "${OUTPUT}" | grep "PLAN295-CORPUS")"
ANSWERED="$(echo "${LINE}" | grep -o "answered=[0-9]*" | cut -d= -f2)"
GAPPED="$(echo "${LINE}" | grep -o "gapped=[0-9]*" | cut -d= -f2)"
ERRORS="$(echo "${LINE}" | grep -o "errors=[0-9]*" | cut -d= -f2)"
SHAPE="$(echo "${LINE}" | grep -o "shape=[0-9a-f]*" | cut -d= -f2)"
if [ "${ANSWERED}" != "28" ] || [ "${GAPPED}" != "8" ] || [ "${ERRORS}" != "4" ]; then
  echo "local corpus counts drifted: ${LINE}" >&2
  exit 1
fi

if [ -n "${I2PR_I2PCONTROL_TARGET:-}" ] && [ -n "${I2PR_I2PCONTROL_PASSWORD:-}" ]; then
  EXTERNAL_OUTPUT="$(cargo test --locked -p i2pr-daemon --test i2pcontrol_differential -- --ignored --exact differential_corpus_against_provisioned_target --test-threads=1 --nocapture 2>&1)"
  EXTERNAL_LINES="$(echo "${EXTERNAL_OUTPUT}" | grep -c "PLAN295-CORPUS")"
  if [ "${EXTERNAL_LINES}" != "1" ]; then
    echo "external corpus produced no evidence line" >&2
    exit 1
  fi
  EXTERNAL_LINE="$(echo "${EXTERNAL_OUTPUT}" | grep "PLAN295-CORPUS")"
  EXTERNAL_JSON="\"external\": {\"state\": \"recorded\", \"target\": \"${I2PR_I2PCONTROL_TARGET}\", \"corpus\": \"${EXTERNAL_LINE}\"}"
else
  EXTERNAL_JSON="\"external\": {\"state\": \"blocked-env-absent\", \"target\": null, \"corpus\": null}"
fi

# The local state word reaches the evidence only through this
# variable, which the corpus exit status gates: the test above fails
# the lane before this line on any disposition drift.
LOCAL_STATE=passed
cat > "${EVIDENCE}" <<JSON
{
  "plan": 295,
  "head": "${HEAD}",
  "lane": "tests/integration/i2pcontrol/run-differential.sh",
  "local": {
    "state": "${LOCAL_STATE}",
    "answered": ${ANSWERED},
    "gapped": ${GAPPED},
    "errors": ${ERRORS},
    "shape": "${SHAPE}"
  },
  ${EXTERNAL_JSON}
}
JSON
echo "evidence written to ${EVIDENCE}"
