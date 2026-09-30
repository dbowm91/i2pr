#!/usr/bin/env bash
# Plan 264 work package A — per-epoch fresh-mesh composition gate,
# extended by Plan 265 into the fixed-budget opportunity-qualified
# closure composer.
#
# Plan 264 (retained): closure composes ONLY from per-epoch manifests:
# every mandatory epoch needs two same-SHA passes (distinct
# epoch_pass), each pass on its own fresh mesh, with no cross-epoch or
# cross-pass merge. A single-mesh full-matrix run (manifest epoch
# "full-matrix") is diagnostic-only and can never satisfy counted
# closure.
#
# Plan 265 (current authority): closure composes from the three frozen
# scenario families, each with EXACTLY eight retained fresh-mesh
# attempts on one qualification SHA, plus the integrity-checked
# retained Plan 264 five-epoch evidence. The input-side opportunity
# predicate is what separates "the reference never supplied the input"
# from "i2pr mishandled the input it was given"; every dispatched
# attempt stays in the denominator and any semantic contradiction
# retains the plan.
#
# Usage:
#   bash scripts/check-m11-per-epoch-composition.sh
#     Static invariants only (routine CI): driver composition helpers
#     + unit rows, runner manifest shape, workflow epoch/pass/scenario
#     inputs, frozen lane numerics, the Plan 265 vocabulary and the
#     retained Plan 264 evidence index.
#   bash scripts/check-m11-per-epoch-composition.sh --compose DIR...
#     Additionally validate the Plan 264 per-epoch composition over
#     the given per-epoch evidence roots.
#   bash scripts/check-m11-per-epoch-composition.sh --compose-265 \
#       --retained DIR --retained DIR ... -- DIR DIR ...
#     The Plan 265 closure composition. Every DIR after the bare `--`
#     is one Plan 265 attempt evidence root; every `--retained DIR` is
#     one retained Plan 264 manifest root. Both sets are mandatory.
#   bash scripts/check-m11-per-epoch-composition.sh --self-test
#     Run the negative/positive composition fixtures against the real
#     composer. Exits non-zero if any fixture's expected verdict is
#     not observed.
#
# Never merges evidence across runs: each manifest is read
# independently and only its own declared facts count.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DRIVER="${REPO_ROOT}/crates/i2pr-daemon/tests/m11_transit_i2pd_external.rs"
HARNESS="${REPO_ROOT}/tests/integration/m11-transit/run-i2pd.sh"
WORKFLOW="${REPO_ROOT}/.github/workflows/m11-transit-external.yml"
RETAINED_INDEX="${REPO_ROOT}/plans/closure/transit-tunnels/265-retained-plan264-evidence.tsv"

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
for token in '"plan": 265' '"epoch": manifest_epoch' '"epoch_pass": epoch_pass' '"epoch_qualification"' '"epoch_terminal_key"' 'EPOCH_TERMINAL_KEY' 'I2PR_M11_ONLY_EPOCH' 'I2PR_M11_EPOCH_PASS' 'I2PR_M11_DRIVER_RC' 'I2PR_M11_SCENARIO' 'i2pr-m11-transit-qualification-v5' '"attempt_budget": ATTEMPT_BUDGET' '"qualification_sha": commit' '"production_baseline": PRODUCTION_BASELINE' '"opportunity": opportunity' '"opportunity_reason": opportunity_reason' '"semantic": semantic' '"external_completion": external_completion' '"terminal_class": terminal_class' 'PLAN265_PRODUCTION_BASELINE' 'PLAN265_ATTEMPT_BUDGET'; do
  if ! grep -qF "${token}" "${HARNESS}"; then
    fail "runner missing Plan 265 manifest token ${token}"
  fi
done
if grep -qF '"plan": 264' "${HARNESS}"; then
  fail "runner must not name stale plan 264 (closure authority is Plan 265)"
fi
if grep -qF '"plan": 263' "${HARNESS}"; then
  fail "runner must not name stale plan 263 (closure authority is Plan 265)"
fi
if ! grep -qF 'full-matrix' "${HARNESS}"; then
  fail "runner must mark full-matrix runs diagnostic-only"
fi

# ---- Static invariants: workflow inputs -------------------------------
for token in 'I2PR_M11_ONLY_EPOCH' 'I2PR_M11_EPOCH_PASS' 'I2PR_M11_SCENARIO' 'I2PR_M11_ATTEMPT'; do
  if ! grep -qF "${token}" "${WORKFLOW}"; then
    fail "workflow missing Plan 265 input ${token}"
  fi
done
if ! grep -qF 'attempt: [1, 2, 3, 4, 5, 6, 7, 8]' "${WORKFLOW}"; then
  fail "workflow must dispatch the frozen eight-attempt budget (no attempt nine)"
fi

