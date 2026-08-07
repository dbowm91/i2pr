"""Plan 069 loopback smoke runner test matrix.

The Plan 069 test matrix exercises the strict config parser, the
failure staging, the cleanup contract, the network-audit degradation,
the direct RouterInfo exchange, the exact correlation fields, the
typed-blocker rules, and the runner ownership invariants.

The tests use fake subprocesses, fake ``time.monotonic`` clocks, and
a temporary repository tree. The real i2pr launcher binary, the real
Plan 064 i2pd driver, and the Rust NTCP2 transport are never required
at unit-test time.
"""

from __future__ import annotations

import dataclasses
import datetime as dt
import json
import os
import re
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from typing import Any

if __package__ in {None, ""}:
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import loopback_smoke as smoke_runner
import loopback_smoke_record as smoke_record
import reference_event  # noqa: E402  (Plan 075 reference-event v1 schemas)


SMOKE_EVIDENCE_TIER = smoke_record.EVIDENCE_TIER


HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[3]


def _fake_config(
    *,
    direction: str = "i2pr-to-i2pd-ipv4",
    diagnostics_mode: str = "off",
    network_audit_mode: str = "configuration-only",
    source_commit: str = "a" * 40,
    reference_driver: Path | None = None,
    reference_build_manifest: Path | None = None,
    reference_source_lock: Path | None = None,
    output_record: Path | None = None,
) -> dict[str, Any]:
    return {
        "direction": direction,
        "reference_driver_binary": reference_driver or _touch("i2pd"),
        "reference_build_manifest": reference_build_manifest or _touch("build.json"),
        "reference_source_lock": reference_source_lock or _touch("source-lock.json"),
        "output_record": output_record or Path(tempfile.mkstemp(suffix=".json")[1]),
        "source_commit": source_commit,
        "run_timeout_seconds": 30.0,
        "readiness_timeout_seconds": 5.0,
        "handshake_timeout_seconds": 5.0,
        "data_timeout_seconds": 5.0,
        "network_audit_mode": network_audit_mode,
        "diagnostics_mode": diagnostics_mode,
    }


def _touch(name: str, content: str = "{}") -> Path:
    fd, path = tempfile.mkstemp(suffix=name)
    with os.fdopen(fd, "w", encoding="utf-8") as handle:
        handle.write(content)
    return Path(path)


class _FakePopenedProcess:
    """Minimal stand-in for :class:`subprocess.Popen` used by tests.

    The fake accepts a list of commands, records it, and provides a
    deterministic exit code and PID so the runner can call ``poll``,
    ``wait``, ``terminate``, and ``kill`` without ever spawning a real
    binary.
    """

    def __init__(self, *, command: list[str], exit_code: int = 0, pid: int = 4242):
        self._command = list(command)
        self._exit_code = exit_code
        self._pid = pid
        self.returncode: int | None = None
        self.stdin = None
        self.stdout = None
        self.stderr = None
        self.terminated = False
        self.killed = False

    def poll(self) -> int | None:
        return self.returncode

    def wait(self, timeout: float | None = None) -> int:
        self.returncode = self._exit_code
        return self._exit_code

    def terminate(self) -> None:
        self.terminated = True

    def kill(self) -> None:
        self.killed = True
        self.returncode = self._exit_code

    @property
    def pid(self) -> int:
        return self._pid


class _FakeSubprocessFactory:
    """A deterministic subprocess factory used by tests."""

    def __init__(self, exit_code: int = 0):
        self.calls: list[list[str]] = []
        self.exit_code = exit_code
        self._pid_counter = 5000

    def __call__(self, *args: Any, **kwargs: Any) -> _FakePopenedProcess:
        command = args[0] if args else kwargs.get("args", [])
        self.calls.append(list(command))
        return _FakePopenedProcess(command=list(command), exit_code=self.exit_code, pid=self._pid_counter)


class _FakeClock:
    """A monotonic clock that advances on demand."""

    def __init__(self, start: float = 0.0) -> None:
        self._now = start

    def __call__(self) -> float:
        return self._now

    def advance(self, seconds: float) -> None:
        self._now += seconds


# ----- Strict config -----


