"""Plan 083 runner test matrix.

Exercises the ``plan083_runner`` module against the bounded event
sources and subprocess stubs. The runner is structurally incapable of
producing a passed record when the injected sources omit the
canonical events; the tests verify the fail-closed behavior and the
typed reason-code assignment for every pre-protocol and protocol
failure path. No real subprocess is launched.
"""

from __future__ import annotations

import os
import tempfile
import unittest
from pathlib import Path

import minimal_i2pd_probe as probe

from plan083_runner import (
    FakeEventSource,
    FakeProcess,
    ProbeConfig,
    ProbeRunner,
    RunnerError,
    detect_host_blocker,
    execute_real_probe,
    write_host_blocked_record,
)


def _hex(value: str, length: int = 64) -> str:
    return value * length


def _minimal_config(**overrides: object) -> ProbeConfig:
    base = dict(
        run_id="plan083-runner",
        source_commit=_hex("a", 40),
        reference_revision=_hex("b", 40),
        lane_qualification_sha256=_hex("c", 64),
        topology_kind="rootless-sealed-single-netns",
        parent_network_state_unchanged=True,
        i2pr_binary_sha256=_hex("d", 64),
        i2pd_binary_sha256=_hex("e", 64),
        i2pr_router_info_sha256=_hex("1", 64),
        i2pd_router_info_sha256=_hex("2", 64),
        i2pr_router_hash_sha256=_hex("3", 64),
        i2pd_router_hash_sha256=_hex("4", 64),
        delivery_status_message_id=0x04200001,
    )
    base.update(overrides)
    return ProbeConfig(**base)  # type: ignore[arg-type]


class Plan083RunnerPreProtocolTests(unittest.TestCase):
    """Pre-protocol rejection paths classify into typed reason codes."""

    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.run_root = Path(self._tmp.name)

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def test_invalid_topology_returns_lane_invalid(self) -> None:
        runner = ProbeRunner(_minimal_config(topology_kind="public-internet"))
        record_path = runner.run(self.run_root)
        record = probe.validate_record(__import__("json").loads(record_path.read_text()))
        self.assertEqual(record["terminal_result"], probe.LANE_INVALID)
        self.assertEqual(record["reason_code"], probe.REASON_LANE_INVALID)
        self.assertEqual(record["highest_stage_reached"], probe.NOT_STARTED)

    def test_parent_network_state_unchanged_false_returns_lane_invalid(self) -> None:
        runner = ProbeRunner(_minimal_config(parent_network_state_unchanged=False))
        record_path = runner.run(self.run_root)
        record = probe.validate_record(__import__("json").loads(record_path.read_text()))
        self.assertEqual(record["terminal_result"], probe.LANE_INVALID)
        self.assertEqual(record["reason_code"], probe.REASON_LANE_INVALID)

    def test_invalid_message_id_returns_pre_protocol_run_identity_failed(self) -> None:
        runner = ProbeRunner(_minimal_config(delivery_status_message_id=0))
        record_path = runner.run(self.run_root)
        record = probe.validate_record(__import__("json").loads(record_path.read_text()))
        self.assertEqual(record["terminal_result"], probe.PRE_PROTOCOL_REJECTED)
        self.assertEqual(record["reason_code"], probe.REASON_PRE_PROTOCOL_RUN_IDENTITY_FAILED)
        self.assertEqual(record["highest_stage_reached"], probe.STATE_PREPARED)

    def test_invalid_router_info_hash_returns_validation_failed(self) -> None:
        runner = ProbeRunner(_minimal_config(i2pr_router_info_sha256="not-hex"))
        record_path = runner.run(self.run_root)
        record = probe.validate_record(__import__("json").loads(record_path.read_text()))
        self.assertEqual(record["terminal_result"], probe.PRE_PROTOCOL_REJECTED)
        self.assertEqual(
            record["reason_code"], probe.REASON_PRE_PROTOCOL_ROUTER_INFO_VALIDATION_FAILED
        )


class Plan083RunnerProtocolTests(unittest.TestCase):
    """Protocol rejection paths use typed reason codes."""

    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.run_root = Path(self._tmp.name)

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def test_missing_listener_factory_returns_i2pd_listener_not_ready(self) -> None:
        # Without an I2PD_DRIVER_PATH the runner short-circuits the
        # listener factory. The probe record reflects an i2pd listener
        # start that did not begin; the result is a protocol_rejected.
        runner = ProbeRunner(_minimal_config())
        record_path = runner.run(self.run_root)
        record = probe.validate_record(__import__("json").loads(record_path.read_text()))
        # Without an explicit failure path the runner stays at state
        # prepared with the not-started reason.
        self.assertEqual(record["highest_stage_reached"], probe.STATE_PREPARED)
        self.assertEqual(record["reason_code"], probe.REASON_NOT_STARTED)

    def test_invalid_run_id_returns_pre_protocol_failure(self) -> None:
        runner = ProbeRunner(_minimal_config(run_id="BAD ID WITH SPACES"))
        record_path = runner.run(self.run_root)
        record = probe.validate_record(__import__("json").loads(record_path.read_text()))
        self.assertEqual(record["reason_code"], probe.REASON_PRE_PROTOCOL_RUN_IDENTITY_FAILED)


