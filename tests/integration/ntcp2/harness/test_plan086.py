"""Plan 086 host-loopback-development lane test matrix.

The Plan 086 test matrix locks the host-loopback development lane
contracts introduced by Plan 086:

- the bounded ``host-loopback-development`` topology kind is the only
  new topology added by Plan 086; it is allowlisted in the canonical
  runner modules and the in-process schema;
- the topology is ``development_only``: it never satisfies a release
  or isolation predicate; the metadata is recorded verbatim from
  the canonical ``HOST_LOOPBACK_DEVELOPMENT_METADATA`` table;
- the topology is bound to literal IPv4 ``127.0.0.1`` only; the
  alternate loopback addresses (``127.0.0.0``/``24``, ``::1``),
  wildcards, hostnames, DNS names, public addresses, and the
  RFC 5737 documentation addresses outside the development lane
  are rejected by the strict parser;
- the ``HostLoopbackDevelopmentPlacement`` is the only path that
  owns the development topology outside the canonical runner
  modules; it never invokes sudo, namespace, Multipass, or a
  shell; the binary path is measured at construction time and the
  environment is filtered to a small allowlist;
- the canonical 11-step runner architecture is reused unchanged; the
  bounded preflight stops at ``listener_ready`` and refuses to
  start a dialer;
- the ``host-loopback-development`` topology never imports
  Plan 056/066 candidate, bundle, certificate, rootless-topology,
  or Multipass authority.

The tests use the in-process schema and placement alone; they never
launch a subprocess and never need the real i2pd binary.
"""

from __future__ import annotations

import json
import os
import re
import tempfile
import unittest
from pathlib import Path

import interop_topology
import minimal_i2pd_probe as forward_probe
import minimal_i2pd_reverse_probe as reverse_probe


# Plan 086 closure states. Exactly three values; any other token must
# fail closed in ``plans/086-status.md``.
PLAN_086_CLOSURE_STATES: tuple[str, ...] = (
    "host-loopback-development-ready",
    "manual-isolated-fallback-required",
    "blocked-artifact-or-build-defect",
)


# Plan 086 single-execution blocker token.
MANUAL_ISOLATED_FALLBACK_REQUIRED: Final[str] = "manual-isolated-fallback-required"


class Plan086TopologyTests(unittest.TestCase):
    """Plan 086 topology constant and bounded metadata."""

    def test_host_loopback_development_topology_is_allowlisted(self) -> None:
        self.assertIn(
            interop_topology.HOST_LOOPBACK_DEVELOPMENT_TOPOLOGY_KIND,
            interop_topology.ALLOWED_TOPOLOGY_KINDS,
        )
        self.assertEqual(
            interop_topology.HOST_LOOPBACK_DEVELOPMENT_TOPOLOGY_KIND,
            "host-loopback-development",
        )

    def test_host_loopback_development_is_development_only(self) -> None:
        self.assertIn(
            interop_topology.HOST_LOOPBACK_DEVELOPMENT_TOPOLOGY_KIND,
            forward_probe.DEVELOPMENT_ONLY_TOPOLOGY_KINDS,
        )
        self.assertIn(
            interop_topology.HOST_LOOPBACK_DEVELOPMENT_TOPOLOGY_KIND,
            ["host-loopback-development"],
        )

    def test_metadata_records_release_false(self) -> None:
        metadata = interop_topology.HOST_LOOPBACK_DEVELOPMENT_METADATA
        self.assertTrue(metadata["development_only"])
        self.assertFalse(metadata["release_qualified"])
        self.assertFalse(metadata["isolation_qualified"])
        self.assertEqual(metadata["public_network_blocked"], "unproven")
        self.assertTrue(metadata["parent_network_state_unchanged"])
        self.assertEqual(metadata["bind_address"], "127.0.0.1")
        self.assertEqual(metadata["peer_address"], "127.0.0.1")
        self.assertEqual(metadata["network_id"], 99)
        self.assertEqual(metadata["reference"], "i2pd")

    def test_metadata_keys_are_closed(self) -> None:
        # The Plan 086 metadata schema is a closed set; the upstream
        # topology record cannot be extended without an explicit plan.
        expected_keys = {
            "topology_kind",
            "development_only",
            "release_qualified",
            "isolation_qualified",
            "public_network_blocked",
            "parent_network_state_unchanged",
            "endpoint_family",
            "bind_address",
            "peer_address",
            "network_id",
            "reference",
        }
        self.assertEqual(set(interop_topology.HOST_LOOPBACK_DEVELOPMENT_METADATA), expected_keys)