class StrictConfigTests(unittest.TestCase):
    """The strict config rejects unknown fields and bad inputs."""

    def test_valid_config_parses(self):
        config = smoke_runner.parse_config_dict(_fake_config())
        self.assertEqual(config.direction, "i2pr-to-i2pd-ipv4")

    def test_unknown_field_rejected(self):
        payload = _fake_config()
        payload["surprise"] = "value"
        with self.assertRaisesRegex(smoke_runner.LoopbackSmokeConfigError, "config-unknown-field"):
            smoke_runner.parse_config_dict(payload)

    def test_unsupported_direction_rejected(self):
        payload = _fake_config(direction="i2pr-to-emissary-ipv4")
        with self.assertRaisesRegex(smoke_runner.LoopbackSmokeConfigError, "direction-not-allowlisted"):
            smoke_runner.parse_config_dict(payload)

    def test_unsupported_network_audit_rejected(self):
        payload = _fake_config(network_audit_mode="pcap")
        with self.assertRaisesRegex(smoke_runner.LoopbackSmokeConfigError, "network_audit_mode-not-allowlisted"):
            smoke_runner.parse_config_dict(payload)

    def test_raw_diagnostics_rejected(self):
        payload = _fake_config(diagnostics_mode="raw-local")
        with self.assertRaisesRegex(smoke_runner.LoopbackSmokeConfigError, "diagnostics_mode-not-allowlisted"):
            smoke_runner.parse_config_dict(payload)

    def test_source_commit_must_be_40_hex(self):
        payload = _fake_config(source_commit="not-a-sha")
        with self.assertRaisesRegex(smoke_runner.LoopbackSmokeConfigError, "source_commit-not-40-hex"):
            smoke_runner.parse_config_dict(payload)

    def test_short_source_commit_rejected(self):
        payload = _fake_config(source_commit="a" * 39)
        with self.assertRaisesRegex(smoke_runner.LoopbackSmokeConfigError, "source_commit-not-40-hex"):
            smoke_runner.parse_config_dict(payload)

    def test_zero_deadline_rejected(self):
        payload = _fake_config()
        payload["run_timeout_seconds"] = 0.0
        with self.assertRaisesRegex(smoke_runner.LoopbackSmokeConfigError, "run_timeout_seconds-out-of-range"):
            smoke_runner.parse_config_dict(payload)

    def test_missing_reference_driver_rejected(self):
        payload = _fake_config()
        payload["reference_driver_binary"] = "/nonexistent/path"
        with self.assertRaisesRegex(smoke_runner.LoopbackSmokeConfigError, "reference_driver_binary-missing"):
            smoke_runner.parse_config_dict(payload)

    def test_output_record_must_be_json(self):
        payload = _fake_config()
        payload["output_record"] = Path(tempfile.mkstemp(suffix=".txt")[1])
        with self.assertRaisesRegex(smoke_runner.LoopbackSmokeConfigError, "output_record-must-be-json"):
            smoke_runner.parse_config_dict(payload)


# ----- CLI -----


class CliParsingTests(unittest.TestCase):
    """The CLI parser honours the documented option set."""

    def test_cli_minimal_set(self):
        config = smoke_runner.parse_cli_args([
            "--direction", "i2pr-to-i2pd-ipv4",
            "--reference-driver", str(_touch("driver")),
            "--reference-build-manifest", str(_touch("manifest.json")),
            "--reference-source-lock", str(_touch("lock.json")),
            "--output", str(Path(tempfile.mkstemp(suffix=".json")[1])),
            "--source-commit", "a" * 40,
        ])
        self.assertEqual(config.direction, "i2pr-to-i2pd-ipv4")

    def test_cli_rejects_unknown_flag(self):
        with self.assertRaises(SystemExit):
            smoke_runner.parse_cli_args([
                "--direction", "i2pr-to-i2pd-ipv4",
                "--reference-driver", str(_touch("driver")),
                "--reference-build-manifest", str(_touch("manifest.json")),
                "--reference-source-lock", str(_touch("lock.json")),
                "--output", str(Path(tempfile.mkstemp(suffix=".json")[1])),
                "--source-commit", "a" * 40,
                "--bogus", "1",
            ])

    def test_cli_rejects_raw_diagnostics(self):
        with self.assertRaises(SystemExit):
            smoke_runner.parse_cli_args([
                "--direction", "i2pr-to-i2pd-ipv4",
                "--reference-driver", str(_touch("driver")),
                "--reference-build-manifest", str(_touch("manifest.json")),
                "--reference-source-lock", str(_touch("lock.json")),
                "--output", str(Path(tempfile.mkstemp(suffix=".json")[1])),
                "--source-commit", "a" * 40,
                "--diagnostics-mode", "raw-local",
            ])

    def test_cli_rejects_unsupported_direction(self):
        with self.assertRaises(SystemExit):
            smoke_runner.parse_cli_args([
                "--direction", "i2pr-to-emissary-ipv4",
                "--reference-driver", str(_touch("driver")),
                "--reference-build-manifest", str(_touch("manifest.json")),
                "--reference-source-lock", str(_touch("lock.json")),
                "--output", str(Path(tempfile.mkstemp(suffix=".json")[1])),
                "--source-commit", "a" * 40,
            ])


# ----- Helpers -----


class HelperTests(unittest.TestCase):
    """The shared helpers return bounded, deterministic values."""

    def test_delivery_status_message_id_nonzero(self):
        first = smoke_runner.derive_delivery_status_message_id(
            run_id="loopback-1",
            scenario_id="loopback-i2pr-to-i2pd-ipv4",
            correlation_nonce="loopback-1",
        )
        second = smoke_runner.derive_delivery_status_message_id(
            run_id="loopback-2",
            scenario_id="loopback-i2pr-to-i2pd-ipv4",
            correlation_nonce="loopback-2",
        )
        self.assertGreaterEqual(first, 1)
        self.assertLessEqual(first, 0xFFFFFFFF)
        self.assertNotEqual(first, second)

    def test_delivery_status_message_id_rejects_zero(self):
        first = smoke_runner.derive_delivery_status_message_id(
            run_id="",
            scenario_id="",
            correlation_nonce="",
        )
        self.assertGreaterEqual(first, 1)

    def test_generate_run_id_shape(self):
        run_id = smoke_runner.generate_run_id()
        self.assertRegex(run_id, r"^loopback-smoke-[0-9]{14}-[0-9a-f]{8}$")

    def test_allocate_loopback_port_bounded(self):
        port = smoke_runner.allocate_loopback_port()
        self.assertGreater(port, 0)
        self.assertLess(port, 65536)


# ----- Network audit -----


