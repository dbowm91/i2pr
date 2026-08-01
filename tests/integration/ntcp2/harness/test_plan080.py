"""Plan 080 stale-instance inspection and lane-qualification regression tests.

The Plan 080 plan-of-record enumerates the required test matrix. The
minimum cases cover:

1. schema and close-status constants exist with expected values;
2. typed blocker is exactly ``blocked_execution_lane_unavailable``;
3. guest inspect record minimal unowned fixture;
4. guest inspect record rejects unknown fields;
5. guest inspect record rejects empty run_id;
6. qualification writer full-runtime qualified fixture;
7. qualification writer rejects qualified with reduced scope;
8. qualification writer rejects qualified with unavailable full-runtime lane;
9. qualification writer artifact digests measured;
10. qualification writer rejects invalid digest;
11. close-status default returns ``lane-blocked``;
12. plan080 import does not modify execution_lane.
"""

from __future__ import annotations

import hashlib
import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import execution_lane
import plan080
from plan080 import (
    CLOSE_STATUS_BLOCKED,
    CLOSE_STATUS_IN_PROGRESS,
    CLOSE_STATUS_QUALIFIED,
    INSPECT_SCHEMA,
    Plan080Error,
    SCHEMA,
    SCHEMA_VERSION,
    TYPED_BLOCKER,
    plan080_close_status,
    plan080_guest_inspect_record,
    plan080_lane_qualification_digest,
    plan080_qualification_writer,
    plan080_typed_blocker,
)


def _minimal_probe() -> dict[str, object]:
    return {
        "selected_lane": execution_lane.LANE_REMOTE,
        "host_architecture": "x86_64",
        "docker_cli_present": False,
        "docker_daemon_accessible": False,
        "qemu_system_present": False,
        "qemu_tcg_usable": False,
        "remote_workflow_present": True,
        "reason_codes": ["full-runtime-lane-unavailable"],
    }


class Plan080SchemaTests(unittest.TestCase):
    """Constants and schema markers."""

    def test_schema_and_close_status_constants(self) -> None:
        self.assertEqual(SCHEMA, "i2pr-plan080-closure-v1")
        self.assertEqual(SCHEMA_VERSION, 1)
        self.assertEqual(CLOSE_STATUS_IN_PROGRESS, "in-progress")
        self.assertEqual(CLOSE_STATUS_QUALIFIED, "lane-qualified")
        self.assertEqual(CLOSE_STATUS_BLOCKED, "lane-blocked")
        self.assertIn(CLOSE_STATUS_BLOCKED, plan080.ALLOWED_CLOSE_STATUSES)

    def test_typed_blocker_is_blocked_execution_lane_unavailable(self) -> None:
        self.assertEqual(TYPED_BLOCKER, "blocked_execution_lane_unavailable")
        self.assertEqual(plan080_typed_blocker(), TYPED_BLOCKER)


class Plan080GuestInspectRecordTests(unittest.TestCase):
    """Guest inspect record construction and validation."""

    def test_guest_inspect_record_minimal_unowned(self) -> None:
        record = plan080_guest_inspect_record(
            run_id="plan080-run-01",
            instance_name="i2pr-interop-run-01",
            multipass_list_entry={"ipv4": "10.0.0.2", "state": "Running"},
            host_lifecycle_record_present=False,
            ownership_contract_present=False,
            remediation_class="selective-purge",
            notes="unowned stale instance",
        )
        self.assertIsInstance(record, dict)
        self.assertEqual(record["schema"], INSPECT_SCHEMA)
        self.assertEqual(record["schema_version"], 1)
        self.assertEqual(record["run_id"], "plan080-run-01")
        self.assertEqual(record["instance_name"], "i2pr-interop-run-01")
        self.assertFalse(record["host_lifecycle_record_present"])
        self.assertFalse(record["ownership_contract_present"])
        self.assertEqual(record["remediation_class"], "selective-purge")
        self.assertIn("recorded_utc", record)
        self.assertTrue(record["recorded_utc"].endswith("Z"))
        self.assertEqual(
            len(record["record_sha256"]), 64,
        )

    def test_guest_inspect_record_rejects_unknown_field(self) -> None:
        with self.assertRaisesRegex(Plan080Error, "unknown|unexpected|forbidden|field"):
            plan080_guest_inspect_record(
                run_id="plan080-run-02",
                instance_name="i2pr-interop-run-02",
                multipass_list_entry={},
                host_lifecycle_record_present=False,
                ownership_contract_present=False,
                remediation_class="selective-purge",
                notes="",
                **{"not_a_field": "x"},  # type: ignore[arg-type]
            )

    def test_guest_inspect_record_rejects_empty_run_id(self) -> None:
        with self.assertRaisesRegex(Plan080Error, "run_id"):
            plan080_guest_inspect_record(
                run_id="",
                instance_name="i2pr-interop-run-03",
                multipass_list_entry={},
                host_lifecycle_record_present=False,
                ownership_contract_present=False,
                remediation_class="selective-purge",
                notes="",
            )