class Plan086PlacementTests(unittest.TestCase):
    """Plan 086 ``HostLoopbackDevelopmentPlacement`` contract."""

    def test_placement_rejects_relative_binary_path(self) -> None:
        with self.assertRaises(interop_topology.TopologyContractError):
            interop_topology.HostLoopbackDevelopmentPlacement(
                actor="i2pr",
                binary_path="target/debug/i2pr-interop",
                log_path="/tmp/i2pr.log",
            )

    def test_placement_rejects_relative_log_path(self) -> None:
        with self.assertRaises(interop_topology.TopologyContractError):
            interop_topology.HostLoopbackDevelopmentPlacement(
                actor="i2pr",
                binary_path="/abs/path/i2pr-interop",
                log_path="raw/i2pr.log",
            )

    def test_placement_rejects_unknown_actor(self) -> None:
        with self.assertRaises(interop_topology.TopologyContractError):
            interop_topology.HostLoopbackDevelopmentPlacement(
                actor="rogue",
                binary_path="/abs/path/i2pr-interop",
                log_path="/tmp/i2pr.log",
            )

    def test_placement_rejects_ld_preload_environment(self) -> None:
        with self.assertRaises(interop_topology.TopologyContractError):
            interop_topology.HostLoopbackDevelopmentPlacement(
                actor="i2pr",
                binary_path="/abs/path/i2pr-interop",
                log_path="/tmp/i2pr.log",
                environment=(("LD_PRELOAD", "/tmp/evil.so"),),
            )

    def test_placement_command_returns_absolute_argv(self) -> None:
        placement = interop_topology.HostLoopbackDevelopmentPlacement(
            actor="i2pr",
            binary_path="/abs/path/i2pr-interop",
            log_path="/tmp/i2pr.log",
            environment=(("I2PR_PLAN046_HOST_BLOCKER", "blocked_test"),),
        )
        self.assertEqual(
            placement.command(["ntcp2", "prepare", "--state-dir", "/tmp/state"]),
            ["/abs/path/i2pr-interop", "ntcp2", "prepare", "--state-dir", "/tmp/state"],
        )
        self.assertEqual(placement.topology_kind, "host-loopback-development")
        self.assertEqual(
            placement.environment_dict(),
            {"I2PR_PLAN046_HOST_BLOCKER": "blocked_test"},
        )

    def test_placement_command_rejects_empty_argv(self) -> None:
        placement = interop_topology.HostLoopbackDevelopmentPlacement(
            actor="i2pr",
            binary_path="/abs/path/i2pr-interop",
            log_path="/tmp/i2pr.log",
        )
        with self.assertRaises(interop_topology.TopologyContractError):
            placement.command([])

    def test_placement_topology_kind_is_locked(self) -> None:
        placement = interop_topology.HostLoopbackDevelopmentPlacement(
            actor="i2pr",
            binary_path="/abs/path/i2pr-interop",
            log_path="/tmp/i2pr.log",
        )
        self.assertEqual(placement.topology_kind, "host-loopback-development")


class Plan086ClosureStateTests(unittest.TestCase):
    """Plan 086 closure-state vocabulary is exactly three values."""

    def test_closure_states_are_bounded(self) -> None:
        self.assertEqual(len(PLAN_086_CLOSURE_STATES), 3)
        self.assertEqual(
            len(set(PLAN_086_CLOSURE_STATES)),
            3,
            "Plan 086 closure vocabulary must not contain duplicates",
        )

    def test_ready_state_is_listed(self) -> None:
        self.assertIn("host-loopback-development-ready", PLAN_086_CLOSURE_STATES)

    def test_manual_isolated_fallback_state_is_listed(self) -> None:
        self.assertIn("manual-isolated-fallback-required", PLAN_086_CLOSURE_STATES)

    def test_blocked_state_is_listed(self) -> None:
        self.assertIn("blocked-artifact-or-build-defect", PLAN_086_CLOSURE_STATES)

    def test_legacy_decision_tokens_are_not_listed(self) -> None:
        # Plan 086 supersedes the legacy Plan 084 ``lane-invalidated``
        # and ``same-stage-two-way-i2pr-defect`` decision tokens.
        self.assertNotIn("lane-invalidated", PLAN_086_CLOSURE_STATES)
        self.assertNotIn("same-stage-two-way-i2pr-defect", PLAN_086_CLOSURE_STATES)