# ---- Static invariants: no tuning --------------------------------------
for binding in 'DIAL_TIMEOUT: Duration = Duration::from_secs(20)' 'SAM_IO_TIMEOUT: Duration = Duration::from_secs(15)' 'SETUP_HEARTBEAT_SECS: u64 = 30' 'MAX_SAM_DATAGRAM_RX_BYTES: usize = 8192' 'MAX_LEDGER_OBSERVATIONS: usize = 4096'; do
  if ! grep -qF "${binding}" "${DRIVER}"; then
    fail "driver changed a frozen Plan 262 lane constant: ${binding}"
  fi
done

# ---- Plan 265 static invariants: vocabulary + predicates ---------------
for symbol in 'PLAN265_SCENARIO_FAMILIES' 'PLAN265_ATTEMPT_BUDGET' 'PLAN265_PRODUCTION_BASELINE' 'PLAN265_TERMINAL_VOCABULARY' 'PLAN265_OPPORTUNITY_VALUES' 'PLAN265_SEMANTIC_VALUES' 'PLAN265_EXTERNAL_VALUES' 'PLAN265_LIFECYCLE_ROWS' 'PLAN265_RETAINED_PLAN264_EPOCHS' 'PLAN265_SETUP_STOPS'; do
  if ! grep -qF "${symbol}" "${DRIVER}"; then
    fail "driver missing Plan 265 symbol ${symbol}"
  fi
done
for symbol in 'plan265_ibgw_opportunity' 'plan265_ibgw_semantic_pass' 'plan265_classify_ibgw' 'plan265_receipt_opportunity' 'plan265_receipt_semantic_pass' 'plan265_classify_receipt' 'plan265_participant_opportunity' 'plan265_classify_participant' 'plan265_gateway_seam_observed' 'plan265_lifecycle_chain_complete' 'plan265_family_c_failure' 'ReplayOutcomeKind'; do
  if ! grep -qF "${symbol}" "${DRIVER}"; then
    fail "driver missing Plan 265 predicate ${symbol}"
  fi
done
for test_row in 'plan265_ibgw_opportunity_is_input_side_only' 'plan265_ibgw_large_input_multicell_passes' 'plan265_ibgw_large_input_single_cell_is_semantic_failure' 'plan265_ibgw_opportunity_absent_is_typed' 'plan265_receipt_self_action_opportunity_classification' 'plan265_receipt_semantic_and_socket_completion_pass' 'plan265_receipt_reference_completion_miss_is_not_a_semantic_failure' 'plan265_receipt_opportunity_present_with_failed_local_ingress_fails' 'plan265_participant_input_side_opportunity_classification' 'plan265_participant_input_present_without_local_forward_fails' 'plan265_participant_local_forward_with_b_completion_miss' 'plan265_participant_lifecycle_chain_passes' 'plan265_replay_duplicate_dropped_passes' 'plan265_replay_contained_no_output_requires_zero_forward_and_zero_b_delta' 'plan265_replay_duplicate_forwarded_fails_regardless_of_b_receipt' 'plan265_manifest_v5_requires_exactly_eight_ordinals' 'plan265_composition_rejects_budget_sha_duplicate_missing_and_unclassified' 'plan265_lifecycle_rows_cannot_be_borrowed_across_attempts' 'plan265_production_source_diff_guard'; do
  if ! grep -qF "${test_row}" "${DRIVER}"; then
    fail "Plan 265 focused test missing: ${test_row}"
  fi
done
if ! grep -qF 'const PLAN265_ATTEMPT_BUDGET: usize = 8;' "${DRIVER}"; then
  fail "driver must freeze the per-family attempt budget at 8"
fi
# Plan 265 section 3.3: the input-side opportunity boundary must never
# read the downstream success result. This is a source-level property,
# so it is enforced statically here: an opportunity predicate that
# mentions any non-input-side field is a violation regardless of what
# the unit tests assert. The matcher is brace-balanced and keeps every
# statement on one physical line, so no string literal can straddle a
# line break.
check_input_side() {
python3 - "${1:-$DRIVER}" <<'PYOPPORTUNITY'
import sys
from pathlib import Path

source = Path(sys.argv[1]).read_text(encoding="utf-8")
FORBIDDEN = {
    "plan265_ibgw_opportunity": (
        "emitted_cells",
        "aux_count",
        "failures",
        "gateway_failures",
        "next_router",
        "next_tunnel",
    ),
    "plan265_receipt_opportunity": (
        "emitted_cells",
        "failures",
        "next_router",
        "next_tunnel",
        "tuple_bound",
        "socket_receipts",
    ),
    "plan265_participant_opportunity": (
        "local_forward",
        "b_endpoint_observations",
        "lifecycle_rows",
        "replay",
    ),
}
OPENERS = "{(["


def body_of(name):
    start = source.find("fn " + name + "(&self")
    if start < 0:
        return None
    index = source.index("{", start)
    depth = 0
    for cursor in range(index, len(source)):
        char = source[cursor]
        if char in OPENERS:
            depth += 1
        elif char in "})]":
            depth -= 1
            if depth == 0:
                return source[index : cursor + 1]
    return None


bad = []
for name, tokens in FORBIDDEN.items():
    body = body_of(name)
    if body is None:
        bad.append(f"{name}: predicate not found")
        continue
    for token in tokens:
        if token in body:
            bad.append(f"{name}: reads non-input-side field {token}")
if bad:
    print("; ".join(bad), file=sys.stderr)
    raise SystemExit(1)
PYOPPORTUNITY
}