class Plan080QualificationWriterTests(unittest.TestCase):
    """Qualification writer construction and validation."""

    def test_qualification_writer_full_runtime_qualified(self) -> None:
        probe = _minimal_probe()
        record = plan080_qualification_writer(probe, qualified=True)
        self.assertEqual(record["schema"], execution_lane.QUALIFICATION_SCHEMA)
        self.assertTrue(record["qualified"])
        self.assertEqual(record["scope"], "full-runtime")
        self.assertEqual(record["full_runtime_lane"], "available")
        self.assertEqual(record["selected_lane"], execution_lane.LANE_REMOTE)
        expected_digest = execution_lane.canonical_digest(record)
        self.assertEqual(record["record_sha256"], expected_digest)
        execution_lane.validate_qualification_record(record)

    def test_qualification_writer_rejects_qualified_with_reduced_scope(self) -> None:
        probe = _minimal_probe()
        with self.assertRaisesRegex(Plan080Error, "full-runtime"):
            plan080_qualification_writer(
                probe,
                qualified=True,
                scope="reduced-scope-diagnostic",
            )

    def test_qualification_writer_rejects_qualified_with_unavailable_full_runtime(self) -> None:
        probe = _minimal_probe()
        with self.assertRaisesRegex(Plan080Error, "full-runtime lane"):
            plan080_qualification_writer(
                probe,
                qualified=True,
                full_runtime_lane="unavailable",
            )

    def test_qualification_writer_artifact_digests_measured(self) -> None:
        probe = _minimal_probe()
        artifact_digests = {
            "i2pd_ntcp2_interop_driver_instrumented": "0" * 64,
            "i2pd_ntcp2_interop_driver_control": "1" * 64,
            "host_i2pd_cache_sha256": "2" * 64,
        }
        record = plan080_qualification_writer(
            probe,
            qualified=False,
            artifact_digests=artifact_digests,
        )
        self.assertEqual(
            record["artifact_digests"]["i2pd_ntcp2_interop_driver_instrumented"],
            "0" * 64,
        )
        self.assertEqual(
            record["artifact_digests"]["i2pd_ntcp2_interop_driver_control"],
            "1" * 64,
        )
        self.assertEqual(
            record["artifact_digests"]["host_i2pd_cache_sha256"],
            "2" * 64,
        )

    def test_qualification_writer_rejects_invalid_digest(self) -> None:
        probe = _minimal_probe()
        with self.assertRaisesRegex(Plan080Error, "64 lowercase hex"):
            plan080_qualification_writer(
                probe,
                qualified=False,
                artifact_digests={"bad": "not-a-digest"},
            )


class Plan080ClosureStatusTests(unittest.TestCase):
    """Close-status classifier."""

    def test_close_status_default(self) -> None:
        self.assertEqual(plan080_close_status(), "lane-blocked")


class Plan080RejectUnknownFieldTests(unittest.TestCase):
    """Import hygiene: plan080 must not modify execution_lane."""

    def test_plan080_import_does_not_modify_execution_lane(self) -> None:
        self.assertTrue(hasattr(execution_lane, "validate_qualification_record"))
        self.assertTrue(hasattr(execution_lane, "canonical_digest"))
        # Verify signatures still accept the same arguments.
        import inspect
        sig_validate = inspect.signature(execution_lane.validate_qualification_record)
        self.assertEqual(len(sig_validate.parameters), 1)
        sig_digest = inspect.signature(execution_lane.canonical_digest)
        self.assertIn("record", sig_digest.parameters)


class Plan080LaneQualificationDigestTests(unittest.TestCase):
    """Lane qualification digest validation."""

    def test_digest_matches_canonical(self) -> None:
        probe = _minimal_probe()
        record = plan080_qualification_writer(probe, qualified=True)
        digest = plan080_lane_qualification_digest(record)
        self.assertEqual(digest, record["record_sha256"])

    def test_digest_rejects_mismatched_record(self) -> None:
        probe = _minimal_probe()
        record = plan080_qualification_writer(probe, qualified=True)
        record["recorded_utc"] = "2099-12-31T23:59:59Z"
        with self.assertRaises(Plan080Error):
            plan080_lane_qualification_digest(record)


if __name__ == "__main__":
    unittest.main()