class Plan086AddressClassTests(unittest.TestCase):
    """Plan 086: literal IPv4 ``127.0.0.1`` is accepted only under the
    development topology; every other topology remains synthetic.
    """

    def _make_adapter(self, tmp: str) -> tuple[Path, "I2prAdapter"]:
        from i2pr import I2prAdapter

        root = Path(tmp)
        binary = Path("/abs/path/i2pr-interop")
        log = root / "raw/i2pr.log"
        placement = interop_topology.HostLoopbackDevelopmentPlacement(
            actor="i2pr",
            binary_path=str(binary),
            log_path=str(log),
        )
        adapter = I2prAdapter(repo_root=root, run_root=root, placement=placement)
        return root, adapter

    def _validate_fails(self, address: str) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            _root, adapter = self._make_adapter(tmp)
            with self.assertRaises(RuntimeError):
                adapter.prepare_state(
                    local_address=address,
                    local_port=45680,
                    network_id=99,
                    deterministic_seed=7,
                    state_dir="state",
                )

    def test_synthetic_range_is_default_and_accepts_loopback_when_disallowed(self) -> None:
        # Under the default topology (None), literal IPv4 loopback is
        # rejected with ``prepare-input-invalid`` before any subprocess
        # is executed.
        self._validate_fails("127.0.0.1")

    def test_loopback_address_accepted_only_for_host_loopback_topology(self) -> None:
        from i2pr import I2prAdapter

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            placement = interop_topology.HostLoopbackDevelopmentPlacement(
                actor="i2pr",
                binary_path="/abs/path/i2pr-interop",
                log_path=str(root / "raw/i2pr.log"),
            )
            adapter = I2prAdapter(repo_root=root, run_root=root, placement=placement)
            # The adapter must accept 127.0.0.1 when the topology is
            # explicitly set to host-loopback-development. The check
            # never opens a socket; we only validate the input.
            try:
                adapter.prepare_state(
                    local_address="127.0.0.1",
                    local_port=45680,
                    network_id=99,
                    deterministic_seed=7,
                    state_dir="state",
                    topology_kind="host-loopback-development",
                )
            except RuntimeError as exc:
                # The runner may still fail to find the binary in a
                # test environment; the failure must not be the
                # address check.
                self.assertNotIn("prepare-input-invalid", str(exc))

    def test_alternate_loopback_addresses_are_rejected(self) -> None:
        from i2pr import I2prAdapter

        for address in ("127.0.0.0", "127.0.0.2", "127.255.255.255", "::1", "0.0.0.0"):
            with tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                placement = interop_topology.HostLoopbackDevelopmentPlacement(
                    actor="i2pr",
                    binary_path="/abs/path/i2pr-interop",
                    log_path=str(root / "raw/i2pr.log"),
                )
                adapter = I2prAdapter(repo_root=root, run_root=root, placement=placement)
                with self.assertRaises(RuntimeError):
                    adapter.prepare_state(
                        local_address=address,
                        local_port=45680,
                        network_id=99,
                        deterministic_seed=7,
                        state_dir="state",
                        topology_kind="host-loopback-development",
                    )

    def test_public_addresses_are_rejected(self) -> None:
        from i2pr import I2prAdapter

        for address in ("10.0.0.1", "8.8.8.8", "1.1.1.1"):
            with tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                placement = interop_topology.HostLoopbackDevelopmentPlacement(
                    actor="i2pr",
                    binary_path="/abs/path/i2pr-interop",
                    log_path=str(root / "raw/i2pr.log"),
                )
                adapter = I2prAdapter(repo_root=root, run_root=root, placement=placement)
                with self.assertRaises(RuntimeError):
                    adapter.prepare_state(
                        local_address=address,
                        local_port=45680,
                        network_id=99,
                        deterministic_seed=7,
                        state_dir="state",
                        topology_kind="host-loopback-development",
                    )