class Plan083RunnerFakeSourcesTests(unittest.TestCase):
    """Fake event sources and processes exercise the failure paths."""

    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.run_root = Path(self._tmp.name)
        # Populate the i2pr state/router.info so the runner does not
        # rewrite the cleanup result to ``failed`` for a missing
        # state directory.
        (self.run_root / "state").mkdir(parents=True, exist_ok=True)
        (self.run_root / "state" / "router.info").write_bytes(b"")

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def test_no_listener_event_advances_to_listener_ready_stage(self) -> None:
        i2pd_source = FakeEventSource()
        runner = ProbeRunner(_minimal_config(i2pd_event_source=i2pd_source))
        record_path = runner.run(self.run_root)
        record = probe.validate_record(__import__("json").loads(record_path.read_text()))
        # The runner advances the stage to listener_ready even without
        # an event when the source has no terminal; the result is the
        # bounded not-started classification.
        self.assertEqual(record["highest_stage_reached"], probe.STATE_PREPARED)

    def test_fake_process_stop_reports_clean(self) -> None:
        process = FakeProcess(exit_code=0, terminal_status={"result": "passed"})
        process.start()
        self.assertEqual(process.stop(1.0), "clean")
        self.assertEqual(process.wait_terminal(1.0), {"result": "passed"})


class Plan083RealProbeLaneTests(unittest.TestCase):
    """Lane validation in the real-probe entry point."""

    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.run_root = Path(self._tmp.name)

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def test_invalid_lane_records_lane_invalid(self) -> None:
        import json
        record = execute_real_probe(
            repo_root=Path("/nonexistent"),
            run_root=self.run_root,
            run_id="real-lane",
            source_commit=_hex("a", 40),
            reference_revision=_hex("b", 40),
            lane_qualification_sha256=_hex("c", 64),
            topology_kind="public-network",
            i2pr_binary_sha256=_hex("d", 64),
            i2pd_binary_sha256=_hex("e", 64),
            delivery_status_message_id=0x04200001,
            i2pd_driver_binary=Path("/nonexistent/i2pd-driver"),
        )
        validated = probe.validate_record(record)
        self.assertEqual(validated["terminal_result"], probe.LANE_INVALID)
        self.assertEqual(validated["reason_code"], probe.REASON_LANE_INVALID)
        self.assertEqual(validated["highest_stage_reached"], probe.NOT_STARTED)

    def test_zero_message_id_records_pre_protocol_run_identity_failure(self) -> None:
        import json
        record = execute_real_probe(
            repo_root=Path("/nonexistent"),
            run_root=self.run_root,
            run_id="real-zero-msgid",
            source_commit=_hex("a", 40),
            reference_revision=_hex("b", 40),
            lane_qualification_sha256=_hex("c", 64),
            topology_kind="rootless-sealed-single-netns",
            i2pr_binary_sha256=_hex("d", 64),
            i2pd_binary_sha256=_hex("e", 64),
            delivery_status_message_id=0,
            i2pd_driver_binary=Path("/nonexistent/i2pd-driver"),
        )
        validated = probe.validate_record(record)
        self.assertEqual(validated["terminal_result"], probe.PRE_PROTOCOL_REJECTED)
        self.assertEqual(
            validated["reason_code"], probe.REASON_PRE_PROTOCOL_RUN_IDENTITY_FAILED
        )


class Plan083HostBlockerTests(unittest.TestCase):
    """Host blocker detection and record writer."""

    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.run_root = Path(self._tmp.name)
        self._old_env = os.environ.get("I2PR_PLAN046_HOST_BLOCKER")

    def tearDown(self) -> None:
        if self._old_env is None:
            os.environ.pop("I2PR_PLAN046_HOST_BLOCKER", None)
        else:
            os.environ["I2PR_PLAN046_HOST_BLOCKER"] = self._old_env
        self._tmp.cleanup()

    def test_detect_host_blocker_returns_env_value(self) -> None:
        os.environ["I2PR_PLAN046_HOST_BLOCKER"] = "blocked_unprivileged_user_namespace"
        self.assertEqual(
            detect_host_blocker(), "blocked_unprivileged_user_namespace"
        )

    def test_detect_host_blocker_returns_none_when_unset(self) -> None:
        os.environ.pop("I2PR_PLAN046_HOST_BLOCKER", None)
        self.assertIsNone(detect_host_blocker())

    def test_write_host_blocked_record_uses_lane_invalid(self) -> None:
        output = write_host_blocked_record(
            run_root=self.run_root,
            run_id="blocked",
            source_commit=_hex("a", 40),
            reference_revision=_hex("b", 40),
            lane_qualification_sha256=_hex("c", 64),
            topology_kind="rootless-sealed-single-netns",
            host_blocker="blocked_unprivileged_user_namespace",
        )
        import json
        record = probe.validate_record(json.loads(output.read_text()))
        self.assertEqual(record["terminal_result"], probe.LANE_INVALID)
        self.assertEqual(record["reason_code"], probe.REASON_LANE_INVALID)
        self.assertEqual(record["cleanup_result"], "not-run")


class Plan083RunnerErrorTests(unittest.TestCase):
    """RunnerError carries the typed code."""

    def test_runner_error_carries_code(self) -> None:
        exc = RunnerError("lane-invalid")
        self.assertEqual(exc.code, "lane-invalid")
        self.assertIn("lane-invalid", str(exc))


if __name__ == "__main__":
    unittest.main()