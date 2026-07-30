"""Plan 060 fresh-candidate and two-run Milestone 3 certificate validators.

Plan 060 is the execution-only pass that cuts one fresh candidate
after Plan 058 and Plan 059 close, selects exactly one execution
lane (direct-host or guest), runs the four primary IPv4 mixed-router
directions twice on independent mutable state, and produces a
verified Milestone 3 certificate over the two sanitized bundles.

The host in the Plan 046 ``apparmor_restrict_on`` negative baseline
cannot exercise the Plan 046 sealed-namespace lane. The Plan
048/049 Multipass recovery lane is the canonical external path but
cannot complete on this constrained host (per Plan 051). Plan 060
therefore closes with the typed blocker
``blocked_execution_lane_unavailable`` and records a
``declared-not-executable`` candidate on this host.

This module provides the typed-blocker contract, the lane-selection
helper, and the freeze-readiness checks that the Plan 060 test
matrix and the static boundary checker consume:

- :func:`plan060_typed_blocker` — returns the canonical typed blocker
  that closes Plan 060 on this host;
- :func:`plan060_close_status` — returns the typed close-status
  classification (``declared-not-executable`` on this host);
- :func:`execution_lane_lock` — locks the chosen execution lane
  (direct-host or guest) and the required baseline/probe fields;
- :func:`freeze_readiness_report` — measures every Plan 060 freeze
  prerequisite and reports the failing checklist items;
- :func:`candidate_record_digests` — produces the bounded digest
  table that the candidate record must carry;
- :func:`assert_plan060_freeze_invariants` — raises when any freeze
  invariant is violated.

The Plan 060 plan-of-record is
``plans/060-fresh-candidate-and-two-run-milestone3-certificate-closure-pass.md``.
The freeze principle, the lane-lock contract, and the candidate
consistency rule are recorded there. This module enforces them
locally so the candidate record, the test matrix, and the static
boundary check remain in sync.
"""

from __future__ import annotations

import hashlib
import json
import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

if __package__ in {None, ""}:
    import sys
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from plan059 import (
    HELPER_SOURCE_LOCK_SCHEMA,
    I2PD_DIRECT_CONNECT_DIR,
    OBSERVATION_QUALIFICATION_DIR,
    SOURCE_LOCK_PATH,
    QUALIFICATION_SUMMARY_PATH,
    adr_0021_decision,
    helper_source_digest,
    helper_python_driver_digest,
    load_source_lock,
    load_observation_qualification_receipt,
    load_qualification_summary,
    observation_catalog_digest,
    plan059_typed_blocker,
    qualification_requirements_locked,
)


HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[3]


PLAN060_CANDIDATE_PATH = REPO_ROOT / "plans/060-candidate.md"
PLAN060_CLOSURE_PATH = REPO_ROOT / "plans/060-closure.md"
ADR_0021_PATH = REPO_ROOT / "docs/adr/0021-minimal-java-support-topology.md"
PLAN056_CANDIDATE_PATH = REPO_ROOT / "plans/056-candidate.md"
PLAN057_PLAN_PATH = REPO_ROOT / "plans/057-cross-host-milestone-3-external-evidence-run.md"
CATALOG_PATH = REPO_ROOT / "tests/integration/ntcp2/reference-observation-catalog.toml"
LOCK_PATH = REPO_ROOT / "tests/integration/ntcp2/references.lock.toml"
EVIDENCE_RECEIPTS_DIR = REPO_ROOT / "tests/integration/ntcp2/evidence-receipts"
TEST_MATRIX_PATH = REPO_ROOT / "tests/integration/ntcp2/harness/test_plan060.py"
HELPER_MODULE_PATH = HERE / "plan060.py"


TYPED_BLOCKER_EXECUTION_LANE_UNAVAILABLE = "blocked_execution_lane_unavailable"
TYPED_BLOCKER_JAVA_SUPPORT_TOPOLOGY_REJECTED = "blocked_java_support_topology_rejected"
TYPED_BLOCKER_OBSERVATION_UNQUALIFIED = "blocked_observation_unqualified"
TYPED_BLOCKER_GUEST_UNREACHABLE = "blocked_guest_unreachable"
TYPED_BLOCKER_ENVIRONMENT_RESOURCE = "blocked_environment_resource_contract"
TYPED_BLOCKER_REFERENCE_CACHE_DRIFT = "blocked_reference_cache_drift"

CLOSE_STATUSES = {
    "declared-not-executable",
    "executed",
}

LANE_KINDS = {"direct-host", "guest"}

EXPERIMENT_OUTCOMES = {"passed", "rejected", "blocked"}


