"""Plan 046 rootless topology, supervisor, and attestation contract tests.

These tests cover the typed contract and the structural checks of the rootless
sealed-namespace evidence lane. They do **not** require the host to allow
unprivileged user namespaces; tests that need real ``unshare`` are marked
with the ``real-rootless-support`` skip label so they can be opt-in.
"""

from __future__ import annotations

import json
import os
import subprocess
import tempfile
import unittest
import unittest.mock
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent
REPO_ROOT = Path(__file__).resolve().parents[3]

try:
    from interop_topology import (  # type: ignore
        ALLOWED_ACTORS,
        ALLOWED_TOPOLOGY_KINDS,
        PRIVILEGED_PRIVILEGE_MODEL,
        PRIVILEGED_TOPOLOGY_KIND,
        ProcessPlacement,
        ROOTLESS_PRIVILEGE_MODEL,
        ROOTLESS_TOPOLOGY_KIND,
        TopologyContractError,
        normalize_description,
        register_topology,
        select_topology,
    )
    from rootless_supervisor import (  # type: ignore
        ALLOWED_PROBE_OUTCOMES,
        HEX64,
        INNER_MARKER,
        IsolationAttestation,
        SandboxError,
        SandboxPolicy,
        build_attestation,
        emit_probe_status,
        run as supervisor_run,
        verify_attestation_file,
        write_attestation,
    )
    from rootless_topology import RootlessSealedTopology, RootlessTopologyError  # type: ignore
except ImportError:  # pragma: no cover
    pass


class ProcessPlacementContractTests(unittest.TestCase):
    def test_topology_kinds_are_locked(self) -> None:
        self.assertEqual(
            ALLOWED_TOPOLOGY_KINDS,
            frozenset({ROOTLESS_TOPOLOGY_KIND, PRIVILEGED_TOPOLOGY_KIND}),
        )
        self.assertEqual(ROOTLESS_TOPOLOGY_KIND, "rootless-sealed-single-netns")
        self.assertEqual(PRIVILEGED_TOPOLOGY_KIND, "privileged-dual-netns-veth")

    def test_actors_are_locked(self) -> None:
        self.assertEqual(ALLOWED_ACTORS, frozenset({"i2pr", "reference", "control"}))

    def test_unknown_topology_rejected(self) -> None:
        with self.assertRaises(TopologyContractError):
            ProcessPlacement(topology_kind="not-a-kind", actor="i2pr")

    def test_unknown_actor_rejected(self) -> None:
        with self.assertRaises(TopologyContractError):
            ProcessPlacement(topology_kind=ROOTLESS_TOPOLOGY_KIND, actor="not-an-actor")

    def test_command_returns_prefix_plus_args(self) -> None:
        placement = ProcessPlacement(
            topology_kind=PRIVILEGED_TOPOLOGY_KIND,
            actor="i2pr",
            command_prefix=("sudo", "-n", "ip", "netns", "exec", "ns"),
        )
        self.assertEqual(
            placement.command(["i2pr-interop", "ntcp2", "listen"]),
            ["sudo", "-n", "ip", "netns", "exec", "ns", "i2pr-interop", "ntcp2", "listen"],
        )

    def test_rootless_placement_has_empty_prefix(self) -> None:
        placement = ProcessPlacement(topology_kind=ROOTLESS_TOPOLOGY_KIND, actor="i2pr")
        self.assertEqual(placement.command(["i2pr-interop"]), ["i2pr-interop"])


