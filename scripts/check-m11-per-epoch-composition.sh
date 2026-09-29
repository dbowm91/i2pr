#!/usr/bin/env bash
# Plan 264 work package A — per-epoch fresh-mesh composition gate.
#
# Closure composes ONLY from per-epoch manifests: every mandatory
# epoch needs two same-SHA passes (distinct epoch_pass), each pass
# on its own fresh mesh, with no cross-epoch or cross-pass merge.
# A single-mesh full-matrix run (manifest epoch "full-matrix") is
# diagnostic-only and can never satisfy counted closure.
#
# Usage:
#   bash scripts/check-m11-per-epoch-composition.sh
#     Static invariants only (routine CI): driver composition
#     helpers + unit rows, runner manifest shape, workflow
#     epoch/pass inputs, frozen lane numerics.
#   bash scripts/check-m11-per-epoch-composition.sh --compose DIR...
#     Additionally validate the composition over the given
#     per-epoch evidence roots (each holding evidence.json):
#     every mandatory epoch present twice on one SHA with
#     distinct passes, all rows passed, no full-matrix manifest
#     counted, no cross-epoch merge.
#
# Never merges evidence across runs: each manifest is read
# independently and only its own (epoch, epoch_pass, i2pr_commit,
# rows) tuple counts toward its epoch's gate.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DRIVER="${REPO_ROOT}/crates/i2pr-daemon/tests/m11_transit_i2pd_external.rs"
HARNESS="${REPO_ROOT}/tests/integration/m11-transit/run-i2pd.sh"
WORKFLOW="${REPO_ROOT}/.github/workflows/m11-transit-external.yml"

failures=0
fail() {
  echo "check-m11-per-epoch-composition: $*" >&2
  failures=$((failures + 1))
}

# ---- Static invariants: driver composition surface --------------------
for symbol in 'PLAN264_MANDATORY_EPOCHS' 'plan264_required_passes_per_epoch' 'plan264_epoch_is_mandatory' 'plan264_epoch_passes_satisfy' 'plan264_composition_covers_all_epochs' 'plan264_single_mesh_is_diagnostic_only'; do
  if ! grep -qF "${symbol}" "${DRIVER}"; then
    fail "driver missing Plan 264 composition symbol ${symbol}"
  fi
done
for test_row in 'plan264_single_pass_cannot_close_epoch' 'plan264_mixed_sha_epochs_rejected' 'plan264_cross_epoch_merge_rejected' 'plan264_missing_epoch_rejected' 'plan264_single_mesh_run_is_diagnostic_only'; do
  if ! grep -qF "${test_row}" "${DRIVER}"; then
    fail "Plan 264 composition regression missing: ${test_row}"
  fi
done
# The per-epoch selector must cover every mandatory epoch label.
for epoch in '"obep-data"' '"ibgw-data"' '"participant-data"' '"replay"' '"expiry"' '"cancel"' '"session-close"' '"restart"'; do
  if ! grep -qF "run_epoch(${epoch})" "${DRIVER}"; then
    fail "driver has no per-epoch gate for ${epoch}"
  fi
done
if ! grep -qF 'needs_participant_setup' "${DRIVER}"; then
  fail "driver must provision participant setup for per-epoch data/lifecycle runs"
fi
if ! grep -qF 'needs_forward_setup' "${DRIVER}"; then
  fail "driver must provision forward-cell setup for per-epoch lifecycle runs"
fi
if ! grep -qF 'run_lifecycle_chain' "${DRIVER}"; then
  fail "driver must gate the lifecycle chain per epoch"
fi
if ! grep -qF 'run_obep_data' "${DRIVER}"; then
  fail "driver must gate the obep-data epoch per epoch"
fi
if ! grep -qF 'run_ibgw_data_epoch' "${DRIVER}"; then
  fail "driver must gate the ibgw-data epoch per epoch"
fi
if ! grep -qF 'run_receipt_epoch' "${DRIVER}"; then
  fail "driver must gate the receipt epoch per epoch"
fi
if ! grep -qF 'run_participant_data_epoch' "${DRIVER}"; then
  fail "driver must gate the participant-data epoch per epoch"
fi

# ---- Static invariants: runner manifest shape -------------------------
for token in '"plan": 264' '"epoch": manifest_epoch' '"epoch_pass": epoch_pass' '"epoch_qualification"' '"epoch_terminal_key"' 'EPOCH_TERMINAL_KEY' 'I2PR_M11_ONLY_EPOCH' 'I2PR_M11_EPOCH_PASS' 'I2PR_M11_DRIVER_RC'; do
  if ! grep -qF "${token}" "${HARNESS}"; then
    fail "runner missing Plan 264 per-epoch manifest token ${token}"
  fi
done
if grep -qF '"plan": 263' "${HARNESS}"; then
  fail "runner must not name stale plan 263 (scoping authority is Plan 264)"
