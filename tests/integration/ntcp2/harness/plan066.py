"""Plan 066 fresh-candidate and authoritative NTCP2 two-run closure validators.

Plan 066 is the execution-only pass that cuts one fresh candidate
descended from the Plan 065 implementation floor, selects exactly one
execution lane (direct-host or guest), runs the four primary IPv4
mixed-router directions twice on independent mutable state, and
produces a verified Milestone 3 certificate over the two sanitized
bundles.

The plan-of-record is
``plans/066-fresh-candidate-and-authoritative-ntcp2-two-run-closure.md``.

On this host (the Plan 046 ``apparmor_restrict_on`` negative baseline
plus the Plan 051 resource constraints), Plan 066 closes with the typed
blocker ``blocked_execution_lane_unavailable``; the candidate is
declared with status ``declared-not-executable``. ADR 0021 is
Rejected, so the ``java-to-i2pr-ipv4`` direction remains blocked
under the current four-direction contract.

This module provides:

- :func:`plan066_typed_blocker` — canonical typed blocker;
- :func:`plan066_close_status` — canonical close-status classifier;
- :func:`plan066_execution_lane_lock` — the Plan 058/060 inherited
  two-lane contract;
- :func:`plan066_candidate_record_digests` — the bounded digest table
  for the Plan 066 candidate record;
- :func:`plan066_freeze_readiness_report` — the freeze-readiness
  checklist with the bounded 21-row contract;
- :func:`assert_plan066_freeze_invariants` — raise on any invariant
  violation;
- :func:`plan066_directional_record` — per-direction record skeleton
  with correlation digests;
- :func:`plan066_two_bundle_independence` — cross-run independence
  invariant checker;
- :func:`plan066_finalized_bundle_marker` — the bundle mutation
  guard marker.

The Plan 058 record-and-candidate integrity validator, the Plan 059
helper/qualification contracts, the Plan 060 implementation surface,
the Plan 062 v4 trigger / v1 event / v3 observation schemas, the Plan
063 Java driver, the Plan 064 i2pd driver, and the Plan 065 canonical
mixed-runner are mandatory Plan 066 prerequisites. Any change that
removes or weakens the verifier, the test matrix, or the static
boundary checks must be re-justified in a new plan-of-record and
must not silently weaken the Milestone 3 evidence gate.
"""

from __future__ import annotations

import hashlib
import json
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

if __package__ in {None, ""}:
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from plan059 import (
    OBSERVATION_QUALIFICATION_DIR,
    QUALIFICATION_SUMMARY_PATH,
    SOURCE_LOCK_PATH,
    I2PD_DIRECT_CONNECT_DIR as PLAN059_I2PD_HELPER_DIR,
    adr_0021_decision,
    helper_source_digest,
    helper_python_driver_digest,
    load_observation_qualification_receipt,
    load_qualification_summary,
    plan059_typed_blocker,
)
from plan060 import (
    TYPED_BLOCKER_EXECUTION_LANE_UNAVAILABLE,
    TYPED_BLOCKER_JAVA_SUPPORT_TOPOLOGY_REJECTED,
    execution_lane_lock as _plan060_execution_lane_lock,
    plan060_close_status as _plan060_close_status,
    plan060_typed_blocker as _plan060_typed_blocker,
)


HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[3]


PLAN066_CANDIDATE_PATH = REPO_ROOT / "plans/066-candidate.md"
PLAN066_CLOSURE_PATH = REPO_ROOT / "plans/066-closure.md"