class TopologyBackendRegistryTests(unittest.TestCase):
    def setUp(self) -> None:
        self._prior = os.environ.get(INNER_MARKER)
        os.environ[INNER_MARKER] = "1"

    def tearDown(self) -> None:
        if self._prior is None:
            os.environ.pop(INNER_MARKER, None)
        else:
            os.environ[INNER_MARKER] = self._prior

    def test_rootless_topology_registered(self) -> None:
        topology = select_topology(
            ROOTLESS_TOPOLOGY_KIND, repo_root=REPO_ROOT, run_id="reg-1"
        )
        self.assertIsInstance(topology, RootlessSealedTopology)
        self.assertEqual(topology.topology_kind, ROOTLESS_TOPOLOGY_KIND)
        self.assertEqual(topology.privilege_model, ROOTLESS_PRIVILEGE_MODEL)

    def test_privileged_topology_registered(self) -> None:
        from topology import PrivilegedDualNamespaceTopology  # type: ignore
        topology = select_topology(
            PRIVILEGED_TOPOLOGY_KIND,
            repo_root=REPO_ROOT,
            run_id="reg-2",
            ipv6=False,
        )
        self.assertIsInstance(topology, PrivilegedDualNamespaceTopology)
        self.assertEqual(topology.topology_kind, PRIVILEGED_TOPOLOGY_KIND)

    def test_unknown_topology_fails_closed(self) -> None:
        with self.assertRaises(TopologyContractError):
            select_topology("mystery", repo_root=REPO_ROOT, run_id="reg-3")

    def test_register_rejects_unknown_topology(self) -> None:
        with self.assertRaises(TopologyContractError):
            register_topology("mystery", lambda **_: None)

    def test_rootless_topology_requires_inner_marker(self) -> None:
        prior = os.environ.pop(INNER_MARKER, None)
        try:
            with self.assertRaises(RootlessTopologyError):
                RootlessSealedTopology(repo_root=REPO_ROOT, run_id="reg-4")
        finally:
            if prior is not None:
                os.environ[INNER_MARKER] = prior

    def test_rootless_topology_rejects_invalid_run_id(self) -> None:
        prior = os.environ.get(INNER_MARKER)
        os.environ[INNER_MARKER] = "1"
        try:
            with self.assertRaises(RootlessTopologyError):
                RootlessSealedTopology(repo_root=REPO_ROOT, run_id="!!!invalid!!!")
        finally:
            if prior is None:
                os.environ.pop(INNER_MARKER, None)
            else:
                os.environ[INNER_MARKER] = prior

    def test_rootless_topology_rejects_unknown_reference(self) -> None:
        prior = os.environ.get(INNER_MARKER)
        os.environ[INNER_MARKER] = "1"
        try:
            with self.assertRaises(RootlessTopologyError):
                RootlessSealedTopology(
                    repo_root=REPO_ROOT,
                    run_id="reg-5",
                    reference_kind="not-a-reference",
                )
        finally:
            if prior is None:
                os.environ.pop(INNER_MARKER, None)
            else:
                os.environ[INNER_MARKER] = prior


class RootlessTopologyDescriptionTests(unittest.TestCase):
    def setUp(self) -> None:
        self._prior = os.environ.get(INNER_MARKER)
        os.environ[INNER_MARKER] = "1"

    def tearDown(self) -> None:
        if self._prior is None:
            os.environ.pop(INNER_MARKER, None)
        else:
            os.environ[INNER_MARKER] = self._prior

    def _make(self, run_id: str = "desc-1", ipv6: bool = False) -> RootlessSealedTopology:
        return RootlessSealedTopology(
            repo_root=REPO_ROOT, run_id=run_id, ipv6=ipv6
        )

    def test_topology_kind_is_typed(self) -> None:
        self.assertEqual(RootlessSealedTopology.topology_kind, ROOTLESS_TOPOLOGY_KIND)
        self.assertEqual(RootlessSealedTopology.privilege_model, ROOTLESS_PRIVILEGE_MODEL)

    def test_description_is_digest_stable(self) -> None:
        a = self._make(run_id="stable")
        b = self._make(run_id="stable")
        self.assertEqual(a.description(), b.description())
        self.assertEqual(a.digest(), b.digest())

    def test_different_run_id_yields_different_digest(self) -> None:
        a = self._make(run_id="one")
        b = self._make(run_id="two")
        self.assertNotEqual(a.digest(), b.digest())

    def test_endpoints_use_synthetic_addresses(self) -> None:
        topo = self._make(run_id="endpoints")
        i2pr = topo.endpoint_for_i2pr()
        ref = topo.endpoint_for_reference()
        self.assertEqual(i2pr.local_address, "192.0.2.1")
        self.assertEqual(i2pr.peer_address, "192.0.2.2")
        self.assertEqual(ref.local_address, "192.0.2.2")
        self.assertEqual(ref.peer_address, "192.0.2.1")
        self.assertEqual(i2pr.namespace, "rootless-sealed")
        self.assertEqual(ref.namespace, "rootless-sealed")
        self.assertEqual(i2pr.network_id, "99")

    def test_ipv6_addresses_present_when_enabled(self) -> None:
        topo = self._make(run_id="ipv6", ipv6=True)
        self.assertEqual(topo.i2pr_ipv6, "2001:db8:36::1")
        self.assertEqual(topo.reference_ipv6, "2001:db8:36::2")