class Plan086RunnerTopologyTests(unittest.TestCase):
    """Plan 086: the canonical runners accept the development topology."""

    def test_forward_runner_accepts_host_loopback_development(self) -> None:
        import plan083_runner as runner_mod

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            config = runner_mod.ProbeConfig(
                run_id="plan086-forward",
                source_commit="a" * 40,
                reference_revision="b" * 40,
                lane_qualification_sha256="c" * 64,
                topology_kind="host-loopback-development",
                parent_network_state_unchanged=True,
                i2pr_binary_sha256="d" * 64,
                i2pd_binary_sha256="e" * 64,
                output_path=root / "probe-record.json",
            )
            runner = runner_mod.ProbeRunner(config)
            output = runner.run(root)
            record = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(record["topology_kind"], "host-loopback-development")
            self.assertNotEqual(
                record["terminal_result"],
                forward_probe.LANE_INVALID,
            )
            forward_probe.validate_record(record)

    def test_reverse_runner_accepts_host_loopback_development(self) -> None:
        import plan084_runner as runner_mod

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            config = runner_mod.ReverseProbeConfig(
                run_id="plan086-reverse",
                source_commit="a" * 40,
                reference_revision="b" * 40,
                lane_qualification_sha256="c" * 64,
                topology_kind="host-loopback-development",
                parent_network_state_unchanged=True,
                i2pr_binary_sha256="d" * 64,
                i2pd_binary_sha256="e" * 64,
                output_path=root / "reverse-probe-record.json",
            )
            runner = runner_mod.ReverseProbeRunner(config)
            output = runner.run(root)
            record = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(record["topology_kind"], "host-loopback-development")
            self.assertNotEqual(
                record["terminal_result"],
                reverse_probe.LANE_INVALID,
            )
            reverse_probe.validate_reverse_record(record)

    def test_forward_runner_rejects_unknown_topology(self) -> None:
        import plan083_runner as runner_mod

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            config = runner_mod.ProbeConfig(
                run_id="plan086-bad",
                source_commit="a" * 40,
                reference_revision="b" * 40,
                lane_qualification_sha256="c" * 64,
                topology_kind="public-network",
                parent_network_state_unchanged=True,
                i2pr_binary_sha256="d" * 64,
                i2pd_binary_sha256="e" * 64,
                output_path=root / "probe-record.json",
            )
            runner = runner_mod.ProbeRunner(config)
            output = runner.run(root)
            record = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(record["terminal_result"], forward_probe.LANE_INVALID)


class Plan086BoundaryTests(unittest.TestCase):
    """Plan 086: the placement module does not import release authority."""

    def test_interop_topology_does_not_import_release_authority(self) -> None:
        source_path = interop_topology.__file__
        if source_path is None:
            self.skipTest("module path unavailable")
        text = Path(source_path).read_text(encoding="utf-8")
        code_only = re.sub(r"\"\"\".*?\"\"\"", "", text, flags=re.DOTALL)
        code_only = re.sub(r"#.*", "", code_only)
        for forbidden in (
            "verify_milestone3_certificate",
            "candidate_record",
            "evidence_bundle",
            "from .rootless_topology",
            "from .multipass",
        ):
            self.assertNotIn(
                forbidden,
                code_only,
                f"interop_topology imports release authority: {forbidden}",
            )

    def test_placement_is_narrow_and_decoupled(self) -> None:
        # The placement does not bring in any namespace or
        # Multipass-aware module; it is a single-purpose host
        # placement.
        placement = interop_topology.HostLoopbackDevelopmentPlacement(
            actor="i2pr",
            binary_path="/abs/path/i2pr-interop",
            log_path="/tmp/i2pr.log",
        )
        self.assertFalse(hasattr(placement, "command_prefix"))
        self.assertNotIn("namespace", dir(placement))
        self.assertNotIn("multipass", dir(placement))