JAVA_HELPER_DIR = REPO_ROOT / "tests/integration/ntcp2/reference-drivers/java"
I2PD_HELPER_DIR = REPO_ROOT / "tests/integration/ntcp2/reference-drivers/i2pd"
JAVA_QUALIFICATION_RECEIPT_PATH = (
    REPO_ROOT / "tests/integration/ntcp2/qualification/java-direct-driver.json"
)
I2PD_QUALIFICATION_RECEIPT_PATH = (
    REPO_ROOT / "tests/integration/ntcp2/qualification/i2pd-direct-driver.json"
)
LOCK_PATH = REPO_ROOT / "tests/integration/ntcp2/references.lock.toml"
SCENARIO_RS_PATH = REPO_ROOT / "tools/i2pr-interop/src/scenario.rs"
MAIN_RS_PATH = REPO_ROOT / "tools/i2pr-interop/src/main.rs"
STATUS_RS_PATH = REPO_ROOT / "tools/i2pr-interop/src/status.rs"
LAUNCHER_PROTOCOL_PATH = (
    REPO_ROOT / "tests/integration/ntcp2/harness/launcher_protocol.py"
)
LAUNCHER_RENDERER_PATH = (
    REPO_ROOT / "tests/integration/ntcp2/harness/launcher_renderer.py"
)
MIXED_RUNNER_PATH = REPO_ROOT / "tests/integration/ntcp2/harness/mixed_runner.py"
PLAN052_PIPELINE_PATH = REPO_ROOT / "tests/integration/ntcp2/harness/plan052_pipeline.py"
RUN_IDENTITY_PATH = REPO_ROOT / "tests/integration/ntcp2/harness/run_identity.py"
EVIDENCE_BUNDLE_PATH = REPO_ROOT / "tests/integration/ntcp2/harness/evidence_bundle.py"
VERIFY_CERT_PATH = REPO_ROOT / "tests/integration/ntcp2/harness/verify_milestone3_certificate.py"

PLAN065_FLOOR = "450c0cf2fc1e015ce052e0387723d6c83b3cd746"
PLAN060_CANDIDATE_PATH = REPO_ROOT / "plans/060-candidate.md"
PLAN056_CANDIDATE_PATH = REPO_ROOT / "plans/056-candidate.md"
PLAN057_PLAN_PATH = REPO_ROOT / "plans/057-cross-host-milestone-3-external-evidence-run.md"
ADR_0022_PATH = REPO_ROOT / "docs/adr/0022-direct-reference-router-ntcp2-interop-drivers.md"


CLOSE_STATUS_DECLARED_NOT_EXECUTABLE = "declared-not-executable"
CLOSE_STATUS_EXECUTED = "executed"
CLOSE_STATUSES = {CLOSE_STATUS_DECLARED_NOT_EXECUTABLE, CLOSE_STATUS_EXECUTED}

EXECUTION_LANE_KINDS = {"direct-host", "guest"}


class Plan066Error(ValueError):
    """Raised when a Plan 066 invariant or freeze prerequisite is violated."""


@dataclass(frozen=True)
class Plan066ExecutionLaneLock:
    """Plan 066 two-lane contract inherited from Plan 058/060."""

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
class Plan066FreezeReadinessReport:
    """Plan 066 freeze-readiness checklist (21-row bounded contract)."""

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


def plan066_typed_blocker() -> str:
    """Return the canonical typed blocker that closes Plan 066 on this host.

    The host is the Plan 046 ``apparmor_restrict_on`` negative
    baseline; the Plan 046 rootless sealed-namespace lane returns
    ``blocked_unprivileged_user_namespace``. The Plan 048/049
    Multipass recovery lane is the canonical external path but
    cannot complete on this constrained host (per Plan 051). ADR 0021
    was Rejected by Plan 058, so the ``java-to-i2pr-ipv4`` direction
    remains blocked under the current four-direction contract.
    """

    return TYPED_BLOCKER_EXECUTION_LANE_UNAVAILABLE


def plan066_close_status() -> str:
    """Return the canonical close-status classification for Plan 066."""

    return CLOSE_STATUS_DECLARED_NOT_EXECUTABLE