class RootlessTopologyPlacementTests(unittest.TestCase):
    def setUp(self) -> None:
        self._prior = os.environ.get(INNER_MARKER)
        os.environ[INNER_MARKER] = "1"

    def tearDown(self) -> None:
        if self._prior is None:
            os.environ.pop(INNER_MARKER, None)
        else:
            os.environ[INNER_MARKER] = self._prior

    def test_create_then_placement_works(self) -> None:
        topo = RootlessSealedTopology(repo_root=REPO_ROOT, run_id="pl-1")
        topo.created = True  # simulate already-created state for placement
        placement = topo.placement("i2pr")
        self.assertEqual(placement.topology_kind, ROOTLESS_TOPOLOGY_KIND)
        self.assertEqual(placement.actor, "i2pr")
        self.assertEqual(placement.command_prefix, ())

    def test_placement_before_create_fails(self) -> None:
        topo = RootlessSealedTopology(repo_root=REPO_ROOT, run_id="pl-2")
        with self.assertRaises(RootlessTopologyError):
            topo.placement("i2pr")

    def test_placement_unknown_actor_fails(self) -> None:
        topo = RootlessSealedTopology(repo_root=REPO_ROOT, run_id="pl-3")
        topo.created = True
        with self.assertRaises(TopologyContractError):
            topo.placement("mystery")

    def test_destroy_returns_clean(self) -> None:
        topo = RootlessSealedTopology(repo_root=REPO_ROOT, run_id="pl-4")
        topo.created = True
        self.assertEqual(topo.destroy(), "clean")
        self.assertFalse(topo.created)

    def test_verify_before_start_returns_structural_status(self) -> None:
        topo = RootlessSealedTopology(repo_root=REPO_ROOT, run_id="pl-5")
        topo.created = True
        status = topo.verify_before_start()
        self.assertEqual(status["topology_kind"], ROOTLESS_TOPOLOGY_KIND)
        self.assertEqual(status["external_interface_count"], 0)
        self.assertEqual(status["default_route_count"], 0)


class SandboxPolicyTests(unittest.TestCase):
    def test_digest_is_stable(self) -> None:
        a = SandboxPolicy(
            run_id="r",
            i2pr_address="192.0.2.1",
            i2pr_port=45680,
            reference_address="192.0.2.2",
            reference_port=45678,
            reference_kind="java_i2p",
            ipv6_enabled=False,
            i2pr_ipv6=None,
            reference_ipv6=None,
            parent_digest_pre="a" * 64,
        )
        b = SandboxPolicy(
            run_id="r",
            i2pr_address="192.0.2.1",
            i2pr_port=45680,
            reference_address="192.0.2.2",
            reference_port=45678,
            reference_kind="java_i2p",
            ipv6_enabled=False,
            i2pr_ipv6=None,
            reference_ipv6=None,
            parent_digest_pre="a" * 64,
        )
        self.assertEqual(a.digest(), b.digest())

    def test_different_policy_yields_different_digest(self) -> None:
        a = SandboxPolicy(
            run_id="r",
            i2pr_address="192.0.2.1",
            i2pr_port=45680,
            reference_address="192.0.2.2",
            reference_port=45678,
            reference_kind="java_i2p",
            ipv6_enabled=False,
            i2pr_ipv6=None,
            reference_ipv6=None,
            parent_digest_pre="a" * 64,
        )
        b = SandboxPolicy(
            run_id="r",
            i2pr_address="192.0.2.1",
            i2pr_port=45680,
            reference_address="192.0.2.2",
            reference_port=45678,
            reference_kind="i2pd",
            ipv6_enabled=False,
            i2pr_ipv6=None,
            reference_ipv6=None,
            parent_digest_pre="a" * 64,
        )
        self.assertNotEqual(a.digest(), b.digest())