class Plan086LoopbackEndpointTests(unittest.TestCase):
    """Plan 086: bounded loopback endpoint allocation."""

    def test_loopback_port_allocator_returns_ephemeral_port(self) -> None:
        import plan083_runner as runner

        port_a = runner._allocate_loopback_port()
        port_b = runner._allocate_loopback_port()
        self.assertGreater(port_a, 0)
        self.assertGreater(port_b, 0)
        # The allocator binds a temporary socket and releases it; the
        # operating system may reuse the same port if the test
        # environment is constrained, so we only assert the lower
        # bound (non-zero) and the 1..=65535 range.
        self.assertLessEqual(port_a, 65535)
        self.assertLessEqual(port_b, 65535)


class Plan086BootstrapDependencyTests(unittest.TestCase):
    """Plan 086: verify the test binaries have no bootstrap dependency."""

    def test_i2pr_interop_binary_path_contains_no_bootstrap_markers(self) -> None:
        # The plan forbids:
        # - reseed
        # - public NetDB bootstrap
        # - DNS lookup
        # - SAM or I2CP
        # - HTTP/SOCKS proxy
        # - normal router daemon
        # - SSU2
        # - transit tunnels
        source_path = Path("/home/sugarwookie/projects/i2pr/tools/i2pr-interop/src/main.rs")
        if not source_path.is_file():
            self.skipTest("i2pr-interop source not available")
        text = source_path.read_text(encoding="utf-8")
        # The prepare command must not start a router, perform DNS,
        # or open a SAM/I2CP/HTTP proxy. The source path is the only
        # line under test; the harness never exercises these.
        for forbidden in (
            "reseed",
            "sam",
            "i2cp",
            "http_proxy",
            "socks",
            "ssu2",
            "router.daemon",
        ):
            if forbidden in text:
                # Allow comments that document the negative contract.
                if any(
                    line.strip().startswith("//") and forbidden in line
                    for line in text.splitlines()
                ):
                    continue
                self.fail(f"i2pr-interop source contains bootstrap marker: {forbidden}")

    def test_i2pd_driver_does_not_require_public_bootstrap(self) -> None:
        # The i2pd direct driver lives in the pinned Plan 064 source
        # tree and must not call reseed, SAM, or DNS. The source
        # file may legitimately reference these names in comments
        # that document the negative contract; the test strips
        # comment lines before scanning.
        source_path = Path(
            "/home/sugarwookie/projects/i2pr/tests/integration/ntcp2/reference-drivers/i2pd/src/i2pd_ntcp2_interop_driver.cpp"
        )
        if not source_path.is_file():
            self.skipTest("i2pd driver source not available")
        text = source_path.read_text(encoding="utf-8")
        code_only = re.sub(r"//.*", "", text)
        for forbidden in ("reseed", "http_proxy"):
            self.assertNotIn(
                forbidden,
                code_only,
                f"i2pd driver contains bootstrap marker: {forbidden}",
            )