# The static lane (no arguments) runs the guard inline; `--check-input-side`
# exposes it to the external runner so there is exactly one implementation.
if [[ "${1:-}" == "--check-input-side" ]]; then
  if ! check_input_side "${2:-$DRIVER}"; then
    echo "check-m11-per-epoch-composition: Plan 265 input-side opportunity predicate reads a non-input-side field" >&2
    exit 1
  fi
  echo "check-m11-per-epoch-composition: input-side opportunity predicates are input-side only"
  exit 0
fi

if ! check_input_side "$DRIVER"; then
  fail "Plan 265 input-side opportunity predicate reads a non-input-side field"
fi
# ---- Retained Plan 264 evidence index ---------------------------------
retained_rows=()
if [[ -f "${RETAINED_INDEX}" ]]; then
  while IFS=$'\t' read -r epoch pass sha qual key digest; do
    case "${epoch}" in ''|'#'*) continue ;; esac
    retained_rows+=("${epoch}"$'\t'"${pass}"$'\t'"${sha}"$'\t'"${qual}"$'\t'"${key}"$'\t'"${digest}")
  done < "${RETAINED_INDEX}"
else
  fail "retained Plan 264 evidence index missing: ${RETAINED_INDEX}"
fi
if [[ "${#retained_rows[@]}" -ne 10 ]]; then
  fail "retained Plan 264 evidence index must carry ten manifests (five epochs x two passes), found ${#retained_rows[@]}"
fi

# ---- Plan 264 per-epoch composition (retained mode) --------------------
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