fi
if ! grep -qF 'full-matrix' "${HARNESS}"; then
  fail "runner must mark full-matrix runs diagnostic-only"
fi

# ---- Static invariants: workflow epoch/pass inputs ---------------------
for token in 'I2PR_M11_ONLY_EPOCH' 'I2PR_M11_EPOCH_PASS'; do
  if ! grep -qF "${token}" "${WORKFLOW}"; then
    fail "workflow missing Plan 264 per-epoch input ${token}"
  fi
done

# ---- Static invariants: no tuning --------------------------------------
for binding in 'DIAL_TIMEOUT: Duration = Duration::from_secs(20)' 'SAM_IO_TIMEOUT: Duration = Duration::from_secs(15)' 'SETUP_HEARTBEAT_SECS: u64 = 30' 'MAX_SAM_DATAGRAM_RX_BYTES: usize = 8192' 'MAX_LEDGER_OBSERVATIONS: usize = 4096'; do
  if ! grep -qF "${binding}" "${DRIVER}"; then
    fail "driver changed a frozen Plan 262 lane constant: ${binding}"
  fi
done

# ---- Composition over per-epoch manifests -------------------------------
if [[ "${1:-}" == "--compose" ]]; then
  shift
  if [[ "$#" -eq 0 ]]; then
    fail "--compose requires at least one evidence root"
  fi
  python3 - "$@" <<'PY'
import json
import sys
from pathlib import Path

MANDATORY = [
    "obep", "ibgw", "participant", "reject", "obep-data",
    "ibgw-data", "receipt", "participant-data", "replay",
    "expiry", "cancel", "session-close", "restart",
]
ALIAS = {"ibgw-receipt": "receipt"}

failures = []
by_epoch = {}
shas = set()
for root in sys.argv[1:]:
    manifest = Path(root) / "evidence.json"
    if not manifest.exists():
        failures.append(f"missing evidence.json in {root}")
        continue
    try:
        doc = json.loads(manifest.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError) as exc:
        failures.append(f"unreadable evidence.json in {root}: {exc}")
        continue
    if doc.get("plan") != 264:
        failures.append(f"{root}: manifest plan is not 264")
        continue
    epoch = ALIAS.get(doc.get("epoch", ""), doc.get("epoch", ""))
    if epoch == "full-matrix" or not epoch:
        failures.append(f"{root}: full-matrix runs are diagnostic-only and never counted")
        continue
    if epoch not in MANDATORY:
        failures.append(f"{root}: unknown counted epoch {epoch!r}")
        continue
    sha = doc.get("i2pr_commit", "")
    if not sha:
        failures.append(f"{root}: manifest carries no i2pr_commit")
        continue
    shas.add(sha)
    # Plan 264 per-epoch verdict: the manifest's epoch_qualification
    # is passed only when the driver exited 0 with the epoch's
    # terminal key present (fail-closed gates all green on that
    # fresh mesh). The whole-lane m11_transit_qualification stays
    # full-matrix and is never the per-epoch criterion.
    if doc.get("epoch_qualification") != "passed":
        failures.append(f"{root}: epoch {epoch} pass did not pass (epoch_qualification)")
        continue
    terminal = doc.get("epoch_terminal_key", "")
    if not terminal or terminal not in doc.get("driver_evidence_keys", []):
        failures.append(f"{root}: epoch {epoch} terminal key missing from driver evidence")
        continue
    by_epoch.setdefault(epoch, []).append(
        (str(doc.get("epoch_pass", "")), sha, root)
    )

if len(shas) > 1:
    failures.append(f"mixed implementation SHAs across manifests: {sorted(shas)}")
for epoch in MANDATORY:
    passes = by_epoch.get(epoch, [])
    if len(passes) < 2:
        failures.append(
            f"epoch {epoch}: requires two same-SHA passes, found {len(passes)}"
        )
        continue
    pass_ids = [p[0] for p in passes]
    if len(set(pass_ids)) < 2:
        failures.append(
            f"epoch {epoch}: passes must carry distinct epoch_pass ids, found {pass_ids}"
        )
    epoch_shas = {p[1] for p in passes}
    if len(epoch_shas) > 1:
        failures.append(f"epoch {epoch}: mixed SHAs {sorted(epoch_shas)}")

if failures:
    for line in failures:
        print(f"check-m11-per-epoch-composition: {line}", file=sys.stderr)
    sys.exit(1)
print(
    f"check-m11-per-epoch-composition: composed {len(MANDATORY)} epochs x 2 passes "
    f"on one SHA, no cross-epoch merge"
)
PY
  # shellcheck disable=SC2181
  if [[ "$?" -ne 0 ]]; then
    failures=$((failures + 1))
  fi
fi

if [[ "${failures}" -ne 0 ]]; then
  echo "check-m11-per-epoch-composition: ${failures} violation(s)" >&2
  exit 1
fi
echo "check-m11-per-epoch-composition: passed"