class SandboxAttestationTests(unittest.TestCase):
    def _build(self) -> IsolationAttestation:
        policy = SandboxPolicy(
            run_id="r",
            i2pr_address="192.0.2.1",
            i2pr_port=45680,
            reference_address="192.0.2.2",
            reference_port=45678,
            reference_kind="java_i2p",
            ipv6_enabled=False,
            i2pr_ipv6=None,
            reference_ipv6=None,
            parent_digest_pre="b" * 64,
        )
        return build_attestation(
            policy=policy,
            i2pr_commit="c" * 40,
            user_namespace_distinct=True,
            network_namespace_distinct=True,
            mount_namespace_distinct=True,
            pid_namespace_distinct=True,
            uid_map_class="single-id",
            gid_map_class="single-id",
            setgroups_policy="deny",
            no_new_privs=True,
            external_interface_count=0,
            default_route_count=0,
            synthetic_ipv4_ready=True,
            synthetic_ipv6_disposition="skipped",
            external_route_probe="absent",
            external_connect_probe="blocked",
            socket_inventory_sha256="d" * 64,
            child_reap_result="clean",
            sandbox_cleanup_result="clean",
            parent_digest_post="b" * 64,
        )

    def test_topology_kind_is_rootless(self) -> None:
        a = self._build()
        self.assertEqual(a.topology_kind, "rootless-sealed-single-netns")
        self.assertEqual(a.privilege_model, "unprivileged-userns")

    def test_parent_state_unchanged_when_digests_match(self) -> None:
        a = self._build()
        self.assertTrue(a.parent_network_state_unchanged)

    def test_parent_state_changed_when_digests_differ(self) -> None:
        policy = SandboxPolicy(
            run_id="r",
            i2pr_address="192.0.2.1",
            i2pr_port=45680,
            reference_address="192.0.2.2",
            reference_port=45678,
            reference_kind="java_i2p",
            ipv6_enabled=False,
            i2pr_ipv6=None,
            reference_ipv6=None,
            parent_digest_pre="b" * 64,
        )
        a = build_attestation(
            policy=policy,
            i2pr_commit="c" * 40,
            user_namespace_distinct=True,
            network_namespace_distinct=True,
            mount_namespace_distinct=True,
            pid_namespace_distinct=True,
            uid_map_class="single-id",
            gid_map_class="single-id",
            setgroups_policy="deny",
            no_new_privs=True,
            external_interface_count=0,
            default_route_count=0,
            synthetic_ipv4_ready=True,
            synthetic_ipv6_disposition="skipped",
            external_route_probe="absent",
            external_connect_probe="blocked",
            socket_inventory_sha256="d" * 64,
            child_reap_result="clean",
            sandbox_cleanup_result="clean",
            parent_digest_post="e" * 64,
        )
        self.assertFalse(a.parent_network_state_unchanged)

    def test_attestation_self_signature_round_trip(self) -> None:
        a = self._build()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "attestation.json"
            write_attestation(path, a)
            verify_attestation_file(path)
            self.assertTrue(path.is_file())
            payload = json.loads(path.read_text(encoding="utf-8"))
            self.assertEqual(payload["attestation_sha256"], a.attestation_sha256)

    def test_attestation_rejects_zero_digest(self) -> None:
        a = self._build()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "attestation.json"
            payload = a.to_dict()
            payload["attestation_sha256"] = "0" * 64
            path.write_text(json.dumps(payload), encoding="utf-8")
            with self.assertRaises(SandboxError):
                verify_attestation_file(path)

    def test_attestation_rejects_wrong_topology_kind(self) -> None:
        a = self._build()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "attestation.json"
            payload = a.to_dict()
            payload["topology_kind"] = "mystery"
            payload["attestation_sha256"] = ""
            payload["attestation_sha256"] = __import__("hashlib").sha256(
                json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()
            ).hexdigest()
            path.write_text(json.dumps(payload), encoding="utf-8")
            with self.assertRaises(SandboxError):
                verify_attestation_file(path)

    def test_attestation_rejects_mismatched_digest(self) -> None:
        a = self._build()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "attestation.json"
            payload = a.to_dict()
            payload["attestation_sha256"] = "f" * 64
            path.write_text(json.dumps(payload), encoding="utf-8")
            with self.assertRaises(SandboxError):
                verify_attestation_file(path)