# ---- Plan 265 fixed-budget closure composition -------------------------
# The composer itself lives in this file so the negative fixtures below
# exercise the exact code that decides closure.
compose_265() {
  # $1 is the retained-evidence index; the rest is --retained/--attempt
  # roots. Keeping the index explicit lets the self-test fixtures drive
  # the real composer against synthetic retained evidence.
  local index="$1"
  shift
  python3 - "${index}" "$@" <<'PY'
import hashlib
import json
import sys
from pathlib import Path

index_path = Path(sys.argv[1])
retained_roots = []
attempt_roots = []
mode = "retained"
for arg in sys.argv[2:]:
    if arg == "--retained":
        mode = "retained"
        continue
    if arg in ("--attempt", "--"):
        mode = "attempt"
        continue
    (retained_roots if mode == "retained" else attempt_roots).append(arg)

FAMILIES = ("ibgw-data", "receipt", "participant-lifecycle")
BUDGET = 8
PRODUCTION_BASELINE = "514bf1237e86fde21e17fc98c743eb52852edd99"
RETAINED_EPOCHS = ("obep", "ibgw", "participant", "reject", "obep-data")
TERMINAL_VOCABULARY = {
    "ibgw-large-input-not-observed",
    "receipt-no-b-originated-self-action",
    "receipt-no-live-counted-ibgw-target",
    "participant-input-not-observed",
    "ibgw-large-input-multicell-verified",
    "receipt-tuple-bound-socket-verified",
    "participant-forward-far-side-chain-verified",
    "receipt-reference-completion-miss",
    "participant-reference-completion-miss",
    "ibgw-large-input-emission-semantic-failure",
    "receipt-self-action-semantic-failure",
    "participant-forward-semantic-failure",
    "participant-lifecycle-row-semantic-failure",
    "ibgw-setup-stop",
    "receipt-setup-stop",
    "participant-setup-stop",
}
NO_OPPORTUNITY_TERMINALS = {
    "ibgw-large-input-not-observed",
    "receipt-no-b-originated-self-action",
    "receipt-no-live-counted-ibgw-target",
    "participant-input-not-observed",
}
SEMANTIC_FAILURE_TERMINALS = {
    "ibgw-large-input-emission-semantic-failure",
    "receipt-self-action-semantic-failure",
    "participant-forward-semantic-failure",
    "participant-lifecycle-row-semantic-failure",
}
COMPLETION_MISS_TERMINALS = {
    "receipt-reference-completion-miss",
    "participant-reference-completion-miss",
}
SETUP_STOP_TERMINALS = {
    "ibgw-setup-stop",
    "receipt-setup-stop",
    "participant-setup-stop",
}
FAMILY_SUCCESS_TERMINALS = {
    "ibgw-data": "ibgw-large-input-multicell-verified",
    "receipt": "receipt-tuple-bound-socket-verified",
    "participant-lifecycle": "participant-forward-far-side-chain-verified",
}
LIFECYCLE_ROWS = ("replay", "expiry", "session-close", "cancel", "restart")

failures = []


def read_manifest(root):
    path = Path(root) / "evidence.json"
    if not path.exists():
        failures.append(f"missing evidence.json in {root}")
        return None, None
    raw = path.read_bytes()
    try:
        return json.loads(raw.decode("utf-8")), hashlib.sha256(raw).hexdigest()
    except (json.JSONDecodeError, UnicodeDecodeError, OSError) as exc:
        failures.append(f"unreadable evidence.json in {root}: {exc}")
        return None, None


# ---- 1. retained Plan 264 five-epoch 2/2 evidence ----
index_rows = {}
for line in index_path.read_text(encoding="utf-8").splitlines():
    if not line.strip() or line.lstrip().startswith("#"):
        continue
    parts = line.split("\t")
    if len(parts) != 6:
        failures.append(f"retained index row is malformed: {line!r}")
        continue
    epoch, epoch_pass, sha, qualification, terminal, digest = parts
    index_rows[(epoch, epoch_pass)] = (sha, qualification, terminal, digest)

if not failures:
    for epoch in RETAINED_EPOCHS:
        passes = sorted(p for (e, p) in index_rows if e == epoch)
        if passes != ["1", "2"]:
            failures.append(
                f"retained Plan 264 epoch {epoch} must carry passes 1 and 2, found {passes}"
            )
    extra = {e for (e, _p) in index_rows} - set(RETAINED_EPOCHS)
    if extra:
        failures.append(f"retained index carries non-retained epochs: {sorted(extra)}")
    retained_shas = {v[0] for v in index_rows.values()}
    if len(retained_shas) != 1:
        failures.append(f"retained Plan 264 evidence spans multiple SHAs: {sorted(retained_shas)}")
    for (epoch, epoch_pass), (sha, qualification, terminal, digest) in sorted(index_rows.items()):
        if qualification != "passed":
            failures.append(
                f"retained Plan 264 epoch {epoch} pass {epoch_pass} is not passed"
            )
        if not terminal or not digest:
            failures.append(
                f"retained Plan 264 epoch {epoch} pass {epoch_pass} lacks its terminal key or digest"
            )

supplied = []
for root in retained_roots:
    doc, digest = read_manifest(root)
    if doc is None:
        continue
    supplied.append((root, doc, digest))
if not failures or supplied:
    if len(supplied) != len(index_rows):
        failures.append(
            f"Plan 265 composition requires the {len(index_rows)} retained Plan 264 manifests, "
            f"found {len(supplied)}"
        )
    for root, doc, digest in supplied:
        if doc.get("plan") != 264:
            failures.append(f"{root}: retained manifest plan is not 264")
            continue
        if doc.get("schema") != "i2pr-m11-transit-qualification-v4":
            failures.append(f"{root}: retained manifest schema is not the Plan 264 v4 shape")
            continue
        key = (doc.get("epoch", ""), str(doc.get("epoch_pass", "")))
        expected = index_rows.get(key)
        if expected is None:
            failures.append(f"{root}: retained manifest {key} is not in the integrity index")
            continue
        if digest != expected[3]:
            failures.append(f"{root}: retained manifest digest does not match the index")
            continue
        if doc.get("i2pr_commit") != expected[0]:
            failures.append(f"{root}: retained manifest SHA does not match the index")
            continue
        if doc.get("epoch_qualification") != "passed":
            failures.append(f"{root}: retained manifest is not a passed per-epoch pass")
            continue
        if doc.get("epoch_terminal_key") != expected[2]:
            failures.append(f"{root}: retained manifest terminal key does not match the index")
            continue
        keys = doc.get("driver_evidence_keys", [])
        if expected[2] not in keys:
            failures.append(f"{root}: retained terminal key is absent from its own driver evidence")

# ---- 2-5. Plan 265 attempts ----
attempts = []
shas = set()
budgets = set()
for root in attempt_roots:
    doc, _digest = read_manifest(root)
    if doc is None:
        continue
    if doc.get("plan") != 265:
        failures.append(
            f"{root}: manifest plan is {doc.get('plan')!r}; Plan 264/263 evidence is never "
            f"promoted into Plan 265 family counts"
        )
        continue
    if doc.get("schema") != "i2pr-m11-transit-qualification-v5":
        failures.append(f"{root}: manifest schema is not the frozen v5 shape")
        continue
    if not doc.get("qualification_tree_clean", False):
        failures.append(f"{root}: qualification tree was not clean")
        continue
    if doc.get("production_baseline") != PRODUCTION_BASELINE:
        failures.append(f"{root}: production_baseline is not the Plan 262 baseline")
        continue
    if doc.get("production_source_diff") != []:
        failures.append(f"{root}: production source diff is not empty")
        continue
    scenario = doc.get("scenario", "")
    if scenario not in FAMILIES:
        failures.append(f"{root}: scenario {scenario!r} is not a frozen family")
        continue
    try:
        ordinal = int(doc.get("attempt"))
    except (TypeError, ValueError):
        failures.append(f"{root}: attempt ordinal is not an integer")
        continue
    if not 1 <= ordinal <= BUDGET:
        failures.append(f"{root}: attempt ordinal {ordinal} is outside 1..{BUDGET}")
        continue
    if doc.get("attempt_budget") != BUDGET:
        failures.append(f"{root}: attempt_budget is not the frozen {BUDGET}")
        continue
    budgets.add(doc.get("attempt_budget"))
    terminal = doc.get("terminal_class", "")
    if terminal not in TERMINAL_VOCABULARY:
        failures.append(f"{root}: terminal_class {terminal!r} is outside the closed vocabulary")
        continue
    opportunity = doc.get("opportunity")
    semantic = doc.get("semantic")
    external = doc.get("external_completion")
    if opportunity == "absent":
        if semantic != "not-applicable" or external != "not-applicable":
            failures.append(f"{root}: opportunity absent with a semantic/external verdict")
            continue
        if terminal not in NO_OPPORTUNITY_TERMINALS:
            failures.append(
                f"{root}: opportunity absent with undeclared terminal {terminal!r}"
            )
            continue
    elif opportunity == "present":
        if semantic == "not-applicable":
            failures.append(f"{root}: opportunity present with semantic not-applicable")
            continue
        if semantic == "fail":
            failures.append(f"{root}: semantic failure in an opportunity-present attempt")
            continue
        if terminal in SEMANTIC_FAILURE_TERMINALS:
            failures.append(f"{root}: semantic-failure terminal with semantic {semantic!r}")
            continue
        if semantic == "pass" and external == "miss" and terminal not in COMPLETION_MISS_TERMINALS:
            failures.append(f"{root}: completion miss with undeclared terminal {terminal!r}")
            continue
        if semantic == "pass" and external == "pass" and terminal != FAMILY_SUCCESS_TERMINALS[scenario]:
            failures.append(f"{root}: external completion pass with terminal {terminal!r}")
            continue
        if external == "not-applicable" and scenario != "ibgw-data":
            failures.append(f"{root}: {scenario} declares an external-completion gate")
            continue
    elif opportunity == "not-observed":
        if semantic != "not-applicable" or external != "not-applicable":
            failures.append(f"{root}: setup stop with a semantic/external verdict")
            continue
        if terminal not in SETUP_STOP_TERMINALS:
            failures.append(f"{root}: setup stop with undeclared terminal {terminal!r}")
            continue
    else:
        failures.append(f"{root}: opportunity {opportunity!r} is outside the closed vocabulary")
        continue
    if scenario == "participant-lifecycle" and opportunity == "present" and semantic == "pass":
        rows = doc.get("lifecycle_rows", [])
        if sorted(rows) != sorted(LIFECYCLE_ROWS):
            failures.append(
                f"{root}: participant-lifecycle success must carry all five lifecycle rows on "
                f"this one attempt, found {rows}"
            )
            continue
        declared = doc.get("lifecycle_rows_declared", "")
        if declared and sorted(declared.split(",")) != sorted(LIFECYCLE_ROWS):
            failures.append(
                f"{root}: the driver's own lifecycle row set disagrees with the manifest: "
                f"{declared!r}"
            )
            continue
    if scenario == "participant-lifecycle" and terminal == "participant-lifecycle-row-semantic-failure":
        failures.append(f"{root}: replay duplicate-forwarded is a hard semantic failure")
        continue
    sha = doc.get("qualification_sha", "")
    if not sha:
        failures.append(f"{root}: manifest carries no qualification_sha")
        continue
    shas.add(sha)
    attempts.append((root, scenario, ordinal, terminal, opportunity, semantic, external))

if len(budgets) > 1:
    failures.append(f"attempt budget changed mid-family: {sorted(budgets)}")
if len(shas) > 1:
    failures.append(f"mixed qualification SHAs across Plan 265 attempts: {sorted(shas)}")

# ---- 4. attempt ordinals are exactly 1..8 for each family ----
for scenario in FAMILIES:
    ordinals = sorted(o for (_r, s, o, *_rest) in attempts if s == scenario)
    if ordinals != list(range(1, BUDGET + 1)):
        failures.append(
            f"family {scenario}: requires exactly ordinals 1..{BUDGET}, found {ordinals}"
        )

# ---- 5-9. family gates ----
summary = []
for scenario in FAMILIES:
    family = [(root, terminal) for (root, s, _o, terminal, *_r) in attempts if s == scenario]
    if not family:
        continue
    successes = sum(1 for (_r, t) in family if t == FAMILY_SUCCESS_TERMINALS[scenario])
    if successes < 2:
        failures.append(
            f"family {scenario}: requires at least two retained successes, found {successes}"
        )
    for root, terminal in family:
        if terminal in SEMANTIC_FAILURE_TERMINALS:
            failures.append(f"family {scenario}: semantic failure at {root}")
    summary.append((scenario, len(family), successes))

if failures:
    for line in failures:
        print(f"check-m11-per-epoch-composition: {line}", file=sys.stderr)
    sys.exit(1)
for scenario, total, successes in summary:
    print(
        f"check-m11-per-epoch-composition: family {scenario}: {total} retained attempts, "
        f"{successes} qualified successes"
    )
print(
    "check-m11-per-epoch-composition: composed 3 families x 8 retained attempts on one "
    f"qualification SHA with integrity-checked retained Plan 264 evidence"
)
PY
}

