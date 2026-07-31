"""Plan 077 constrained-host lane contract and probe tests."""

from __future__ import annotations

import copy
import json
import sys
import unittest
from pathlib import Path
from unittest import mock

if __package__ in {None, ""}:
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import execution_lane


def _capabilities(**overrides: bool) -> dict[str, bool]:
    value = {
        "docker_daemon_accessible": False,
        "qemu_tcg_usable": False,
        "seccomp_no_new_privs_supported": False,
        "remote_workflow_present": False,
    }
    value.update(overrides)
    return value


def _manifest() -> dict[str, object]:
    return {
        "schema": execution_lane.MANIFEST_SCHEMA,
        "schema_version": 1,
        "source_commit": "a" * 40,
        "reference_revision": "b" * 40,
        "i2pr_binary_sha256": "c" * 64,
        "i2pd_binary_sha256": "d" * 64,
        "reference_build_manifest_sha256": "e" * 64,
        "direction": "i2pr-to-i2pd-ipv4",
        "run_id": "run-07701",
        "result_output": "results/direction.json",
        "execution_timeout_seconds": 120,
    }


class LaneSelectionTests(unittest.TestCase):
    def test_required_order_prefers_docker(self) -> None:
        self.assertEqual(
            execution_lane.select_lane(_capabilities(
                docker_daemon_accessible=True,
                qemu_tcg_usable=True,
                seccomp_no_new_privs_supported=True,
                remote_workflow_present=True,
            )),
            execution_lane.LANE_DOCKER,
        )

    def test_qemu_follows_docker(self) -> None:
        self.assertEqual(
            execution_lane.select_lane(_capabilities(qemu_tcg_usable=True)),
            execution_lane.LANE_QEMU,
        )

    def test_reduced_scope_precedes_remote(self) -> None:
        self.assertEqual(
            execution_lane.select_lane(_capabilities(
                seccomp_no_new_privs_supported=True,
                remote_workflow_present=True,
            )),
            execution_lane.LANE_INHERITED,
        )

    def test_remote_is_selected_only_without_local_capability(self) -> None:
        self.assertEqual(
            execution_lane.select_lane(_capabilities(remote_workflow_present=True)),
            execution_lane.LANE_REMOTE,
        )

    def test_no_capability_is_no_lane(self) -> None:
        self.assertEqual(execution_lane.select_lane(_capabilities()), execution_lane.LANE_NONE)

    def test_unknown_capability_is_rejected(self) -> None:
        with self.assertRaisesRegex(execution_lane.ExecutionLaneError, "must be boolean"):
            execution_lane.select_lane({"docker_daemon_accessible": None})  # type: ignore[arg-type]


class ManifestTests(unittest.TestCase):
    def test_manifest_round_trips(self) -> None:
        value = _manifest()
        self.assertEqual(execution_lane.validate_execution_manifest(value), value)

    def test_unknown_field_rejected(self) -> None:
        value = _manifest()
        value["secret"] = "not allowed"
        with self.assertRaisesRegex(execution_lane.ExecutionLaneError, "fields are not exact"):
            execution_lane.validate_execution_manifest(value)

    def test_absolute_result_path_rejected(self) -> None:
        value = _manifest()
        value["result_output"] = "/tmp/result.json"
        with self.assertRaisesRegex(execution_lane.ExecutionLaneError, "relative bounded path"):
            execution_lane.validate_execution_manifest(value)

    def test_short_digest_rejected(self) -> None:
        value = _manifest()
        value["i2pd_binary_sha256"] = "0" * 63
        with self.assertRaisesRegex(execution_lane.ExecutionLaneError, "i2pd_binary_sha256 invalid"):
            execution_lane.validate_execution_manifest(value)

    def test_bool_timeout_rejected(self) -> None:
        value = _manifest()
        value["execution_timeout_seconds"] = True
        with self.assertRaisesRegex(execution_lane.ExecutionLaneError, "out of range"):
            execution_lane.validate_execution_manifest(value)