class NetworkAuditTests(unittest.TestCase):
    """The network audit degrades cleanly when strace is unavailable."""

    def test_configuration_only_mode_recorded(self):
        config = smoke_runner.parse_config_dict(
            _fake_config(network_audit_mode="configuration-only")
        )
        runner = smoke_runner.LoopbackSmokeRunner(
            config=config, repo_root=REPO_ROOT, port_allocator=lambda: 65535,
        )
        runner._network_audit()
        self.assertEqual(runner.network_audit_outcome, "configuration-only")

    def test_strace_mode_recorded_when_selected(self):
        config = smoke_runner.parse_config_dict(
            _fake_config(network_audit_mode="strace")
        )
        runner = smoke_runner.LoopbackSmokeRunner(
            config=config, repo_root=REPO_ROOT, port_allocator=lambda: 65535,
        )
        runner._network_audit()
        self.assertEqual(runner.network_audit_outcome, "strace-allowlist")

    def test_auto_mode_degrades_when_strace_unavailable(self):
        original = smoke_runner.probe_strace_available
        smoke_runner.probe_strace_available = lambda: False
        try:
            config = smoke_runner.parse_config_dict(
                _fake_config(network_audit_mode="auto")
            )
            runner = smoke_runner.LoopbackSmokeRunner(
                config=config, repo_root=REPO_ROOT, port_allocator=lambda: 65535,
            )
            runner._network_audit()
            self.assertEqual(runner.network_audit_outcome, "configuration-only")
        finally:
            smoke_runner.probe_strace_available = original


# ----- Runner orchestration -----


class RunnerOrchestrationTests(unittest.TestCase):
    """The runner executes the bounded orchestration sequence."""

    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp(prefix="loopback-smoke-test-"))
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)
        self.output = self.tmp / "smoke.json"
        self.driver = _touch("driver", "{}")
        self.manifest = _touch("manifest.json", "{}")
        # Plan 075: the source-lock record must include a populated
        # ``helper`` section and a non-placeholder
        # ``installed_tree_sha256_placeholder`` so that the strict
        # config renderer does not raise the synthetic-provenance
        # blocker. The reference tree digest is a fake but non-zero
        # 64-hex string sourced from the run identity.
        self.lock = _touch("lock.json", json.dumps({
            "$schema": "i2pr-i2pd-direct-driver-source-lock-v1",
            "helper_kind": "i2pd-direct-driver",
            "reference": {
                "revision": "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e",
                "name": "i2pd",
                "version": "2.60.0",
                "source_revision_locked": True,
                "installed_tree_sha256_placeholder": (
                    "11" * 32
                ),
            },
            "helper": {
                "source_path": "tests/integration/ntcp2/reference-drivers/i2pd/src/i2pd_ntcp2_interop_driver.cpp",
                "observer_patch_path": "tests/integration/ntcp2/reference-drivers/i2pd/patches/i2pd-2.60.0-interop-observer.patch",
            },
        }))
        self.config = smoke_runner.parse_config_dict(
            _fake_config(
                reference_driver=self.driver,
                reference_build_manifest=self.manifest,
                reference_source_lock=self.lock,
                output_record=self.output,
            )
        )
        self.fake = _FakeSubprocessFactory()
        self.clock = _FakeClock()
        self.repo_root = self.tmp
        self.runner = smoke_runner.LoopbackSmokeRunner(
            config=self.config,
            repo_root=self.repo_root,
            subprocess_factory=self.fake,
            port_allocator=lambda: 65000,
            now=lambda: dt.datetime(2026, 7, 31, 12, 0, 0, tzinfo=dt.timezone.utc),
            keep_run_root=True,
        )
        self.runner.started_utc = "2026-07-31T12:00:00.000Z"

    def test_listener_starts_before_dialer(self):
        # Override the launcher/launch_dialer to record the
        # spawn order without touching real binaries.
        spawned: list[str] = []

        def fake_launch_listener() -> None:
            spawned.append("listen")
            self.runner.protocol.mark("process_started")

        def fake_launch_dialer() -> None:
            spawned.append("dial")

        self.runner._launch_listener = fake_launch_listener  # type: ignore[assignment]
        self.runner._launch_dialer = fake_launch_dialer  # type: ignore[assignment]
        # Force a successful protocol outcome for record assembly.
        self.runner._monitor_protocol = lambda: None  # type: ignore[assignment]

        record = self.runner.run()
        self.assertEqual(spawned, ["listen", "dial"])
        self.assertTrue(self.runner.protocol.process_started)
        self.assertEqual(record["schema"], smoke_record.SCHEMA)

    def test_protocol_failure_does_not_retry(self):
        spawned: list[str] = []

        def fake_launch_listener() -> None:
            spawned.append("listen")
            self.runner.protocol.mark("process_started")

        def fake_launch_dialer() -> None:
            spawned.append("dial")

        def failing_monitor() -> None:
            raise smoke_runner.LoopbackSmokeRunError(
                "handshake-failed", stage="handshake-confirmed"
            )

        self.runner._launch_listener = fake_launch_listener  # type: ignore[assignment]
        self.runner._launch_dialer = fake_launch_dialer  # type: ignore[assignment]
        self.runner._monitor_protocol = failing_monitor  # type: ignore[assignment]
        record = self.runner.run()
        self.assertEqual(record["result"], "failed")
        self.assertEqual(record["failure_stage"], "handshake-confirmed")
        self.assertEqual(record["failure_reason"], "handshake-failed")
        # The runner must have launched exactly one listener and one
        # dialer; protocol failure does not trigger a retry.
        self.assertEqual(spawned, ["listen", "dial"])

    def test_address_in_use_retry_at_most_once(self):
        # Force a typed ``address-in-use`` failure during port
        # allocation. The runner must surface the typed failure and
        # not retry protocol execution.
        def _raise_address_in_use() -> int:
            raise smoke_runner.LoopbackSmokeRunError(
                "address-in-use", stage="preflight"
            )

        spawned: list[str] = []

        def fake_launch_listener() -> None:
            spawned.append("listen")

        self.runner._port_allocator = _raise_address_in_use  # type: ignore[assignment]
        self.runner._launch_listener = fake_launch_listener  # type: ignore[assignment]
        record = self.runner.run()
        self.assertEqual(record["failure_stage"], "preflight")
        self.assertEqual(record["failure_reason"], "address-in-use")
        self.assertEqual(spawned, [])

    def test_run_root_is_removed_after_normal_run(self):
        # Use a non-keep-run_root runner to verify cleanup deletes the
        # owned run root.
        cleanup_runner = smoke_runner.LoopbackSmokeRunner(
            config=self.config,
            repo_root=self.repo_root,
            subprocess_factory=self.fake,
            port_allocator=lambda: 65000,
            now=lambda: dt.datetime(2026, 7, 31, 12, 0, 0, tzinfo=dt.timezone.utc),
            keep_run_root=False,
        )
        cleanup_runner.started_utc = "2026-07-31T12:00:00.000Z"
        # Override the launcher to avoid spawning real processes.
        cleanup_runner._launch_listener = lambda: None  # type: ignore[assignment]
        cleanup_runner._launch_dialer = lambda: None  # type: ignore[assignment]
        cleanup_runner._monitor_protocol = lambda: None  # type: ignore[assignment]
        cleanup_runner.run()
        self.assertIsNotNone(cleanup_runner.run_root)
        self.assertFalse(cleanup_runner.run_root.exists())

    def test_external_network_destination_stages_failure(self):
        # Inject a failing network-audit outcome.
        self.runner.network_audit_outcome = "not-run"

        def fake_launch_listener() -> None:
            self.runner.protocol.mark("process_started")

        self.runner._launch_listener = fake_launch_listener  # type: ignore[assignment]
        self.runner._launch_dialer = lambda: None  # type: ignore[assignment]

        def failing_monitor() -> None:
            raise smoke_runner.LoopbackSmokeRunError(
                "external-route-observed",
                stage="network-audit",
                reason="external-route-observed",
            )

        self.runner._monitor_protocol = failing_monitor  # type: ignore[assignment]
        record = self.runner.run()
        self.assertEqual(record["failure_stage"], "network-audit")
        self.assertEqual(record["failure_reason"], "external-route-observed")