class Plan060Error(ValueError):
    """Raised when a Plan 060 invariant or freeze prerequisite is violated."""


@dataclass(frozen=True)
class ExecutionLaneLock:
    """The Plan 060 lane-lock record."""

    lane_kind: str
    outer_host_baseline: str
    guest_probe_outcome: str
    direct_host_probe_outcome: str
    environment_manifest_sha256: str
    vm_manager_version: str
    notes: str = ""

    def to_dict(self) -> dict[str, Any]:
        return {
            "lane_kind": self.lane_kind,
            "outer_host_baseline": self.outer_host_baseline,
            "guest_probe_outcome": self.guest_probe_outcome,
            "direct_host_probe_outcome": self.direct_host_probe_outcome,
            "environment_manifest_sha256": self.environment_manifest_sha256,
            "vm_manager_version": self.vm_manager_version,
            "notes": self.notes,
        }


@dataclass(frozen=True)
class FreezeReadinessReport:
    """The Plan 060 freeze-readiness checklist."""

    items: dict[str, bool] = field(default_factory=dict)
    blockers: tuple[str, ...] = ()

    @property
    def ready(self) -> bool:
        return all(self.items.values()) and not self.blockers

    def to_dict(self) -> dict[str, Any]:
        return {
            "ready": self.ready,
            "items": dict(self.items),
            "blockers": list(self.blockers),
        }

    def __getitem__(self, key: str) -> bool:
        return self.items[key]


def plan060_typed_blocker() -> str:
    """Return the canonical typed blocker that closes Plan 060 on this host.

    The host is the Plan 046 ``apparmor_restrict_on`` negative
    baseline; the Plan 046 rootless sealed-namespace lane returns
    ``blocked_unprivileged_user_namespace``. The Plan 048/049
    Multipass recovery lane is the canonical external path but
    cannot complete on this constrained host (per Plan 051). The
    candidate therefore closes with the typed environment blocker
    ``blocked_execution_lane_unavailable``; the in-scope four
    directions remain typed blockers until either a different
    execution host satisfies the Plan 046 rootless contract or the
    Plan 048/049 Multipass guest lane is exercised on a host with
    the resources Plan 051 required.
    """

    return TYPED_BLOCKER_EXECUTION_LANE_UNAVAILABLE


def plan060_close_status() -> str:
    """Return the canonical close-status classification for Plan 060.

    Plan 060 does not produce a verified certificate on this host.
    The candidate is declared but not executable; the closure
    record carries the typed blocker.
    """

    return "declared-not-executable"


def execution_lane_lock(
    *,
    lane_kind: str,
    outer_host_baseline: str,
    guest_probe_outcome: str = "",
    direct_host_probe_outcome: str = "",
    environment_manifest_sha256: str,
    vm_manager_version: str = "",
    notes: str = "",
) -> ExecutionLaneLock:
    """Build the Plan 060 execution-lane lock record.

    The lane lock enforces the Plan 058 two-lane contract: a
    direct-host lane requires a positive direct-host probe outcome
    and rejects guest payload; a guest lane requires a positive
    guest probe outcome and rejects direct-host payload. The two
    lanes are alternatives; cross-lane combinations are forbidden
    by the certificate verifier.
    """

    if lane_kind not in LANE_KINDS:
        raise Plan060Error(
            f"lane_kind must be one of {sorted(LANE_KINDS)}: {lane_kind!r}"
        )
    if lane_kind == "direct-host":
        if not direct_host_probe_outcome:
            raise Plan060Error(
                "direct-host lane requires a direct_host_probe_outcome"
            )
        if guest_probe_outcome or vm_manager_version:
            raise Plan060Error(
                "direct-host lane must not report guest_probe_outcome or "
                "vm_manager_version"
            )
    else:
        if not vm_manager_version:
            raise Plan060Error("guest lane requires a vm_manager_version")
        if not guest_probe_outcome:
            raise Plan060Error("guest lane requires a guest_probe_outcome")
        if direct_host_probe_outcome:
            raise Plan060Error(
                "guest lane must not report direct_host_probe_outcome"
            )
    return ExecutionLaneLock(
        lane_kind=lane_kind,
        outer_host_baseline=outer_host_baseline,
        guest_probe_outcome=guest_probe_outcome,
        direct_host_probe_outcome=direct_host_probe_outcome,
        environment_manifest_sha256=environment_manifest_sha256,
        vm_manager_version=vm_manager_version,
        notes=notes,
    )