class QualificationTests(unittest.TestCase):
    def test_no_lane_record_is_not_qualified(self) -> None:
        probe = {
            "selected_lane": execution_lane.LANE_INHERITED,
            "host_architecture": "x86_64",
            "docker_cli_present": True,
            "docker_daemon_accessible": False,
            "qemu_system_present": False,
            "qemu_tcg_usable": False,
            "remote_workflow_present": True,
            "reason_codes": ["docker-daemon-inaccessible"],
        }
        record = execution_lane.no_lane_qualification(probe)
        self.assertFalse(record["qualified"])
        self.assertEqual(record["full_runtime_lane"], "unavailable")
        self.assertEqual(record["reduced_scope_lane"], "available")
        self.assertEqual(execution_lane.validate_qualification_record(record), record)

    def test_qualification_cannot_be_reduced_scope(self) -> None:
        probe = {
            "selected_lane": execution_lane.LANE_INHERITED,
            "host_architecture": "x86_64",
            "docker_cli_present": False,
            "docker_daemon_accessible": False,
            "qemu_system_present": False,
            "qemu_tcg_usable": False,
            "remote_workflow_present": False,
            "reason_codes": [],
        }
        record = execution_lane.no_lane_qualification(probe)
        invalid = copy.deepcopy(record)
        invalid["qualified"] = True
        invalid["record_sha256"] = execution_lane.canonical_digest(invalid)
        with self.assertRaisesRegex(execution_lane.ExecutionLaneError, "only a full-runtime"):
            execution_lane.validate_qualification_record(invalid)

    def test_digest_mutation_is_rejected(self) -> None:
        probe = {
            "selected_lane": execution_lane.LANE_NONE,
            "host_architecture": "x86_64",
            "docker_cli_present": False,
            "docker_daemon_accessible": False,
            "qemu_system_present": False,
            "qemu_tcg_usable": False,
            "remote_workflow_present": False,
            "reason_codes": ["full-runtime-lane-unavailable"],
        }
        record = execution_lane.no_lane_qualification(probe)
        record["reason_code"] = "changed-reason"
        with self.assertRaisesRegex(execution_lane.ExecutionLaneError, "record_sha256 mismatch"):
            execution_lane.validate_qualification_record(record)


class ProbeTests(unittest.TestCase):
    @mock.patch.object(execution_lane, "_probe_no_new_privs", return_value=(False, "unsupported"))
    @mock.patch.object(execution_lane, "_probe_qemu", return_value=(False, False, "absent"))
    @mock.patch.object(execution_lane, "_probe_docker", return_value=(True, False, "inaccessible"))
    def test_probe_records_remote_workflow_without_mutation(
        self,
        _docker: mock.Mock,
        _qemu: mock.Mock,
        _nnp: mock.Mock,
    ) -> None:
        with mock.patch.object(execution_lane, "_remote_workflow_present", return_value=True):
            result = execution_lane.probe_environment(Path("."))
        self.assertEqual(result["selected_lane"], execution_lane.LANE_REMOTE)
        self.assertTrue(result["remote_workflow_present"])
        self.assertIn("remote-workflow-present", result["reason_codes"])

    @mock.patch.object(execution_lane, "_probe_no_new_privs", return_value=(True, "supported"))
    @mock.patch.object(execution_lane, "_probe_qemu", return_value=(False, False, "absent"))
    @mock.patch.object(execution_lane, "_probe_docker", return_value=(True, False, "inaccessible"))
    def test_probe_selects_reduced_scope_when_local_full_lanes_are_unavailable(
        self,
        _docker: mock.Mock,
        _qemu: mock.Mock,
        _nnp: mock.Mock,
    ) -> None:
        with mock.patch.object(execution_lane, "_remote_workflow_present", return_value=True):
            result = execution_lane.probe_environment(Path("."))
        self.assertEqual(result["selected_lane"], execution_lane.LANE_INHERITED)
        self.assertIn("remote-workflow-present-not-selected-after-reduced-scope", result["reason_codes"])


class ArtifactAndStaticTests(unittest.TestCase):
    def test_source_contains_no_privilege_escalation_or_network_mutation(self) -> None:
        source = (Path(__file__).resolve().parents[4] / "scripts/interop/probe-constrained-host-lanes.sh").read_text()
        self.assertNotIn("sudo", source)
        self.assertNotIn("ip netns", source)
        self.assertNotIn("--privileged", source)
        self.assertNotIn("--network host", source)

    def test_probe_output_is_json(self) -> None:
        sample = {"schema": execution_lane.PROBE_SCHEMA, "selected_lane": execution_lane.LANE_NONE}
        self.assertEqual(json.loads(json.dumps(sample))["schema"], execution_lane.PROBE_SCHEMA)


if __name__ == "__main__":
    unittest.main()