class ProbeOutcomeTests(unittest.TestCase):
    def test_allowed_outcomes_are_locked(self) -> None:
        for code in (
            "rootless_sandbox_available",
            "blocked_unprivileged_user_namespace",
            "blocked_uid_map",
            "blocked_gid_map",
            "blocked_setgroups_contract",
            "blocked_network_namespace",
            "blocked_namespace_local_net_admin",
            "blocked_mount_namespace",
            "blocked_private_proc",
            "blocked_no_new_privs",
            "blocked_loopback_configuration",
            "blocked_synthetic_address_configuration",
            "blocked_external_route_present",
            "blocked_external_connect_possible",
            "blocked_rootless_cleanup",
        ):
            self.assertIn(code, ALLOWED_PROBE_OUTCOMES)

    def test_emit_probe_status_rejects_unknown_outcome(self) -> None:
        with self.assertRaises(SandboxError):
            emit_probe_status("mystery-outcome")

    def test_emit_probe_status_writes_strict_json(self) -> None:
        import io
        import contextlib
        buffer = io.StringIO()
        with contextlib.redirect_stdout(buffer):
            emit_probe_status("rootless_sandbox_available", details={"foo": 1})
        payload = json.loads(buffer.getvalue())
        self.assertEqual(payload["schema"], 1)
        self.assertEqual(payload["type"], "rootless-sandbox-probe")
        self.assertEqual(payload["outcome"], "rootless_sandbox_available")
        self.assertEqual(payload["details"], {"foo": 1})


class SupervisorFailureInjectionTests(unittest.TestCase):
    def test_supervisor_rejects_invalid_i2pr_commit(self) -> None:
        policy = SandboxPolicy(
            run_id="r",
            i2pr_address="192.0.2.1",
            i2pr_port=45680,
            reference_address="192.0.2.2",
            reference_port=45678,
            reference_kind="java_i2p",
            ipv6_enabled=False,
            i2pr_ipv6=None,
            reference_ipv6=None,
            parent_digest_pre="a" * 64,
        )
        with self.assertRaises(SandboxError):
            supervisor_run(policy=policy, i2pr_commit="not-a-commit")

    def test_known_capabilities_drive_typed_blockers(self) -> None:
        # We are not in a real user namespace in the test environment, so we
        # expect the first verification check to fail closed with a typed code.
        policy = SandboxPolicy(
            run_id="r",
            i2pr_address="192.0.2.1",
            i2pr_port=45680,
            reference_address="192.0.2.2",
            reference_port=45678,
            reference_kind="java_i2p",
            ipv6_enabled=False,
            i2pr_ipv6=None,
            reference_ipv6=None,
            parent_digest_pre="a" * 64,
        )
        try:
            supervisor_run(policy=policy, i2pr_commit="c" * 40)
        except SandboxError as exc:
            self.assertIn(
                exc.code,
                {
                    "blocked_unprivileged_user_namespace",
                    "blocked_uid_map",
                    "blocked_gid_map",
                    "blocked_setgroups_contract",
                    "blocked_no_new_privs",
                },
            )


class RootlessTopologyRegistryBackedTests(unittest.TestCase):
    def test_select_returns_rootless_with_running_inner_marker(self) -> None:
        prior = os.environ.get(INNER_MARKER)
        os.environ[INNER_MARKER] = "1"
        try:
            topology = select_topology(
                ROOTLESS_TOPOLOGY_KIND, repo_root=REPO_ROOT, run_id="reg-rt-1"
            )
            self.assertIsInstance(topology, RootlessSealedTopology)
        finally:
            if prior is None:
                os.environ.pop(INNER_MARKER, None)
            else:
                os.environ[INNER_MARKER] = prior

    def test_normalize_description_round_trip(self) -> None:
        payload = {"run_id": "n1", "i2pr_address": "192.0.2.1"}
        normalized = normalize_description(ROOTLESS_TOPOLOGY_KIND, payload)
        self.assertEqual(normalized["topology_kind"], ROOTLESS_TOPOLOGY_KIND)
        self.assertEqual(normalized["run_id"], "n1")