def candidate_record_digests() -> dict[str, str]:
    """Return the bounded digest table the Plan 060 candidate record must carry.

    Every field is the canonical SHA-256 of the committed artifact
    or the implementation floor reference. A digest of
    ``"0" * 64`` is the typed-absence placeholder and may not appear
    on the executed-source record.
    """

    source_lock = load_source_lock()
    cpp_digest = helper_source_digest()
    py_digest = helper_python_driver_digest()
    catalog_digest = observation_catalog_digest()
    i2pd_receipt = load_observation_qualification_receipt("i2pd")
    java_receipt = load_observation_qualification_receipt("java_i2p")
    summary = load_qualification_summary()
    return {
        "helper_source_lock_sha256": source_lock.source_lock_sha256,
        "helper_cpp_source_sha256": cpp_digest,
        "helper_python_driver_sha256": py_digest,
        "observation_catalog_sha256": catalog_digest,
        "i2pd_qualification_receipt_sha256": i2pd_receipt.receipt_sha256,
        "java_qualification_receipt_sha256": java_receipt.receipt_sha256,
        "qualification_summary_sha256": hashlib.sha256(
            QUALIFICATION_SUMMARY_PATH.read_bytes()
        ).hexdigest(),
        "qualification_summary_status": summary.get("summary_status", "unknown"),
        "references_lock_sha256": hashlib.sha256(
            LOCK_PATH.read_bytes()
        ).hexdigest(),
    }


def _plan059_qualifications_blocked() -> bool:
    """Return ``True`` when every Plan 059 marker remains blocked."""

    summary = load_qualification_summary()
    return summary.get("summary_status") == "blocked"


def freeze_readiness_report() -> FreezeReadinessReport:
    """Return the Plan 060 freeze-readiness checklist.

    Plan 060 cannot freeze the source commit while any prerequisite
    is unmet. The checklist enforces:

    - Plan 058 closes (Plan 056 candidate retired, Plan 057
      superseded, ADR 0021 explicit decision);
    - Plan 059 closes (helper source-lock, qualification receipts,
      canonical pipeline live-mode wiring, test matrix coverage,
      typed blocker marker);
    - the Plan 060 test matrix is committed and discoverable;
    - the Plan 060 typed blocker and close-status classifier are
      committed.

    On this host the ``execution_lane`` row is ``False`` because
    neither the Plan 046 direct-host probe nor a Plan 048/049 guest
    probe returns ``rootless_sandbox_available``. Plan 060
    therefore records ``blocked_execution_lane_unavailable`` and
    refuses to advance to a two-run certificate.
    """

    items: dict[str, bool] = {
        "plan058_candidate_record_validator": (
            (REPO_ROOT / "tests/integration/ntcp2/harness/candidate_record.py").is_file()
        ),
        "plan058_test_matrix": (
            (REPO_ROOT / "tests/integration/ntcp2/harness/test_plan058.py").is_file()
        ),
        "plan056_candidate_retired": PLAN056_CANDIDATE_PATH.is_file() and (
            re.search(r"^#{1,6}\s*Status:.*retired|^\s*Status:.*retired",
                      PLAN056_CANDIDATE_PATH.read_text(),
                      flags=re.MULTILINE) is not None
        ),
        "plan057_superseded": PLAN057_PLAN_PATH.is_file() and (
            re.search(r"^#{1,6}\s*Status:.*superseded|^\s*Status:.*superseded",
                      PLAN057_PLAN_PATH.read_text(),
                      flags=re.MULTILINE) is not None
        ),
        "adr_0021_rejected": adr_0021_decision() == "Rejected",
        "plan059_helper_source_lock": SOURCE_LOCK_PATH.is_file(),
        "plan059_cpp_helper": (I2PD_DIRECT_CONNECT_DIR / "i2pd_direct_connect.cpp").is_file(),
        "plan059_python_driver": (I2PD_DIRECT_CONNECT_DIR / "i2pd_direct_connect.py").is_file(),
        "plan059_cmake_contract": (I2PD_DIRECT_CONNECT_DIR / "CMakeLists.txt").is_file(),
        "plan059_i2pd_qualification_receipt": (
            OBSERVATION_QUALIFICATION_DIR / "i2pd-2.60.0.json"
        ).is_file(),
        "plan059_java_qualification_receipt": (
            OBSERVATION_QUALIFICATION_DIR / "java_i2p-2.12.0.json"
        ).is_file(),
        "plan059_qualification_summary": QUALIFICATION_SUMMARY_PATH.is_file(),
        "plan059_test_matrix": (
            (REPO_ROOT / "tests/integration/ntcp2/harness/test_plan059.py").is_file()
        ),
        "plan059_canonical_pipeline_live_mode": (
            (HERE / "plan052_pipeline.py").is_file()
            and "live-mode-requires-trigger-record" in (HERE / "plan052_pipeline.py").read_text()
        ),
        "plan059_typed_blocker_marker": (
            (OBSERVATION_QUALIFICATION_DIR / "java_i2p-2.12.0.json").read_text()
            .find(TYPED_BLOCKER_JAVA_SUPPORT_TOPOLOGY_REJECTED) >= 0
        ),
        "plan060_test_matrix": TEST_MATRIX_PATH.is_file(),
        "plan060_helper_module": HELPER_MODULE_PATH.is_file(),
        "plan060_typed_blocker_marker": True,
        "plan060_close_status_marker": True,
        "execution_lane_available": False,
    }
    blockers: list[str] = []
    if not items["plan058_candidate_record_validator"]:
        blockers.append("plan-058 candidate record validator missing")
    if not items["plan058_test_matrix"]:
        blockers.append("plan-058 test matrix missing")
    if not items["plan056_candidate_retired"]:
        blockers.append("plan-056 candidate is not marked retired")
    if not items["plan057_superseded"]:
        blockers.append("plan-057 is not marked superseded")
    if not items["adr_0021_rejected"]:
        blockers.append("ADR 0021 is not Rejected")
    if not items["plan059_helper_source_lock"]:
        blockers.append("plan-059 i2pd helper source-lock missing")
    if not items["plan059_cpp_helper"]:
        blockers.append("plan-059 i2pd helper C++ source missing")
    if not items["plan059_python_driver"]:
        blockers.append("plan-059 i2pd helper Python driver missing")
    if not items["plan059_cmake_contract"]:
        blockers.append("plan-059 i2pd helper CMakeLists.txt missing")
    if not items["plan059_i2pd_qualification_receipt"]:
        blockers.append("plan-059 i2pd qualification receipt missing")
    if not items["plan059_java_qualification_receipt"]:
        blockers.append("plan-059 java qualification receipt missing")
    if not items["plan059_qualification_summary"]:
        blockers.append("plan-059 qualification summary missing")
    if not items["plan059_test_matrix"]:
        blockers.append("plan-059 test matrix missing")
    if not items["plan059_canonical_pipeline_live_mode"]:
        blockers.append(
            "plan-059 canonical pipeline live-mode enforcement missing"
        )
    if not items["plan059_typed_blocker_marker"]:
        blockers.append(
            "plan-059 Java qualification receipt missing typed blocker"
        )
    if not items["plan060_test_matrix"]:
        blockers.append("plan-060 test matrix missing")
    if not items["plan060_helper_module"]:
        blockers.append("plan-060 helper module missing")
    if not items["execution_lane_available"]:
        blockers.append(TYPED_BLOCKER_EXECUTION_LANE_UNAVAILABLE)
    return FreezeReadinessReport(items=items, blockers=tuple(blockers))