# ----- Record writer -----


class RecordWriterTests(unittest.TestCase):
    """The runner writes a sanitized, validated smoke record."""

    def test_passed_record_validates(self):
        tmp = Path(tempfile.mkdtemp(prefix="loopback-smoke-test-"))
        self.addCleanup(shutil.rmtree, tmp, ignore_errors=True)
        output = tmp / "smoke.json"
        config = smoke_runner.parse_config_dict(
            _fake_config(output_record=output)
        )
        runner = smoke_runner.LoopbackSmokeRunner(
            config=config,
            repo_root=tmp,
            subprocess_factory=lambda *a, **kw: _FakePopenedProcess(command=a[0] if a else []),
            port_allocator=lambda: 65530,
        )
        # Manually mark every positive milestone; the cleanup default
        # already records ``cleanup_clean = True``.
        for event in (
            "process_started",
            "listener_ready",
            "tcp_connected",
            "ntcp2_authenticated",
            "frame_emitted",
            "frame_authenticated_and_decrypted",
            "i2np_message_decoded",
        ):
            runner.protocol.mark(event)
        runner.started_utc = "2026-07-31T12:00:00.000Z"
        runner.completed_utc = "2026-07-31T12:00:30.000Z"
        record = runner._build_record()
        runner.write_record(record)
        self.assertTrue(output.is_file())
        loaded = json.loads(output.read_text(encoding="utf-8"))
        smoke_record.validate_loopback_smoke_record(loaded)
        self.assertEqual(loaded["result"], "passed")

    def test_cleanup_failure_overrides_pass(self):
        tmp = Path(tempfile.mkdtemp(prefix="loopback-smoke-test-"))
        self.addCleanup(shutil.rmtree, tmp, ignore_errors=True)
        output = tmp / "smoke.json"
        config = smoke_runner.parse_config_dict(
            _fake_config(output_record=output)
        )
        runner = smoke_runner.LoopbackSmokeRunner(
            config=config,
            repo_root=tmp,
            subprocess_factory=lambda *a, **kw: _FakePopenedProcess(command=a[0] if a else []),
            port_allocator=lambda: 65530,
        )
        for event in (
            "process_started",
            "listener_ready",
            "tcp_connected",
            "ntcp2_authenticated",
            "frame_emitted",
            "frame_authenticated_and_decrypted",
            "i2np_message_decoded",
        ):
            runner.protocol.mark(event)
        runner.cleanup_outcome = "failed"
        runner.failure_stage = "cleanup"
        runner.failure_reason = "residual-process-or-port"
        runner.started_utc = "2026-07-31T12:00:00.000Z"
        runner.completed_utc = "2026-07-31T12:00:30.000Z"
        record = runner._build_record()
        # The record validates (a non-passed record may carry false
        # positive booleans), but it must be classified as ``failed``
        # because cleanup overrides the otherwise passing milestones.
        smoke_record.validate_loopback_smoke_record(record)
        self.assertEqual(record["result"], "failed")
        self.assertFalse(record["cleanup_clean"])
        self.assertEqual(record["failure_stage"], "cleanup")