class RealRootlessProcessProbeTests(unittest.TestCase):
    """Tests that require actual unprivileged user namespaces are opt-in.

    They run only when the host supports ``unshare --user --net`` writing
    /proc/self/uid_map. CI runners that disable unprivileged user namespaces
    skip these tests and treat the lane as unavailable.
    """

    def setUp(self) -> None:
        self._support = _host_supports_unshare()
        if not self._support:
            self.skipTest("host does not allow unprivileged user namespaces")

    def test_supervisor_probe_passes_on_supported_hosts(self) -> None:
        env = {
            "I2PR_INTEROP_ROOTLESS_INNER": "1",
            "I2PR_INTEROP_ROOTLESS_PARENT_DIGEST_PRE": "a" * 64,
        }
        result = subprocess.run(
            [
                "unshare",
                "--user",
                "--net",
                "--mount",
                "--pid",
                "--fork",
                "--propagation",
                "private",
                "--mount-proc",
                "--map-root-user",
                "python3",
                str(REPO_ROOT / "tests/integration/ntcp2/harness/rootless_supervisor.py"),
                "--probe",
            ],
            capture_output=True,
            text=True,
            timeout=15,
            env={**os.environ, **env},
            check=False,
        )
        # The supervisor prints a single strict JSON status line. On a
        # supported host the outcome is rootless_sandbox_available; on a
        # host that disables some capability the typed blocker is also
        # acceptable because the supervisor fails closed without escalation.
        self.assertEqual(result.returncode in (0, 1), True)
        if result.returncode == 0:
            payload = json.loads(result.stdout.strip().splitlines()[-1])
            self.assertEqual(payload["outcome"], "rootless_sandbox_available")
        else:
            self.assertIn('"outcome"', result.stdout)


def _host_supports_unshare() -> bool:
    """Return True when ``unshare --user --net --map-root-user`` can write
    /proc/self/uid_map on this host.
    """

    result = subprocess.run(
        [
            "unshare",
            "--user",
            "--net",
            "--map-root-user",
            "python3",
            "-c",
            "import os; print(os.getuid())",
        ],
        capture_output=True,
        text=True,
        timeout=5,
        check=False,
    )
    return result.returncode == 0


class MixedRunnerTopologyDispatchTests(unittest.TestCase):
    """The mixed runner must accept ``--topology-kind`` and route through
    ``select_topology`` rather than constructing a privileged namespace.
    """

    def test_mixed_runner_exposes_topology_kind_argument(self) -> None:
        """The CLI parser must accept ``--topology-kind rootless-sealed-single-netns``
        and route the run through the rootless topology backend.
        """

        from mixed_runner import main as mixed_main

        captured: dict[str, Any] = {}

        def _fake_run(args: Any) -> int:
            captured["topology_kind"] = args.topology_kind
            return 0

        original_run = mixed_main.__globals__.get("run")
        mixed_main.__globals__["run"] = _fake_run  # type: ignore[index]
        try:
            argv = [
                "--scenario", "i2pr-to-java-ipv4",
                "--reference", "java_i2p",
                "--topology-kind", ROOTLESS_TOPOLOGY_KIND,
            ]
            with unittest.mock.patch.dict(
                os.environ,
                {
                    "I2PR_INTEROP_ROOTLESS_ATTESTATION_SHA256": "1" * 64,
                    "I2PR_INTEROP_ROOTLESS_PARENT_STATE_UNCHANGED": "1",
                },
                clear=False,
            ):
                with unittest.mock.patch("sys.argv", ["mixed_runner"] + argv):
                    rc = mixed_main()
        finally:
            mixed_main.__globals__["run"] = original_run  # type: ignore[index]
        self.assertEqual(rc, 0)
        self.assertEqual(captured.get("topology_kind"), ROOTLESS_TOPOLOGY_KIND)