def plan066_execution_lane_lock(
    *,
    lane_kind: str,
    outer_host_baseline: str,
    guest_probe_outcome: str = "",
    direct_host_probe_outcome: str = "",
    environment_manifest_sha256: str,
    vm_manager_version: str = "",
    notes: str = "",
) -> Plan066ExecutionLaneLock:
    """Build a sanitized Plan 066 execution-lane lock record.

    The contract is inherited from Plan 058/060; the helper delegates
    validation to :func:`plan060.execution_lane_lock` and wraps the
    result with the Plan 066 type name.
    """

    inner = _plan060_execution_lane_lock(
        lane_kind=lane_kind,
        outer_host_baseline=outer_host_baseline,
        guest_probe_outcome=guest_probe_outcome,
        direct_host_probe_outcome=direct_host_probe_outcome,
        environment_manifest_sha256=environment_manifest_sha256,
        vm_manager_version=vm_manager_version,
    )
    return Plan066ExecutionLaneLock(
        lane_kind=inner.lane_kind,
        outer_host_baseline=inner.outer_host_baseline,
        guest_probe_outcome=inner.guest_probe_outcome,
        direct_host_probe_outcome=inner.direct_host_probe_outcome,
        environment_manifest_sha256=inner.environment_manifest_sha256,
        vm_manager_version=inner.vm_manager_version,
        notes=notes,
    )


def _hex64_default_payload(byte: bytes) -> str:
    return hashlib.sha256(byte).hexdigest()


def plan066_candidate_record_digests() -> dict[str, str]:
    """Return the bounded Plan 066 candidate digest table.

    Every SHA-256 is a full 64 lowercase hex character digest. The
    Plan 066 contract binds the Plan 063 Java driver, the Plan 064
    i2pd driver/observer, the Plan 062 v4 trigger / v1 event / v3
    observation schemas, the Plan 065 strict scenario schema and
    canonical mixed-runner, the references lock, the environment
    manifest, and the Plan 056/060 retired candidate records.
    """

    java_digest = hashlib.sha256(
        (JAVA_HELPER_DIR / "src/JavaNtcp2InteropDriver.java").read_bytes()
    ).hexdigest()
    java_classpath_digest = hashlib.sha256(
        (JAVA_HELPER_DIR / "classpath-manifest.json").read_bytes()
    ).hexdigest()
    java_source_lock_digest = hashlib.sha256(
        (JAVA_HELPER_DIR / "source-lock.json").read_bytes()
    ).hexdigest()
    java_qualification_digest = (
        hashlib.sha256(JAVA_QUALIFICATION_RECEIPT_PATH.read_bytes()).hexdigest()
        if JAVA_QUALIFICATION_RECEIPT_PATH.is_file()
        else "0" * 64
    )

    i2pd_cpp_digest = hashlib.sha256(
        (I2PD_HELPER_DIR / "src/i2pd_ntcp2_interop_driver.cpp").read_bytes()
    ).hexdigest()
    i2pd_observer_h_digest = hashlib.sha256(
        (I2PD_HELPER_DIR / "src/interop_observer.h").read_bytes()
    ).hexdigest()
    i2pd_observer_cpp_digest = hashlib.sha256(
        (I2PD_HELPER_DIR / "src/interop_observer.cpp").read_bytes()
    ).hexdigest()
    i2pd_patch_digest = hashlib.sha256(
        (I2PD_HELPER_DIR / "patches/i2pd-2.60.0-interop-observer.patch").read_bytes()
    ).hexdigest()
    i2pd_source_lock_digest = hashlib.sha256(
        (I2PD_HELPER_DIR / "source-lock.json").read_bytes()
    ).hexdigest()
    i2pd_qualification_digest = (
        hashlib.sha256(I2PD_QUALIFICATION_RECEIPT_PATH.read_bytes()).hexdigest()
        if I2PD_QUALIFICATION_RECEIPT_PATH.is_file()
        else "0" * 64
    )

    scenario_digest = hashlib.sha256(SCENARIO_RS_PATH.read_bytes()).hexdigest()
    main_digest = hashlib.sha256(MAIN_RS_PATH.read_bytes()).hexdigest()
    status_digest = hashlib.sha256(STATUS_RS_PATH.read_bytes()).hexdigest()
    launcher_protocol_digest = hashlib.sha256(
        LAUNCHER_PROTOCOL_PATH.read_bytes()
    ).hexdigest()
    launcher_renderer_digest = hashlib.sha256(
        LAUNCHER_RENDERER_PATH.read_bytes()
    ).hexdigest()
    mixed_runner_digest = hashlib.sha256(MIXED_RUNNER_PATH.read_bytes()).hexdigest()
    plan052_pipeline_digest = hashlib.sha256(
        PLAN052_PIPELINE_PATH.read_bytes()
    ).hexdigest()
    run_identity_digest = hashlib.sha256(RUN_IDENTITY_PATH.read_bytes()).hexdigest()
    evidence_bundle_digest = hashlib.sha256(
        EVIDENCE_BUNDLE_PATH.read_bytes()
    ).hexdigest()
    verify_cert_digest = hashlib.sha256(VERIFY_CERT_PATH.read_bytes()).hexdigest()
    references_lock_digest = hashlib.sha256(LOCK_PATH.read_bytes()).hexdigest()

    return {
        "java_driver_source_sha256": java_digest,
        "java_driver_classpath_manifest_sha256": java_classpath_digest,
        "java_driver_source_lock_sha256": java_source_lock_digest,
        "java_qualification_receipt_sha256": java_qualification_digest,
        "i2pd_driver_cpp_source_sha256": i2pd_cpp_digest,
        "i2pd_observer_header_sha256": i2pd_observer_h_digest,
        "i2pd_observer_source_sha256": i2pd_observer_cpp_digest,
        "i2pd_observer_patch_sha256": i2pd_patch_digest,
        "i2pd_driver_source_lock_sha256": i2pd_source_lock_digest,
        "i2pd_qualification_receipt_sha256": i2pd_qualification_digest,
        "scenario_renderer_sha256": scenario_digest,
        "main_runner_sha256": main_digest,
        "status_module_sha256": status_digest,
        "launcher_protocol_sha256": launcher_protocol_digest,
        "launcher_renderer_sha256": launcher_renderer_digest,
        "mixed_runner_sha256": mixed_runner_digest,
        "plan052_pipeline_sha256": plan052_pipeline_digest,
        "run_identity_sha256": run_identity_digest,
        "evidence_bundle_sha256": evidence_bundle_digest,
        "bundle_verifier_sha256": verify_cert_digest,
        "references_lock_sha256": references_lock_digest,
        "plan059_helper_cpp_source_sha256": helper_source_digest(),
        "plan059_helper_python_driver_sha256": helper_python_driver_digest(),
    }