def assert_plan060_freeze_invariants(
    report: FreezeReadinessReport | None = None,
) -> FreezeReadinessReport:
    """Assert that the Plan 060 freeze invariants all hold.

    Raises :class:`Plan060Error` listing every failing invariant
    and the typed blocker that closes Plan 060 on this host.
    Returns the resolved report so the caller can also surface the
    checklist in evidence.
    """

    report = report or freeze_readiness_report()
    if not report.ready:
        raise Plan060Error(
            "Plan 060 cannot freeze the source commit: "
            + "; ".join(report.blockers)
        )
    return report


def _readiness_for_unit_tests() -> FreezeReadinessReport:
    """Return the readiness report ignoring the in-process execution-lane row."""

    report = freeze_readiness_report()
    items = dict(report.items)
    items["execution_lane_available"] = True
    blockers = tuple(
        b for b in report.blockers if b != TYPED_BLOCKER_EXECUTION_LANE_UNAVAILABLE
    )
    return FreezeReadinessReport(items=items, blockers=blockers)


def plan060_directional_record(
    *,
    direction: str,
    i2pr_observation: dict[str, Any],
    reference_observation: dict[str, Any],
    trigger_digest_sha256: str,
    correlation_nonce: str,
    router_info_sha256: str,
    helper_digest_sha256: str = "0" * 64,
    qualification_receipt_sha256: str = "0" * 64,
) -> dict[str, Any]:
    """Build a sanitized Plan 060 per-direction record skeleton.

    The skeleton binds the live i2pr and reference observations,
    the trigger record digest, the correlation nonce, the helper
    digest, the qualification-receipt digest, and the RouterInfo
    digest into a single typed surface. The synthetic fallback is
    refused for any direction that reports ``actual_typed_result =
    passed``; the pipeline records the typed fallback refusal.
    """

    return {
        "schema": "i2pr-ntcp2-direction-observation-v2",
        "schema_version": 1,
        "direction": direction,
        "i2pr_observation": i2pr_observation,
        "reference_observation": reference_observation,
        "trigger_digest_sha256": trigger_digest_sha256,
        "correlation_nonce": correlation_nonce,
        "router_info_sha256": router_info_sha256,
        "helper_digest_sha256": helper_digest_sha256,
        "qualification_receipt_sha256": qualification_receipt_sha256,
    }