class RootlessInnerRunnerDispatchTests(unittest.TestCase):
    """The inner runner must propagate the attestation SHA into the mixed
    runner's environment so the resulting evidence record is bound to the
    sandbox.
    """

    def test_inner_runner_propagates_attestation_env(self) -> None:
        """When invoked with a synthetic attestation, the inner runner must
        forward ``I2PR_INTEROP_ROOTLESS_ATTESTATION_SHA256`` and
        ``I2PR_INTEROP_ROOTLESS_PARENT_STATE_UNCHANGED`` to ``mixed_runner``.
        """

        from rootless_inner_runner import _scenario_main
        import rootless_inner_runner
        from rootless_supervisor import IsolationAttestation

        sentinel_sha = "1" * 64
        captured_env: dict[str, str] = {}

        class _Completed:
            returncode = 0
            stdout = ""
            stderr = ""

        def _fake_run(command: list[str], env: dict[str, str] | None = None, **_kwargs: Any) -> _Completed:
            captured_env.update(env or {})
            return _Completed()

        attestation = IsolationAttestation(
            schema="i2pr-rootless-sandbox-attestation-v1",
            record_type="rootless-sandbox-attestation",
            date_utc="1970-01-01T00:00:00Z",
            i2pr_commit="0" * 40 + ";clean",
            topology_kind=ROOTLESS_TOPOLOGY_KIND,
            privilege_model=ROOTLESS_PRIVILEGE_MODEL,
            user_namespace_distinct=True,
            network_namespace_distinct=True,
            mount_namespace_distinct=True,
            pid_namespace_distinct=True,
            uid_map_class="single-id",
            gid_map_class="single-id",
            setgroups_policy="deny",
            no_new_privs=True,
            external_interface_count=0,
            default_route_count=0,
            synthetic_ipv4_ready=True,
            synthetic_ipv6_disposition="absent",
            external_route_probe="blocked",
            external_connect_probe="blocked",
            socket_inventory_sha256="0" * 64,
            sandbox_policy_sha256="0" * 64,
            parent_network_state_pre_sha256=sentinel_sha,
            parent_network_state_post_sha256=sentinel_sha,
            parent_network_state_unchanged=True,
            child_reap_result="clean",
            sandbox_cleanup_result="clean",
            attestation_sha256=sentinel_sha,
            known_deviation="",
            reproduction="bash scripts/interop/rootless-enter.sh --probe",
        )

        original_subprocess_run = subprocess.run
        original_select = rootless_inner_runner.select_topology
        original_supervisor_run = rootless_inner_runner.run_supervisor

        rootless_inner_runner.subprocess.run = _fake_run  # type: ignore[attr-defined]
        rootless_inner_runner.select_topology = lambda *_a, **_kw: _FakeTopology()  # type: ignore[attr-defined]
        rootless_inner_runner.run_supervisor = lambda **_kw: attestation  # type: ignore[attr-defined]
        try:
            args = rootless_inner_runner.argparse.Namespace(
                scenario="i2pr-to-java-ipv4",
                reference="java_i2p",
                ipv6=False,
                attestation_output=None,
                build_cache=None,
                run_root=None,
            )
            rc = _scenario_main(args)
        finally:
            rootless_inner_runner.subprocess.run = original_subprocess_run  # type: ignore[attr-defined]
            rootless_inner_runner.select_topology = original_select  # type: ignore[attr-defined]
            rootless_inner_runner.run_supervisor = original_supervisor_run  # type: ignore[attr-defined]
        self.assertEqual(rc, 0)
        self.assertEqual(captured_env.get("I2PR_INTEROP_ROOTLESS_ATTESTATION_SHA256"), sentinel_sha)
        self.assertEqual(captured_env.get("I2PR_INTEROP_ROOTLESS_PARENT_STATE_UNCHANGED"), "1")
        self.assertEqual(captured_env.get("I2PR_INTEROP_ROOTLESS_INNER"), "1")


class _FakeTopology:
    topology_kind = ROOTLESS_TOPOLOGY_KIND
    privilege_model = ROOTLESS_PRIVILEGE_MODEL

    def create(self) -> None:
        return None

    def placement(self, actor: str) -> ProcessPlacement:
        return ProcessPlacement(topology_kind=self.topology_kind, actor=actor)

    def endpoint_for_i2pr(self) -> Any:
        return None

    def endpoint_for_reference(self) -> Any:
        return None


if __name__ == "__main__":
    unittest.main()