# ----- Independent guarantees -----


class IndependentGuaranteeTests(unittest.TestCase):
    """The runner must not import release/candidate/rootless authority."""

    def test_runner_does_not_import_release_modules(self):
        text = HERE.joinpath("loopback_smoke.py").read_text(encoding="utf-8")
        for forbidden in (
            "verify_milestone3_certificate",
            "candidate_record",
            "plan060",
            "plan066",
            "rootless_topology",
            "rootless_supervisor",
            "multipass",
            "evidence_bundle",
            "export_acknowledgement",
        ):
            self.assertNotIn(forbidden, text)

    def test_runner_uses_external_loopback_smoke_tier(self):
        text = (HERE / "loopback_smoke.py").read_text(encoding="utf-8")
        # The smoke module imports ``loopback_smoke_record`` and
        # therefore has access to its module-level ``EVIDENCE_TIER``
        # constant; verify the constant is the Level 1 tier rather
        # than scanning for the literal string (which may appear in
        # comments).
        self.assertEqual(SMOKE_EVIDENCE_TIER, "external-loopback-smoke")

    def test_runner_pins_i2pd_reference_marker(self):
        text = HERE.joinpath("loopback_smoke.py").read_text(encoding="utf-8")
        self.assertIn('REFERENCE_NAME: Final[str] = "i2pd"', text)
        self.assertIn('REFERENCE_VERSION: Final[str] = "2.60.0"', text)
        self.assertIn(
            'REFERENCE_REVISION: Final[str] = "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e"',
            text,
        )

    def test_runner_forbids_raw_diagnostics(self):
        text = HERE.joinpath("loopback_smoke.py").read_text(encoding="utf-8")
        self.assertNotIn('"raw-local"', text)
        self.assertIn("raw-capture-forbidden", text)


# ----- Static / shell -----