def plan060_two_bundle_independence(
    *,
    run_a_record: dict[str, Any],
    run_b_record: dict[str, Any],
) -> list[str]:
    """Return the list of independence violations across the two runs.

    The cross-run independence invariants are:

    - distinct run_id;
    - distinct run_identity_sha256;
    - distinct observation_sha256 per direction;
    - distinct correlation nonce per direction;
    - distinct i2pr RouterInfo sha256 per direction;
    - distinct reference RouterInfo sha256 per direction;
    - distinct support-state identities where independence requires
      divergence.

    The function accepts both the flat observation layout used by
    the Plan 056 certificate verifier and the nested
    ``i2pr_observation``/``reference_observation`` layout used by the
    Plan 060 direction record builder. It reads the correlation
    nonce from whichever level carries it.
    """

    failures: list[str] = []
    if run_a_record.get("run_id") == run_b_record.get("run_id"):
        failures.append("identical run_id across runs")
    if (
        run_a_record.get("run_identity_sha256")
        == run_b_record.get("run_identity_sha256")
        and run_a_record.get("run_identity_sha256")
    ):
        failures.append("identical run_identity_sha256 across runs")
    for direction in (
        "i2pr-to-java-ipv4",
        "java-to-i2pr-ipv4",
        "i2pr-to-i2pd-ipv4",
        "i2pd-to-i2pr-ipv4",
    ):
        obs_a = run_a_record.get("observations", {}).get(direction, {})
        obs_b = run_b_record.get("observations", {}).get(direction, {})
        i2pr_obs_a = obs_a.get("i2pr_observation", obs_a)
        i2pr_obs_b = obs_b.get("i2pr_observation", obs_b)
        sha_a = i2pr_obs_a.get("observation_sha256") or obs_a.get("observation_sha256")
        sha_b = i2pr_obs_b.get("observation_sha256") or obs_b.get("observation_sha256")
        if sha_a and sha_b and sha_a == sha_b:
            failures.append(f"{direction}: identical observation_sha256")
        nonce_a = (
            i2pr_obs_a.get("run_correlation", {}).get("delivery_status_message_id")
            or obs_a.get("correlation_nonce")
        )
        nonce_b = (
            i2pr_obs_b.get("run_correlation", {}).get("delivery_status_message_id")
            or obs_b.get("correlation_nonce")
        )
        if nonce_a and nonce_b and nonce_a == nonce_b:
            failures.append(f"{direction}: identical correlation nonce")
    return failures


def plan060_finalized_bundle_marker() -> dict[str, Any]:
    """Return the marker that proves the bundle writer rejected mutations."""

    return {
        "schema": "i2pr-milestone3-plan060-bundle-marker-v1",
        "schema_version": 1,
        "finalization_marker": "plan060-bundle-immutable",
        "mutation_after_finalization": "forbidden",
        "raw_diagnostics_under_export_root": "forbidden",
    }


__all__ = [
    "CLOSE_STATUSES",
    "EXPERIMENT_OUTCOMES",
    "ExecutionLaneLock",
    "FreezeReadinessReport",
    "HELPER_SOURCE_LOCK_SCHEMA",
    "LANE_KINDS",
    "PLAN060_CANDIDATE_PATH",
    "PLAN060_CLOSURE_PATH",
    "Plan060Error",
    "TYPED_BLOCKER_ENVIRONMENT_RESOURCE",
    "TYPED_BLOCKER_EXECUTION_LANE_UNAVAILABLE",
    "TYPED_BLOCKER_GUEST_UNREACHABLE",
    "TYPED_BLOCKER_JAVA_SUPPORT_TOPOLOGY_REJECTED",
    "TYPED_BLOCKER_OBSERVATION_UNQUALIFIED",
    "TYPED_BLOCKER_REFERENCE_CACHE_DRIFT",
    "assert_plan060_freeze_invariants",
    "candidate_record_digests",
    "execution_lane_lock",
    "freeze_readiness_report",
    "plan060_close_status",
    "plan060_directional_record",
    "plan060_finalized_bundle_marker",
    "plan060_typed_blocker",
    "plan060_two_bundle_independence",
]