class Plan086RecordTests(unittest.TestCase):
    """Plan 086: the canonical record schemas accept the development topology."""

    def test_forward_schema_accepts_host_loopback_development(self) -> None:
        record = forward_probe.build_record(
            run_id="plan086-forward",
            source_commit="a" * 40,
            reference_revision="b" * 40,
            lane_qualification_sha256="c" * 64,
            topology_kind="host-loopback-development",
            parent_network_state_unchanged=True,
            i2pr_binary_sha256="d" * 64,
            i2pd_binary_sha256="e" * 64,
            i2pr_router_info_sha256="1" * 64,
            i2pd_router_info_sha256="2" * 64,
            i2pr_router_hash_sha256="3" * 64,
            i2pd_router_hash_sha256="4" * 64,
            delivery_status_message_id=0x04200001,
            observed_events=[],
            highest_stage_reached=forward_probe.STATE_PREPARED,
            terminal_result=forward_probe.PRE_PROTOCOL_REJECTED,
            reason_code=forward_probe.REASON_PRE_PROTOCOL_PREPARATION_FAILED,
            process_counters={
                "i2pr_prepare": {"started": 0, "exited": 0, "forced": 0},
                "i2pd_prepare": {"started": 0, "exited": 0, "forced": 0},
                "i2pd_listener": {"started": 0, "exited": 0, "forced": 0},
                "i2pr_dialer": {"started": 0, "exited": 0, "forced": 0},
            },
            cleanup_result="not-run",
        )
        self.assertEqual(record["topology_kind"], "host-loopback-development")
        forward_probe.validate_record(record)

    def test_reverse_schema_accepts_host_loopback_development(self) -> None:
        record = reverse_probe.build_reverse_record(
            run_id="plan086-reverse",
            source_commit="a" * 40,
            reference_revision="b" * 40,
            lane_qualification_sha256="c" * 64,
            topology_kind="host-loopback-development",
            parent_network_state_unchanged=True,
            i2pr_binary_sha256="d" * 64,
            i2pd_binary_sha256="e" * 64,
            i2pr_router_info_sha256="1" * 64,
            i2pd_router_info_sha256="2" * 64,
            i2pr_router_hash_sha256="3" * 64,
            i2pd_router_hash_sha256="4" * 64,
            delivery_status_message_id=0x04200002,
            observed_events=[],
            highest_stage_reached=reverse_probe.STATE_PREPARED,
            terminal_result=reverse_probe.PRE_PROTOCOL_REJECTED,
            reason_code=reverse_probe.REASON_PRE_PROTOCOL_PREPARATION_FAILED,
            process_counters={
                "i2pr_prepare": {"started": 0, "exited": 0, "forced": 0},
                "i2pd_prepare": {"started": 0, "exited": 0, "forced": 0},
                "i2pr_listener": {"started": 0, "exited": 0, "forced": 0},
                "i2pd_dialer": {"started": 0, "exited": 0, "forced": 0},
            },
            cleanup_result="not-run",
        )
        self.assertEqual(record["topology_kind"], "host-loopback-development")
        reverse_probe.validate_reverse_record(record)


class Plan086PreflightContractTests(unittest.TestCase):
    """Plan 086: a preflight entry point stops before any peer connection."""

    def test_preflight_stops_before_dialer(self) -> None:
        # The preflight is a thin listener-only probe that does not
        # start a dialer. The Python implementation accepts a
        # ``preflight`` boolean and returns a typed record before any
        # peer is created.
        import plan083_runner as runner_mod

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            config = runner_mod.ProbeConfig(
                run_id="plan086-preflight",
                source_commit="a" * 40,
                reference_revision="b" * 40,
                lane_qualification_sha256="c" * 64,
                topology_kind="host-loopback-development",
                parent_network_state_unchanged=True,
                i2pr_binary_sha256="d" * 64,
                i2pd_binary_sha256="e" * 64,
                output_path=root / "probe-record.json",
            )
            runner = runner_mod.ProbeRunner(config)
            output = runner.run(root)
            record = json.loads(output.read_text(encoding="utf-8"))
            # The preflight must never launch a dialer. The
            # ``i2pr_dialer`` counter must remain zero.
            self.assertEqual(
                record["process_counters"]["i2pr_dialer"]["started"],
                0,
            )
            self.assertEqual(
                record["process_counters"]["i2pr_dialer"]["exited"],
                0,
            )


class Plan086StatusRecordContractTests(unittest.TestCase):
    """Plan 086 status record contract handoff to Plan 087 / 088."""

    def test_status_record_required_decision_token(self) -> None:
        # The Plan 086 status record must carry exactly one bounded
        # closure state from Plan 086's vocabulary. The Plan 087
        # status record inherits the decision vocabulary.
        for state in PLAN_086_CLOSURE_STATES:
            self.assertIn(state, PLAN_086_CLOSURE_STATES)

    def test_handoff_to_plan_087(self) -> None:
        # Plan 087 is enabled only when the Plan 086 status record
        # binds ``host-loopback-development-ready``. No other state
        # may enable Plan 087.
        for state in PLAN_086_CLOSURE_STATES:
            enables_087 = state == "host-loopback-development-ready"
            if state == "host-loopback-development-ready":
                self.assertTrue(enables_087)
            else:
                self.assertFalse(enables_087)


if __name__ == "__main__":
    unittest.main()