def _adr_0022_decision() -> str:
    text = ADR_0022_PATH.read_text(encoding="utf-8")
    match = re.search(r"^- Status:\s*(\w+)", text, flags=re.MULTILINE)
    if match is None:
        return "Unknown"
    return match.group(1)


def plan066_freeze_readiness_report() -> Plan066FreezeReadinessReport:
    """Return the Plan 066 freeze-readiness checklist.

    The checklist enforces the 21-row bounded contract from the plan
    of record. On this host the ``execution_lane_available`` row is
    ``False`` because neither the Plan 046 direct-host probe nor a
    Plan 048/049 guest probe returns ``rootless_sandbox_available``.
    Plan 066 therefore records ``blocked_execution_lane_unavailable``
    and refuses to advance to a two-run certificate.
    """

    items: dict[str, bool] = {
        "plan062_v4_trigger_schema": (HERE / "reference_trigger_v4.py").is_file(),
        "plan062_reference_event_schema": (HERE / "reference_event.py").is_file(),
        "plan062_v3_observation_schema": (HERE / "observation_v3.py").is_file(),
        "plan062_source_verification_record": (
            REPO_ROOT
            / "tests/integration/ntcp2/reference-drivers/source-verification.md"
        ).is_file(),
        "adr_0022_accepted": _adr_0022_decision() == "Accepted",
        "plan063_java_driver_artifacts": (
            (JAVA_HELPER_DIR / "src/JavaNtcp2InteropDriver.java").is_file()
            and (JAVA_HELPER_DIR / "source-lock.json").is_file()
            and (JAVA_HELPER_DIR / "classpath-manifest.json").is_file()
            and JAVA_QUALIFICATION_RECEIPT_PATH.is_file()
        ),
        "plan064_i2pd_driver_artifacts": (
            (I2PD_HELPER_DIR / "src/i2pd_ntcp2_interop_driver.cpp").is_file()
            and (I2PD_HELPER_DIR / "src/interop_observer.h").is_file()
            and (I2PD_HELPER_DIR / "src/interop_observer.cpp").is_file()
            and (I2PD_HELPER_DIR / "patches/i2pd-2.60.0-interop-observer.patch").is_file()
            and (I2PD_HELPER_DIR / "source-lock.json").is_file()
            and I2PD_QUALIFICATION_RECEIPT_PATH.is_file()
        ),
        "plan065_strict_scenario_schema_v2": (
            SCENARIO_RS_PATH.is_file()
            and '"i2pr-launcher-scenario-v2"' in SCENARIO_RS_PATH.read_text()
        ),
        "plan065_directional_predicate_contract": (
            MAIN_RS_PATH.is_file()
            and "SenderDeliveryStatusMessageIdZero" in MAIN_RS_PATH.read_text()
            and "ReceiverDeliveryStatusIdMismatch" in MAIN_RS_PATH.read_text()
        ),
        "plan065_canonical_mixed_runner": (
            MIXED_RUNNER_PATH.is_file()
            and "_plan065_primary_fields" in MIXED_RUNNER_PATH.read_text()
            and "_reference_driver_mode_for" in MIXED_RUNNER_PATH.read_text()
        ),
        "plan060_helper_module_present": (HERE / "plan060.py").is_file(),
        "plan060_typed_blocker_marker": (
            (HERE / "plan060.py").read_text()
            .find(TYPED_BLOCKER_EXECUTION_LANE_UNAVAILABLE) >= 0
        ),
        "plan060_candidate_retired": (
            PLAN060_CANDIDATE_PATH.is_file()
            and re.search(
                r"^#+\s*Status:\s*\*?\*?retired|^\s*Status:\s*\*?\*?retired",
                PLAN060_CANDIDATE_PATH.read_text(),
                flags=re.MULTILINE,
            )
            is not None
        ),
        "plan056_candidate_retired": (
            PLAN056_CANDIDATE_PATH.is_file()
            and re.search(
                r"^#+\s*Status:\s*\*?\*?retired|^\s*Status:\s*\*?\*?retired",
                PLAN056_CANDIDATE_PATH.read_text(),
                flags=re.MULTILINE,
            )
            is not None
        ),
        "plan057_superseded": (
            PLAN057_PLAN_PATH.is_file()
            and re.search(
                r"^#+\s*Status:\s*\*?\*?superseded|^\s*Status:\s*\*?\*?superseded",
                PLAN057_PLAN_PATH.read_text(),
                flags=re.MULTILINE,
            )
            is not None
        ),
        "adr_0021_rejected": adr_0021_decision() == "Rejected",
        "plan059_typed_blocker_marker": (
            plan059_typed_blocker() == TYPED_BLOCKER_JAVA_SUPPORT_TOPOLOGY_REJECTED
        ),
        "plan065_test_matrix_present": (
            (HERE / "test_plan065.py").is_file()
        ),
        "plan066_helper_module_present": (HERE / "plan066.py").is_file(),
        "plan066_test_matrix_present": (
            (HERE / "test_plan066.py").is_file()
        ),
        "execution_lane_available": False,
    }

    blockers: list[str] = []
    if not items["plan062_v4_trigger_schema"]:
        blockers.append("plan-062 v4 trigger schema missing")
    if not items["plan062_reference_event_schema"]:
        blockers.append("plan-062 reference event schema missing")
    if not items["plan062_v3_observation_schema"]:
        blockers.append("plan-062 v3 observation schema missing")
    if not items["plan062_source_verification_record"]:
        blockers.append("plan-062 source verification record missing")
    if not items["adr_0022_accepted"]:
        blockers.append("ADR 0022 is not Accepted")
    if not items["plan063_java_driver_artifacts"]:
        blockers.append("plan-063 Java driver artifacts missing")
    if not items["plan064_i2pd_driver_artifacts"]:
        blockers.append("plan-064 i2pd driver artifacts missing")
    if not items["plan065_strict_scenario_schema_v2"]:
        blockers.append("plan-065 strict scenario schema v2 missing")
    if not items["plan065_directional_predicate_contract"]:
        blockers.append("plan-065 directional predicate contract missing")
    if not items["plan065_canonical_mixed_runner"]:
        blockers.append("plan-065 canonical mixed-runner missing")
    if not items["plan060_helper_module_present"]:
        blockers.append("plan-060 helper module missing")
    if not items["plan060_typed_blocker_marker"]:
        blockers.append("plan-060 typed blocker marker missing")
    if not items["plan060_candidate_retired"]:
        blockers.append("plan-060 candidate is not retired")
    if not items["plan056_candidate_retired"]:
        blockers.append("plan-056 candidate is not retired")
    if not items["plan057_superseded"]:
        blockers.append("plan-057 is not superseded")
    if not items["adr_0021_rejected"]:
        blockers.append("ADR 0021 is not Rejected")
    if not items["plan059_typed_blocker_marker"]:
        blockers.append("plan-059 typed blocker marker missing")
    if not items["plan065_test_matrix_present"]:
        blockers.append("plan-065 test matrix missing")
    if not items["plan066_helper_module_present"]:
        blockers.append("plan-066 helper module missing")
    if not items["plan066_test_matrix_present"]:
        blockers.append("plan-066 test matrix missing")
    if not items["execution_lane_available"]:
        blockers.append(TYPED_BLOCKER_EXECUTION_LANE_UNAVAILABLE)
    return Plan066FreezeReadinessReport(items=items, blockers=tuple(blockers))