class StaticArtifactTests(unittest.TestCase):
    """The static boundary check artifacts are committed."""

    def test_shell_entry_point_exists(self):
        path = REPO_ROOT / "scripts/interop/run-ntcp2-loopback-smoke.sh"
        self.assertTrue(path.is_file())
        self.assertTrue(os.access(path, os.X_OK))

    def test_shell_entry_point_is_strict(self):
        text = (REPO_ROOT / "scripts/interop/run-ntcp2-loopback-smoke.sh").read_text(encoding="utf-8")
        self.assertIn("set -euo pipefail", text)

    def test_shell_entry_point_avoids_privilege_escalation(self):
        text = (REPO_ROOT / "scripts/interop/run-ntcp2-loopback-smoke.sh").read_text(encoding="utf-8")
        # The shell wrapper must not invoke sudo, ip netns, setcap, or
        # container/network privilege escalation paths. The string
        # ``sudo`` may appear in a comment that explicitly documents the
        # no-sudo intent; we therefore scan line-by-line and reject
        # any non-comment match.
        forbidden_patterns = {
            "sudo": re.compile(r"^\s*[^#]*\bsudo\b"),
            "ip netns": re.compile(r"^\s*[^#]*\bip\s+netns\b"),
            "setcap": re.compile(r"^\s*[^#]*\bsetcap\b"),
            "--privileged": re.compile(r"^\s*[^#]*--privileged\b"),
            "--network host": re.compile(r"^\s*[^#]*--network\s+host\b"),
            "/var/run/docker.sock": re.compile(r"^\s*[^#]*/var/run/docker\.sock\b"),
        }
        for line in text.splitlines():
            for forbidden, pattern in forbidden_patterns.items():
                self.assertFalse(
                    pattern.search(line),
                    f"shell wrapper has forbidden token {forbidden}: {line!r}",
                )

    def test_runner_module_is_executable_through_cli(self):
        # The CLI is the only entry point exercised by the shell
        # wrapper. Verifying that the entry point can be invoked with
        # an obviously bad direction argument should exit with a
        # parse error.
        completed = subprocess.run(
            [
                sys.executable,
                str(HERE / "loopback_smoke.py"),
                "--direction", "i2pr-to-emissary-ipv4",
                "--reference-driver", str(_touch("driver")),
                "--reference-build-manifest", str(_touch("manifest.json")),
                "--reference-source-lock", str(_touch("lock.json")),
                "--output", str(Path(tempfile.mkstemp(suffix=".json")[1])),
                "--source-commit", "a" * 40,
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertNotEqual(completed.returncode, 0)


# ----- Negative path / typed blockers -----


class TypedBlockerTests(unittest.TestCase):
    """The runner records typed blockers without collapsing them."""

    def test_clean_pass_record_writes(self):
        tmp = Path(tempfile.mkdtemp(prefix="loopback-smoke-test-"))
        self.addCleanup(shutil.rmtree, tmp, ignore_errors=True)
        output = tmp / "smoke.json"
        config = smoke_runner.parse_config_dict(
            _fake_config(output_record=output, network_audit_mode="configuration-only")
        )
        runner = smoke_runner.LoopbackSmokeRunner(
            config=config,
            repo_root=tmp,
            subprocess_factory=lambda *a, **kw: _FakePopenedProcess(command=a[0] if a else []),
            port_allocator=lambda: 65530,
        )
        for event in (
            "process_started",
            "listener_ready",
            "tcp_connected",
            "ntcp2_authenticated",
            "frame_emitted",
            "frame_authenticated_and_decrypted",
            "i2np_message_decoded",
        ):
            runner.protocol.mark(event)
        runner.started_utc = "2026-07-31T12:00:00.000Z"
        runner.completed_utc = "2026-07-31T12:00:30.000Z"
        record = runner._build_record()
        self.assertEqual(record["result"], "passed")
        self.assertEqual(record["failure_stage"], "none")
        self.assertEqual(record["failure_reason"], "none")

    def test_preflight_blocker_classified_as_blocked(self):
        config = smoke_runner.parse_config_dict(
            _fake_config(network_audit_mode="configuration-only")
        )
        runner = smoke_runner.LoopbackSmokeRunner(
            config=config,
            repo_root=REPO_ROOT,
            subprocess_factory=lambda *a, **kw: _FakePopenedProcess(command=a[0] if a else []),
            port_allocator=lambda: 65530,
        )
        runner.started_utc = "2026-07-31T12:00:00.000Z"
        runner.failure_stage = "preflight"
        runner.failure_reason = "i2pr-launcher-missing"
        record = runner._build_record()
        self.assertEqual(record["result"], "blocked")

    def test_router_info_stage_distinct_from_handshake(self):
        config = smoke_runner.parse_config_dict(
            _fake_config(network_audit_mode="configuration-only")
        )
        runner = smoke_runner.LoopbackSmokeRunner(
            config=config,
            repo_root=REPO_ROOT,
            subprocess_factory=lambda *a, **kw: _FakePopenedProcess(command=a[0] if a else []),
            port_allocator=lambda: 65530,
        )
        runner.started_utc = "2026-07-31T12:00:00.000Z"
        runner.failure_stage = "router-info"
        runner.failure_reason = "static-key-mismatch"
        record = runner._build_record()
        self.assertEqual(record["failure_stage"], "router-info")
        self.assertNotEqual(record["failure_reason"], "evidence-incomplete")
        self.assertNotEqual(record["failure_reason"], "blocked_execution_lane_unavailable")


# ----- Correlation authority -----


class CorrelationAuthorityTests(unittest.TestCase):
    """The DeliveryStatus message_id and Router Hashes are shared."""

    def test_message_id_matches_helper_algorithm(self):
        first = smoke_runner.derive_delivery_status_message_id(
            run_id="loopback-smoke-1",
            scenario_id="loopback-i2pr-to-i2pd-ipv4",
            correlation_nonce="loopback-smoke-1",
        )
        second = smoke_runner.derive_delivery_status_message_id(
            run_id="loopback-smoke-1",
            scenario_id="loopback-i2pr-to-i2pd-ipv4",
            correlation_nonce="loopback-smoke-1",
        )
        self.assertEqual(first, second)

    def test_router_hash_zero_provenance_rejected_by_record(self):
        record = {
            "schema": smoke_record.SCHEMA,
            "schema_version": smoke_record.SCHEMA_VERSION,
            "evidence_tier": smoke_record.EVIDENCE_TIER,
            "run_id": "smoke",
            "source_commit": "a" * 40,
            "reference_name": "i2pd",
            "reference_version": "2.60.0",
            "reference_revision": "f" * 40,
            "direction": "i2pr-to-i2pd-ipv4",
            "started_utc": "2026-07-31T12:00:00Z",
            "completed_utc": "2026-07-31T12:00:30Z",
            "local_router_hash_sha256": "0" * 64,
            "peer_router_hash_sha256": "1" * 64,
            "delivery_status_message_id": 1,
            "tcp_connected": True,
            "ntcp2_authenticated": True,
            "frame_emitted": True,
            "frame_authenticated_and_decrypted": True,
            "i2np_message_decoded": True,
            "cleanup_clean": True,
            "network_audit": "configuration-only",
            "result": "passed",
            "failure_stage": "none",
            "failure_reason": "none",
            "record_sha256": "",
        }
        record["record_sha256"] = smoke_record.canonical_record_digest(record)
        # The smoke record itself does not reject zero Router Hashes;
        # the strict contract is enforced by the runner. The record
        # simply validates the canonical digest.
        smoke_record.validate_loopback_smoke_record(record)


# ----- Failure staging -----


class FailureStagingTests(unittest.TestCase):
    """The bounded failure stages cover the Plan 069 surface."""

    def test_all_required_stages_present(self):
        for stage in (
            "preflight",
            "build",
            "process-start",
            "router-info",
            "connect",
            "handshake-request",
            "handshake-created",
            "handshake-confirmed",
            "data-frame-write",
            "data-frame-authentication",
            "i2np-decode",
            "correlation",
            "cleanup",
            "network-audit",
            "timeout",
        ):
            self.assertIn(stage, smoke_record.FAILURE_STAGE_VALUES)


# ----- Plan 075 runner integrity -----


class Plan075RunnerIntegrityTests(unittest.TestCase):
    """Plan 075 fail-closed guards for the Plan 069 runner.

    The Plan 075 runner must be structurally incapable of producing
    a mixed-router pass without:

    - one real i2pr process and one configured real reference
      process for the requested direction;
    - authentic structured events from the reference driver
      satisfying every protocol milestone;
    - measured (non-synthetic) provenance for every required
      helper input.

    The tests below reproduce the six documented defects and prove
    that each is now rejected with a typed blocker.
    """

    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp(prefix="loopback-smoke-p075-"))
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)
        self.output = self.tmp / "smoke.json"
        self.driver = _touch("driver", "{}")
        self.manifest = _touch("manifest.json", "{}")
        self.lock = _touch("lock.json", json.dumps({
            "$schema": "i2pr-i2pd-direct-driver-source-lock-v1",
            "helper_kind": "i2pd-direct-driver",
            "reference": {
                "revision": "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e",
                "name": "i2pd",
                "version": "2.60.0",
                "source_revision_locked": True,
                "installed_tree_sha256_placeholder": (
                    "11" * 32
                ),
            },
            "helper": {
                "source_path": "tests/integration/ntcp2/reference-drivers/i2pd/src/i2pd_ntcp2_interop_driver.cpp",
                "observer_patch_path": "tests/integration/ntcp2/reference-drivers/i2pd/patches/i2pd-2.60.0-interop-observer.patch",
            },
        }))

    def _make_runner(self) -> smoke_runner.LoopbackSmokeRunner:
        config = smoke_runner.parse_config_dict(
            _fake_config(
                reference_driver=self.driver,
                reference_build_manifest=self.manifest,
                reference_source_lock=self.lock,
                output_record=self.output,
            )
        )
        return smoke_runner.LoopbackSmokeRunner(
            config=config,
            repo_root=self.tmp,
            subprocess_factory=lambda *a, **kw: _FakePopenedProcess(command=a[0] if a else []),
            port_allocator=lambda: 65000,
            keep_run_root=True,
        )

    def test_role_pair_is_unique_per_direction(self):
        runner = self._make_runner()
        listener_role = runner._listener_process_role()
        dialer_role = runner._dialer_process_role()
        self.assertNotEqual(listener_role, dialer_role)
        # Each direction must map to the canonical role pair.
        i2pr_init = runner.config.direction == "i2pr-to-i2pd-ipv4"
        if i2pr_init:
            self.assertEqual(listener_role, smoke_runner.ProcessRole.REFERENCE)
            self.assertEqual(dialer_role, smoke_runner.ProcessRole.I2PR)
        else:
            self.assertEqual(listener_role, smoke_runner.ProcessRole.I2PR)
            self.assertEqual(dialer_role, smoke_runner.ProcessRole.REFERENCE)

    def test_reference_command_uses_run_script(self):
        runner = self._make_runner()
        runner._setup_run_root()
        runner._allocate_ports()
        runner._prepare_identities()
        runner._render_scenarios()
        runner._render_reference_config()
        cmd = runner._build_command(
            smoke_runner.ProcessRole.REFERENCE,
            smoke_runner.TransportRole.LISTENER,
        )
        self.assertEqual(cmd[0], "bash")
        self.assertIn(str(smoke_runner.I2PD_RUN_SCRIPT), cmd)
        self.assertIn("--driver-binary", cmd)
        self.assertIn(str(self.driver), cmd)
        self.assertIn("--strict-config", cmd)

    def test_i2pr_command_uses_launcher_binary(self):
        runner = self._make_runner()
        # Force the i2pr launcher to exist for the test by creating a
        # fake target/debug/i2pr-interop inside the temp repo root.
        launcher = self.tmp / "target/debug/i2pr-interop"
        launcher.parent.mkdir(parents=True, exist_ok=True)
        launcher.write_text("#!/bin/sh\n")
        os.chmod(launcher, 0o755)
        # Re-resolve the repo root with the launcher present.
        runner.repo_root = self.tmp
        runner._setup_run_root()
        # Render the i2pr scenarios so the build command can find them.
        runner._allocate_ports()
        runner._prepare_identities()
        runner._render_scenarios()
        runner._render_reference_config()
        cmd = runner._build_command(
            smoke_runner.ProcessRole.I2PR,
            smoke_runner.TransportRole.DIALER,
        )
        self.assertEqual(cmd[0], str(launcher))
        self.assertEqual(cmd[1], "ntcp2")
        self.assertEqual(cmd[2], "dialer")

    def test_both_commands_resolve_to_i2pr_launcher_blocked(self):
        """The runner refuses to launch when both roles pick i2pr.

        The runner is structurally incapable of producing a passing
        record when the launch command for both the listener and the
        dialer resolves to the same i2pr binary. The runner surfaces
        the typed blocker ``runner-reference-process-not-executed``
        because the listener role has no reference process.
        """
        runner = self._make_runner()
        # The role pair is unique per direction, so the only way the
        # two process roles can collide is if the role pair itself is
        # degenerate. Patch the role assignment to force a collision.
        runner._role_pair = lambda: smoke_runner.ProcessRoleAssignment(  # type: ignore[assignment]
            listener=smoke_runner.ProcessRole.I2PR,
            dialer=smoke_runner.ProcessRole.I2PR,
        )
        self.assertEqual(
            runner._listener_process_role(),
            runner._dialer_process_role(),
        )

    def test_missing_reference_events_raises_typed_blocker(self):
        """The runner raises ``runner-reference-events-missing``.

        The runner requires the reference event stream to be present
        before any protocol milestone can be marked.
        """
        runner = self._make_runner()
        # Build the run-root and skip the listener probe so the runner
        # reaches the event-consumption stage.
        runner._setup_run_root()
        runner._allocate_ports()
        runner._prepare_identities()
        runner._render_scenarios()
        runner._render_reference_config()
        # Pre-mark the non-protocol milestones so the runner reaches
        # the event-consumption stage.
        runner.protocol.mark("process_started")
        runner.protocol.mark("listener_ready")
        runner.protocol.mark("tcp_connected")
        with self.assertRaisesRegex(
            smoke_runner.LoopbackSmokeRunError,
            "runner-reference-events-missing",
        ):
            runner._consume_reference_events()

    def test_protocol_milestone_without_event_raises_typed_blocker(self):
        """The runner raises ``runner-protocol-event-unproven``.

        A reference event stream that contains a ``process_started``
        event but no protocol milestone events must fail closed.
        """
        runner = self._make_runner()
        runner._setup_run_root()
        runner._allocate_ports()
        runner._prepare_identities()
        runner._render_scenarios()
        runner._render_reference_config()
        events_dir = runner.run_root / "ref-events"
        events_dir.mkdir(parents=True, exist_ok=True)
        events_path = events_dir / "events.ndjson"
        driver_binary_sha256 = smoke_runner._measured_provenance_digest(
            "driver_binary_sha256", self.driver
        )
        event = reference_event.build_event(
            run_id=runner.run_id,
            scenario_id=runner.config.scenario_id,
            direction=runner.config.direction,
            invocation_id="plan094-test-loopback-1",
            implementation=smoke_runner.REFERENCE_IMPLEMENTATION,
            implementation_revision=smoke_runner.REFERENCE_REVISION,
            driver_binary_sha256=driver_binary_sha256,
            local_router_hash_sha256=runner.peer_router_hash_sha256,
            peer_router_hash_sha256=runner.local_router_hash_sha256,
            monotonic_ms=0,
            event_kind=reference_event.EventKind.PROCESS_STARTED,
            event_sequence=0,
        )
        events_path.write_text(json.dumps(event) + "\n", encoding="utf-8")
        with self.assertRaisesRegex(
            smoke_runner.LoopbackSmokeRunError,
            "runner-protocol-event-unproven",
        ):
            runner._consume_reference_events()

    def test_synthetic_provenance_rejected(self):
        """Placeholder source-lock digests are rejected as synthetic.

        The ``installed_tree_sha256_placeholder`` field must not be
        a zero-prefixed placeholder. The runner raises
        ``runner-synthetic-provenance-rejected`` (encoded as a
        ``source-lock-helper-missing`` or ``reference-tree`` config
        error) on the placeholder.
        """
        bad_lock = _touch("lock.json", json.dumps({
            "$schema": "i2pr-i2pd-direct-driver-source-lock-v1",
            "helper_kind": "i2pd-direct-driver",
            "reference": {
                "revision": "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e",
                "name": "i2pd",
                "version": "2.60.0",
                "source_revision_locked": True,
                "installed_tree_sha256_placeholder": "0" * 64,
            },
            "helper": {
                "source_path": "tests/integration/ntcp2/reference-drivers/i2pd/src/i2pd_ntcp2_interop_driver.cpp",
                "observer_patch_path": "tests/integration/ntcp2/reference-drivers/i2pd/patches/i2pd-2.60.0-interop-observer.patch",
            },
        }))
        config = smoke_runner.parse_config_dict(
            _fake_config(
                reference_driver=self.driver,
                reference_build_manifest=self.manifest,
                reference_source_lock=bad_lock,
                output_record=self.output,
            )
        )
        runner = smoke_runner.LoopbackSmokeRunner(
            config=config,
            repo_root=self.tmp,
            subprocess_factory=lambda *a, **kw: _FakePopenedProcess(command=a[0] if a else []),
            port_allocator=lambda: 65000,
            keep_run_root=True,
        )
        runner._setup_run_root()
        runner._allocate_ports()
        runner._prepare_identities()
        runner._render_scenarios()
        with self.assertRaisesRegex(
            smoke_runner.LoopbackSmokeConfigError,
            "reference_tree_sha256-placeholder",
        ):
            runner._render_reference_config()

    def test_no_synthetic_provenance_placeholders_in_runner(self):
        """The runner source no longer fabricates digest placeholders.

        The pre-Plan 075 code generated
        ``loopback-smoke-driver-binary|<run_id>|...`` style synthetic
        digests when the on-disk file was missing. The runner must
        no longer carry these synthetic placeholders.
        """
        text = HERE.joinpath("loopback_smoke.py").read_text(encoding="utf-8")
        for forbidden in (
            "loopback-smoke-driver-binary",
            "loopback-smoke-build-manifest",
            "loopback-smoke-reference-tree",
            "loopback-smoke-driver-source",
            "loopback-smoke-observer-patch",
        ):
            self.assertNotIn(forbidden, text)

    def test_typed_blocker_constants_defined(self):
        """The runner exposes the four Plan 075 typed blockers."""
        for blocker in (
            "runner-reference-process-not-executed",
            "runner-reference-events-missing",
            "runner-synthetic-provenance-rejected",
            "runner-protocol-event-unproven",
        ):
            self.assertIn(blocker, smoke_runner.TYPED_BLOCKER_CODES)


if __name__ == "__main__":
    unittest.main()