if [[ "${1:-}" == "--compose-265" ]]; then
  shift
  if [[ "$#" -eq 0 ]]; then
    fail "--compose-265 requires --retained and attempt evidence roots"
  fi
  compose_265 "${RETAINED_INDEX}" "$@" || failures=$((failures + 1))
fi
# ---- Negative/positive composition fixtures ----------------------------
# The fixtures below exercise the real composer, so the gate's accept and
# reject behavior is executable evidence rather than a claim. Each
# fixture isolates exactly one rule; a fixture that passed for the wrong
# reason would be worthless, so every negative family is a complete,
# otherwise-valid eight-attempt set with exactly one rule broken.
if [[ "${1:-}" == "--self-test" ]]; then
  fixture_root="$(mktemp -d -t i2pr-m11-plan265-selftest.XXXXXX)"
  trap 'rm -rf "${fixture_root}"' EXIT
  fixture_rc=0

  attempt_manifest() {
    # $1 = root, $2 = scenario, $3 = ordinal, $4 = opportunity,
    # $5 = semantic, $6 = external, $7 = terminal, $8 = sha, $9 = budget,
    # $10 = lifecycle rows (comma list or "-")
    local root="$1" scenario="$2" ordinal="$3" opportunity="$4" semantic="$5" \
      external="$6" terminal="$7" sha="$8" budget="$9" rows="${10}"
    local rows_json='"-"'
    if [[ "${rows}" != "-" ]]; then
      rows_json="$(printf '%s' "${rows}" | python3 -c \
        "import json,sys;print(json.dumps(sys.stdin.read().strip().split(',')))")"
    fi
    mkdir -p "${root}"
    printf '%s\n' "$(cat <<JSON
{"schema":"i2pr-m11-transit-qualification-v5","plan":265,"scenario":"${scenario}","attempt":${ordinal},"attempt_budget":${budget},"qualification_sha":"${sha}","qualification_tree_clean":true,"production_baseline":"514bf1237e86fde21e17fc98c743eb52852edd99","production_source_diff":[],"opportunity":"${opportunity}","opportunity_reason":"fixture","semantic":"${semantic}","external_completion":"${external}","terminal_class":"${terminal}","lifecycle_rows":${rows_json}}
JSON
)" > "${root}/evidence.json"
  }

  # Emits a complete eight-attempt family: $1 = family label,
  # $2 = number of qualified successes, $3 = ordinal that carries
  # `override` (0 = none), $4.. = override
  # "scenario|ordinal|opportunity|semantic|external|terminal|sha|budget|rows"
  # for that ordinal, $5 = retained miss kind, $6 = attempts emitted
  # (default 8; fewer models a family whose later attempts were never
  # dispatched, which must never compose).
  emit_family() {
    local label="$1" successes="$2" override_at="$3" override="$4" miss="${5:-absent}"
    local emitted="${6:-8}"
    local dir="${fixture_root}/fam-${label}"
    local ordinal terminal opportunity semantic external rows
    for ordinal in $(seq 1 "${emitted}"); do
      if [[ "${ordinal}" -le "${successes}" ]]; then
        case "${label}" in
          ibgw-data)
            opportunity=present; semantic=pass; external=not-applicable
            terminal=ibgw-large-input-multicell-verified; rows="-"
            ;;
          receipt)
            opportunity=present; semantic=pass; external=pass
            terminal=receipt-tuple-bound-socket-verified; rows="-"
            ;;
          *)
            opportunity=present; semantic=pass; external=pass
            terminal=participant-forward-far-side-chain-verified
            rows=replay,expiry,session-close,cancel,restart
            ;;
        esac
      elif [[ "${miss}" == "completion-miss" ]]; then
        opportunity=present; semantic=pass; external=miss
        terminal=receipt-reference-completion-miss; rows="-"
      else
        opportunity=absent; semantic=not-applicable; external=not-applicable
        rows="-"
        case "${label}" in
          ibgw-data) terminal=ibgw-large-input-not-observed ;;
          receipt) terminal=receipt-no-b-originated-self-action ;;
          *) terminal=participant-input-not-observed ;;
        esac
      fi
      if [[ "${ordinal}" -eq "${override_at}" ]]; then
        IFS='|' read -r _f _o opportunity semantic external terminal sha budget rows \
          <<< "${override}"
      else
        sha=fixedsha0000000000000000000000000000000000
        budget=8
      fi
      attempt_manifest "${dir}/${ordinal}" "${label}" "${ordinal}" "${opportunity}" \
        "${semantic}" "${external}" "${terminal}" "${sha}" "${budget}" "${rows}"
    done
    for ordinal in $(seq 1 "${emitted}"); do
      printf '%s\n' "${dir}/${ordinal}"
    done
  }

  expect_compose() {
    # $1 = expected outcome (0 accept / 1 reject), $2 = index, $3 = label,
    # $4.. roots
    local want="$1" index="$2" label="$3"
    shift 3
    local -a retained=()
    local dir
    for dir in "${fixture_root}"/retained/*/; do retained+=("${dir%/}"); done
    local got=0
    compose_265 "${index}" --retained "${retained[@]}" --attempt "$@" >/dev/null 2>&1 || got=1
    if [[ "${got}" -ne "${want}" ]]; then
      echo "check-m11-per-epoch-composition: self-test fixture ${label} expected reject=${want} got ${got}" >&2
      fixture_rc=1
    fi
  }

  # Retained Plan 264 evidence: the synthetic fixtures cannot reproduce
  # the recorded manifests' bytes, so the integrity index is regenerated
  # from the synthetic set. Epoch and pass are read from each manifest
  # itself, never parsed out of a directory name.
  mkdir -p "${fixture_root}/retained"
  : > "${fixture_root}/index.tsv"
  while IFS=$'\t' read -r epoch pass sha qual key digest; do
    case "${epoch}" in ''|'#'*) continue ;; esac
    dir="${fixture_root}/retained/${epoch}-p${pass}"
    mkdir -p "${dir}"
    printf '%s' "$(cat <<JSONRET
{"schema":"i2pr-m11-transit-qualification-v4","plan":264,"epoch":"${epoch}","epoch_pass":"${pass}","i2pr_commit":"${sha}","epoch_qualification":"${qual}","epoch_terminal_key":"${key}","driver_evidence_keys":["${key}"]}
JSONRET
)" > "${dir}/evidence.json"
    python3 - "${dir}/evidence.json" >> "${fixture_root}/index.tsv" <<'PYINDEX'
import hashlib
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
doc = json.loads(path.read_text(encoding="utf-8"))
print(
    "\t".join(
        [
            doc["epoch"],
            str(doc["epoch_pass"]),
            doc["i2pr_commit"],
            doc["epoch_qualification"],
            doc["epoch_terminal_key"],
            hashlib.sha256(path.read_bytes()).hexdigest(),
        ]
    )
)
PYINDEX
  done < "${RETAINED_INDEX}"

  SHA=fixedsha0000000000000000000000000000000000
  OTHER=othersha0000000000000000000000000000000000

  # -- positives -------------------------------------------------------
  mapfile -t A < <(emit_family ibgw-data 2 0 - absent)
  mapfile -t B < <(emit_family receipt 2 0 - absent)
  mapfile -t C < <(emit_family participant-lifecycle 2 0 - absent)
  expect_compose 0 "${fixture_root}/index.tsv" \
    two-successes-plus-six-retained-misses-accepted "${A[@]}" "${B[@]}" "${C[@]}"
  mapfile -t Bmiss < <(emit_family receipt 2 0 - completion-miss)
  expect_compose 0 "${fixture_root}/index.tsv" \
    two-successes-plus-reference-completion-misses-accepted "${A[@]}" "${Bmiss[@]}" "${C[@]}"

  # -- negatives, one rule each ----------------------------------------
  mapfile -t D < <(emit_family ibgw-data 7 8 \
    'ibgw-data|8|present|fail|not-applicable|ibgw-large-input-emission-semantic-failure|'"${SHA}"'|8|-')
  expect_compose 1 "${fixture_root}/index.tsv" \
    seven-successes-plus-one-semantic-failure-rejected "${D[@]}" "${B[@]}" "${C[@]}"

  mapfile -t E < <(emit_family ibgw-data 2 0 - absent 2)
  expect_compose 1 "${fixture_root}/index.tsv" \
    two-successes-then-missing-ordinals-rejected "${E[@]}" "${B[@]}" "${C[@]}"

  attempt_manifest "${fixture_root}/duplicate-ordinal" ibgw-data 3 present pass \
    not-applicable ibgw-large-input-multicell-verified "${SHA}" 8 "-"
  expect_compose 1 "${fixture_root}/index.tsv" \
    duplicate-ordinal-rejected "${A[@]}" "${B[@]}" "${C[@]}" "${fixture_root}/duplicate-ordinal"

  mapfile -t F < <(emit_family ibgw-data 2 3 \
    'ibgw-data|3|present|pass|not-applicable|ibgw-large-input-multicell-verified|'"${OTHER}"'|8|-')
  expect_compose 1 "${fixture_root}/index.tsv" \
    mixed-qualification-sha-rejected "${F[@]}" "${B[@]}" "${C[@]}"

  mapfile -t G < <(emit_family ibgw-data 2 3 \
    'ibgw-data|3|present|pass|not-applicable|ibgw-large-input-multicell-verified|'"${SHA}"'|9|-')
  expect_compose 1 "${fixture_root}/index.tsv" \
    changed-attempt-budget-rejected "${G[@]}" "${B[@]}" "${C[@]}"

  mapfile -t H < <(emit_family ibgw-data 2 3 \
    'ibgw-data|3|present|pass|not-applicable|some-unclassified-token|'"${SHA}"'|8|-')
  expect_compose 1 "${fixture_root}/index.tsv" \
    unclassified-terminal-rejected "${H[@]}" "${B[@]}" "${C[@]}"

  # Opportunity inferred from the downstream result: an opportunity-
  # present attempt can never carry semantic not-applicable.
  mapfile -t I < <(emit_family ibgw-data 2 3 \
    'ibgw-data|3|present|not-applicable|not-applicable|ibgw-large-input-multicell-verified|'"${SHA}"'|8|-')
  expect_compose 1 "${fixture_root}/index.tsv" \
    opportunity-inferred-from-output-rejected "${I[@]}" "${B[@]}" "${C[@]}"

  mapfile -t J < <(emit_family participant-lifecycle 2 3 \
    'participant-lifecycle|3|present|pass|pass|participant-forward-far-side-chain-verified|'"${SHA}"'|8|replay,expiry')
  expect_compose 1 "${fixture_root}/index.tsv" \
    partial-lifecycle-rows-borrowed-rejected "${A[@]}" "${B[@]}" "${J[@]}"

  mapfile -t K < <(emit_family participant-lifecycle 2 3 \
    'participant-lifecycle|3|present|fail|not-applicable|participant-lifecycle-row-semantic-failure|'"${SHA}"'|8|replay')
  expect_compose 1 "${fixture_root}/index.tsv" \
    replay-duplicate-forwarded-rejected "${A[@]}" "${B[@]}" "${K[@]}"

  attempt_manifest "${fixture_root}/legacy" ibgw-data 3 present pass not-applicable \
    ibgw-large-input-multicell-verified "${SHA}" 8 "-"
  python3 - "${fixture_root}/retained/obep-p1/evidence.json" \
    "${fixture_root}/legacy/evidence.json" <<'PYLEGACY'
import json
import sys
from pathlib import Path

legacy = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
Path(sys.argv[2]).write_text(json.dumps(legacy), encoding="utf-8")
PYLEGACY
  expect_compose 1 "${fixture_root}/index.tsv" \
    plan264-manifest-promotion-rejected "${A[@]}" "${B[@]}" "${C[@]}" "${fixture_root}/legacy"

  mapfile -t retained_few < <(find "${fixture_root}/retained" -mindepth 1 -maxdepth 1 -type d | head -9)
  if compose_265 "${fixture_root}/index.tsv" --retained "${retained_few[@]}" --attempt \
      "${A[@]}" "${B[@]}" "${C[@]}" >/dev/null 2>&1; then
    echo "check-m11-per-epoch-composition: self-test fixture missing-retained-manifest-rejected expected reject=1 got 0" >&2
    fixture_rc=1
  fi

  # The "opportunity inferred from output" rule has a static half too:
  # a deliberately output-reading opportunity predicate must be
  # rejected by the shared source-level guard.
  mutated="${fixture_root}/driver-mutated.rs"
  python3 - "${DRIVER}" "${mutated}" <<'PYMUTATE'
import sys
from pathlib import Path

source = Path(sys.argv[1]).read_text(encoding="utf-8")
needle = "if !self.registration_live_at_input {"
if needle not in source:
    raise SystemExit("input-side liveness conjunct not found; guard fixture cannot be built")
Path(sys.argv[2]).write_text(
    source.replace(needle, "if self.aux_count == 0 {", 1), encoding="utf-8"
)
PYMUTATE
  if check_input_side "${mutated}" >/dev/null 2>&1; then
    echo "check-m11-per-epoch-composition: self-test fixture static-opportunity-inferred-from-output-rejected expected reject=1 got 0" >&2
    fixture_rc=1
  fi

  if [[ "${fixture_rc}" -ne 0 ]]; then
    failures=$((failures + 1))
  else
    echo "check-m11-per-epoch-composition: self-test fixtures passed"
  fi
fi


if [[ "${failures}" -ne 0 ]]; then
  echo "check-m11-per-epoch-composition: ${failures} violation(s)" >&2
  exit 1
fi
echo "check-m11-per-epoch-composition: passed"