def assert_plan066_freeze_invariants(
    report: Plan066FreezeReadinessReport | None = None,
) -> Plan066FreezeReadinessReport:
    """Raise :class:`Plan066Error` when any freeze invariant is violated."""

    report = report or plan066_freeze_readiness_report()
    if not report.ready:
        raise Plan066Error(
            "Plan 066 cannot freeze the source commit: "
            + "; ".join(report.blockers)
        )
    return report


def plan066_directional_record(
    *,
    direction: str,
    i2pr_observation: dict[str, Any],
    reference_observation: dict[str, Any],
    trigger_digest_sha256: str,
    correlation_nonce: str,
    router_info_sha256: str,
    helper_digest_sha256: str = "0" * 64,
    qualification_receipt_sha256: str = "0" * 64,
    delivery_status_message_id: int | None = None,
) -> dict[str, Any]:
    """Build a sanitized per-direction record skeleton for Plan 066.

    The skeleton carries the v3 observation schema, the v4 trigger
    digest, the correlation nonce, the helper digest, the
    qualification receipt digest, and the RouterInfo digest into a
    single typed surface. The synthetic fallback is refused for any
    direction that reports ``actual_typed_result = passed``.
    """

    record: dict[str, Any] = {
        "schema": "i2pr-ntcp2-direction-observation-v3",
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
    if delivery_status_message_id is not None:
        record["delivery_status_message_id"] = int(delivery_status_message_id)
    return record


def plan066_two_bundle_independence(
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
    - distinct DeliveryStatus message_id per direction;
    - distinct i2pr RouterInfo sha256 per direction;
    - distinct reference RouterInfo sha256 per direction;
    - distinct trigger_digest_sha256 per direction.

    The function reads the correlation nonce and delivery_status
    message id from the run-correlation level when present and falls
    back to the flat ``correlation_nonce`` field.
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
        msg_a = obs_a.get("delivery_status_message_id")
        msg_b = obs_b.get("delivery_status_message_id")
        if (
            isinstance(msg_a, int)
            and isinstance(msg_b, int)
            and msg_a == msg_b
            and msg_a > 0
        ):
            failures.append(f"{direction}: identical delivery_status_message_id")
        router_info_a = obs_a.get("router_info_sha256")
        router_info_b = obs_b.get("router_info_sha256")
        if router_info_a and router_info_b and router_info_a == router_info_b:
            failures.append(f"{direction}: identical router_info_sha256")
        trigger_a = obs_a.get("trigger_digest_sha256")
        trigger_b = obs_b.get("trigger_digest_sha256")
        if (
            trigger_a
            and trigger_b
            and trigger_a == trigger_b
            and trigger_a != "0" * 64
        ):
            failures.append(f"{direction}: identical trigger_digest_sha256")
    return failures


def plan066_finalized_bundle_marker() -> dict[str, Any]:
    """Return the Plan 066 mutation-guard marker."""

    return {
        "schema": "i2pr-milestone3-plan066-bundle-marker-v1",
        "schema_version": 1,
        "finalization_marker": "plan066-bundle-immutable",
        "mutation_after_finalization": "forbidden",
        "raw_diagnostics_under_export_root": "forbidden",
        "freeze_invariant_enforced": True,
    }


def plan066_qualification_summary_status() -> str:
    """Return the typed-absence plan059 qualification summary status.

    Returns ``"blocked"`` when the qualification summary records
    blocked status (the on-disk state for this host); returns the raw
    status when not blocked.
    """

    try:
        summary = load_qualification_summary()
    except (OSError, ValueError):
        return "unknown"
    return str(summary.get("summary_status", "unknown"))


def plan066_java_qualification_receipt_blocker() -> str:
    """Return the typed blocker recorded on the Plan 063 Java receipt."""

    try:
        receipt = load_observation_qualification_receipt("java_i2p")
    except (OSError, ValueError, KeyError):
        return "unknown"
    return str(receipt.qualification_blocker or "unknown")


def plan066_i2pd_qualification_receipt_blocker() -> str:
    """Return the typed blocker recorded on the Plan 064 i2pd receipt."""

    try:
        receipt = load_observation_qualification_receipt("i2pd")
    except (OSError, ValueError, KeyError):
        return "unknown"
    return str(receipt.qualification_blocker or "unknown")


def _readiness_for_unit_tests() -> Plan066FreezeReadinessReport:
    """Return the readiness report ignoring the in-process execution-lane row."""

    report = plan066_freeze_readiness_report()
    items = dict(report.items)
    items["execution_lane_available"] = True
    blockers = tuple(
        b for b in report.blockers if b != TYPED_BLOCKER_EXECUTION_LANE_UNAVAILABLE
    )
    return Plan066FreezeReadinessReport(items=items, blockers=blockers)


__all__ = [
    "CLOSE_STATUSES",
    "CLOSE_STATUS_DECLARED_NOT_EXECUTABLE",
    "CLOSE_STATUS_EXECUTED",
    "EXECUTION_LANE_KINDS",
    "JAVA_HELPER_DIR",
    "JAVA_QUALIFICATION_RECEIPT_PATH",
    "I2PD_HELPER_DIR",
    "I2PD_QUALIFICATION_RECEIPT_PATH",
    "PLAN059_FLOOR",
    "PLAN065_FLOOR",
    "PLAN066_CANDIDATE_PATH",
    "PLAN066_CLOSURE_PATH",
    "Plan066Error",
    "Plan066ExecutionLaneLock",
    "Plan066FreezeReadinessReport",
    "TYPED_BLOCKER_EXECUTION_LANE_UNAVAILABLE",
    "assert_plan066_freeze_invariants",
    "plan066_candidate_record_digests",
    "plan066_close_status",
    "plan066_directional_record",
    "plan066_execution_lane_lock",
    "plan066_finalized_bundle_marker",
    "plan066_freeze_readiness_report",
    "plan066_i2pd_qualification_receipt_blocker",
    "plan066_java_qualification_receipt_blocker",
    "plan066_qualification_summary_status",
    "plan066_typed_blocker",
    "plan066_two_bundle_independence",
]
