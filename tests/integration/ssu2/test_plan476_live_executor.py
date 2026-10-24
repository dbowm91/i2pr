#!/usr/bin/env python3
"""Offline tests for the Plan 476 guarded per-host executor.

Nothing here starts a process or opens a socket: every path that would
spawn the local router is driven through the fake process adapter, and the
host probe is faked too. The only files this suite creates live inside one
``tempfile.TemporaryDirectory``.

This module deliberately does not ``import socket``, ``import subprocess``,
``import asyncio``, ``import ssl`` or ``import http``: a simulator that can
connect or spawn cannot be evidence of anything, and
``scripts/check-plan476-gate-binding.py`` enforces that.
"""

from __future__ import annotations

import contextlib
import importlib.util
import io
import json
from pathlib import Path
import re
import sys
import tempfile
import unittest


HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[2]


def _load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


executor = _load("plan476_live_executor", HERE / "plan476_live_executor.py")
contract = _load("plan476_contract", HERE / "plan476_executor.py")
gate = _load("plan495_peer_eligibility", REPO_ROOT / "scripts/plan495-peer-eligibility.py")

FIXTURES = HERE / "plan495-eligibility-fixtures"
RUST_PHASES = REPO_ROOT / "crates/i2pr-daemon/src/ssu2_qualification_receipts.rs"


def write_json(path: Path, document: dict, mode: int = 0o600) -> Path:
    path.write_text(json.dumps(document, sort_keys=True), encoding="utf-8")
    path.chmod(mode)
    return path


def write_bytes(path: Path, payload: bytes) -> Path:
    path.write_bytes(payload)
    path.chmod(0o600)
    return path


class ExecutorTestCase(unittest.TestCase):
    """One temporary lab pair, fakes only."""

    def setUp(self) -> None:
        self._temporary = tempfile.TemporaryDirectory()
        self.base = Path(self._temporary.name)
        self.addCleanup(self._temporary.cleanup)
        # The owner inventory must live outside the repository.
        self.assertFalse(str(self.base).startswith(str(REPO_ROOT)))

        self.manifest = write_json(self.base / "inventory.json", self.inventory())
        self.daemon_binary = write_bytes(self.base / "i2pr-daemon", b"daemon")
        self.peer_binary = write_bytes(self.base / "i2pd", b"stock-peer")
        self.daemon_config = write_bytes(self.base / "daemon.toml", b"[ssu2]\nenabled = true\n")
        self.router_info = write_bytes(self.base / "routerInfo.dat", b"local-router-info")
        self.peer_router_info = write_bytes(
            self.base / "peer-routerInfo.dat", b"peer-router-info"
        )
        self.hygiene = write_json(self.base / "hygiene.json", self.hygiene_record())

    def inventory(self) -> dict:
        return {
            "schema": "i2pr-plan449-host-inventory-v1",
            "reference_revision": "635b013a612ff47278ef02acf8580a28e10e26c5",
            "consent": {
                "owner_authorized": True,
                "scope": "isolated-private-subnet-only",
            },
            "attempt_budget": 1,
            "timeout_seconds": 30,
            "hosts": [
                {
                    "role": "i2pr",
                    "host_id": "0" * 32,
                    "interface": "lan0",
                    "address": "192.168.44.11",
                    "prefix": 24,
                    "udp_port": 19001,
                    "data_dir": str(self.base / "i2pr-data"),
                },
                {
                    "role": "i2pd",
                    "host_id": "1" * 32,
                    "interface": "lan0",
                    "address": "192.168.44.12",
                    "prefix": 24,
                    "udp_port": 19002,
                    "data_dir": str(self.base / "i2pd-data"),
                },
            ],
        }

    def hygiene_record(self) -> dict:
        return {
            "schema": "i2pr-plan495-peer-state-v1",
            "peer_data_dir_fresh": True,
            "peer_process_fresh": True,
            "no_stale_netdb_files": True,
            "stale_netdb_file_count": 0,
            "static_key_matches_bound_port": True,
            "advertised_endpoint_matches_bound_socket": True,
        }

    def local_receipt(self, **overrides) -> dict:
        document = {
            "schema": "i2pr-plan476-local-receipt-v1",
            "profile": "private_lan_qualification",
            "phases": [
                {"phase": "permit_checked", "count": 1},
                {"phase": "router_started", "count": 1},
                {"phase": "peer_session_accepted", "count": 1},
                {"phase": "peer_session_initiated", "count": 1},
                {"phase": "peer_authenticated", "count": 2},
                {"phase": "request_sent", "count": 1},
                {"phase": "request_received", "count": 1},
            ],
            "session_observed": True,
            "product_observed": False,
            "reply_correlated": False,
            "live_qualification": "not-run",
            "error": None,
        }
        document.update(overrides)
        return document

    def request(self, **overrides) -> "executor.RunRequest":
        values = {
            "role": "i2pr",
            "manifest": self.manifest,
            "profile": FIXTURES / "lab-profile-i2pd.json",
            "peer_config": FIXTURES / "peer-config-i2pd.json",
            "epoch": "0" * 32,
            "out": self.base / "receipt.json",
            "router_info": self.router_info,
            "peer_router_info": self.peer_router_info,
            "hygiene": self.hygiene,
            "daemon_binary": self.daemon_binary,
            "daemon_config": self.daemon_config,
            "daemon_receipt": self.base / "daemon-receipt.json",
            "peer_binary": self.peer_binary,
        }
        values.update(overrides)
        return executor.RunRequest(**values)

    def adapters(self, receipt: dict | None = None, host=None) -> "executor.ExecutorAdapters":
        return executor.ExecutorAdapters(
            processes=executor.FakeProcessAdapter(),
            receipts=executor.FakeReceiptCollector(
                self.local_receipt() if receipt is None else receipt
            ),
            host=executor.FakeHostProbe() if host is None else host,
        )

    def host_receipt(self, role: str, **overrides) -> dict:
        document = {
            "schema": "i2pr-plan476-host-receipt-v1",
            "role": role,
            "run_epoch": "a" * 64,
            "host_digest": "b" * 64 if role == "i2pr" else "1" * 64,
            "peer_router_info_digest": "c" * 64 if role == "i2pr" else "d" * 64,
            "local_router_info_digest": "d" * 64 if role == "i2pr" else "c" * 64,
            "binary_digest": "e" * 64 if role == "i2pr" else "f" * 64,
            "endpoint_digest": "0" * 64 if role == "i2pr" else "9" * 64,
            "network_id": "2",
            "phases": [],
            "session_observed": False,
            "product_observed": False,
            "reply_correlated": False,
            "peer_ri_persisted": role == "i2pd",
            "live_qualification": "not-run",
        }
        document.update(overrides)
        return document


class PhaseVocabularyTests(ExecutorTestCase):
    def test_the_vocabulary_matches_the_contract_and_the_daemon_export(self) -> None:
        self.assertEqual(executor.local_phases(), contract.LOCAL_PHASES)
        self.assertEqual(len(contract.LOCAL_PHASES), 9)
        source = RUST_PHASES.read_text(encoding="utf-8")
        body = source[
            source.index("pub const LOCAL_PHASES") : source.index("];", source.index("pub const LOCAL_PHASES"))
        ]
        rust_phases = tuple(re.findall(r'"([^"]+)"', body))
        self.assertEqual(rust_phases, contract.LOCAL_PHASES)

    def test_the_vocabulary_is_the_canonical_nine(self) -> None:
        self.assertEqual(
            contract.LOCAL_PHASES,
            (
                "permit_checked",
                "router_started",
                "peer_session_accepted",
                "peer_session_initiated",
                "peer_authenticated",
                "request_sent",
                "request_received",
                "product_effect_observed",
                "router_stopped",
            ),
        )


class RunTests(ExecutorTestCase):
    def test_a_lane_ready_run_starts_exactly_one_process_and_emits_a_receipt(self) -> None:
        adapters = self.adapters()
        outcome = executor.execute_run(self.request(), adapters)
        self.assertEqual(outcome["result"], "host-receipt-written")
        self.assertEqual(len(adapters.processes.spawned), 1)
        self.assertEqual(
            adapters.processes.spawned[0],
            (str(self.daemon_binary), "run", "--config", str(self.daemon_config)),
        )
        self.assertEqual(len(adapters.processes.terminated), 1)
        artifact = json.loads((self.base / "receipt.json").read_text(encoding="utf-8"))
        self.assertEqual(artifact["schema"], "i2pr-plan476-host-receipt-v1")
        self.assertEqual(artifact["role"], "i2pr")
        self.assertEqual(artifact["network_id"], "2")
        self.assertEqual(artifact["live_qualification"], "not-run")
        self.assertFalse(artifact["peer_ri_persisted"])
        self.assertTrue(artifact["session_observed"])
        self.assertEqual(artifact["phases"][0], {"phase": "permit_checked", "count": 1})
        self.assertEqual(
            oct((self.base / "receipt.json").stat().st_mode & 0o777), "0o600"
        )

    def test_the_artifact_carries_no_address_interface_or_host_identity(self) -> None:
        executor.execute_run(self.request(), self.adapters())
        body = (self.base / "receipt.json").read_text(encoding="utf-8")
        for forbidden in ("192.168.44", "lan0", "0" * 32, str(self.base)):
            self.assertNotIn(forbidden, body)

    def test_the_gate_refusal_starts_no_process(self) -> None:
        adapters = self.adapters()
        with self.assertRaises(executor.ExecutorFailure) as caught:
            executor.execute_run(
                self.request(profile=FIXTURES / "negative-mismatched-netid.json"), adapters
            )
        self.assertEqual(caught.exception.code, "admission-predicate-unsatisfied")
        self.assertEqual(caught.exception.outcome_class, "peer-admission-refused")
        self.assertEqual(caught.exception.cause, "i2pr-lab-profile")
        self.assertEqual(adapters.processes.spawned, [])
        self.assertFalse((self.base / "receipt.json").exists())

    def test_an_unknown_predicate_refuses_the_lane(self) -> None:
        unknown = json.loads((FIXTURES / "lab-profile-i2pd.json").read_text(encoding="utf-8"))
        unknown["signature_valid"] = None
        profile = write_json(self.base / "unknown-profile.json", unknown)
        adapters = self.adapters()
        with self.assertRaises(executor.ExecutorFailure) as caught:
            executor.execute_run(self.request(profile=profile), adapters)
        self.assertEqual(caught.exception.outcome_class, "environment-unavailable")
        self.assertEqual(caught.exception.cause, "unattributed")
        self.assertEqual(adapters.processes.spawned, [])

    def test_the_hygiene_refusal_starts_no_process(self) -> None:
        cases = (
            (executor.FakeHostProbe(stale_files=4), "peer-state-poisoned"),
            (executor.FakeHostProbe(data_directory_usable=False), "peer-state-poisoned"),
            (executor.FakeHostProbe(endpoint_free=False), "peer-state-poisoned"),
        )
        for host, expected in cases:
            with self.subTest(expected=expected):
                adapters = self.adapters(host=host)
                with self.assertRaises(executor.ExecutorFailure) as caught:
                    executor.execute_run(self.request(), adapters)
                self.assertEqual(caught.exception.outcome_class, expected)
                self.assertEqual(caught.exception.cause, "peer-state")
                self.assertEqual(adapters.processes.spawned, [])

    def test_a_missing_operator_hygiene_record_refuses(self) -> None:
        adapters = self.adapters()
        with self.assertRaises(executor.ExecutorFailure) as caught:
            executor.execute_run(self.request(hygiene=None), adapters)
        self.assertEqual(caught.exception.outcome_class, "peer-state-poisoned")
        self.assertEqual(adapters.processes.spawned, [])

    def test_the_withholding_control_failure_starts_no_process(self) -> None:
        both_routes = json.loads((FIXTURES / "lab-profile-i2pd.json").read_text(encoding="utf-8"))
        both_routes["published_addresses"] = [
            dict(row, address_caps="46B") for row in both_routes["published_addresses"]
        ]
        profile = write_json(self.base / "both-routes.json", both_routes)
        adapters = self.adapters()
        with self.assertRaises(executor.ExecutorFailure) as caught:
            executor.execute_run(self.request(profile=profile), adapters)
        self.assertEqual(caught.exception.code, "withholding-control-failed")
        self.assertEqual(adapters.processes.spawned, [])

    def test_the_withholding_control_holds_for_the_canonical_lab_profile(self) -> None:
        self.assertTrue(
            executor.withholding_control_holds(
                gate.validate_profile(gate.load_json(FIXTURES / "lab-profile-i2pd.json")),
                gate.validate_peer(gate.load_json(FIXTURES / "peer-config-i2pd.json")),
                gate,
            )
        )

    def test_a_revoked_authorization_emits_no_receipt(self) -> None:
        inventory = self.inventory()
        inventory["consent"]["owner_authorized"] = False
        write_json(self.manifest, inventory)
        adapters = self.adapters()
        with self.assertRaises(executor.ExecutorFailure) as caught:
            executor.execute_run(self.request(), adapters)
        self.assertEqual(caught.exception.code, "authorization-missing")
        self.assertEqual(adapters.processes.spawned, [])

    def test_a_missing_router_info_refuses_before_anything_starts(self) -> None:
        adapters = self.adapters()
        with self.assertRaises(executor.ExecutorFailure) as caught:
            executor.execute_run(self.request(router_info=None), adapters)
        self.assertEqual(caught.exception.code, "router-info-unavailable")
        self.assertEqual(adapters.processes.spawned, [])

    def test_a_missing_peer_binary_refuses_before_anything_starts(self) -> None:
        adapters = self.adapters()
        with self.assertRaises(executor.ExecutorFailure) as caught:
            executor.execute_run(self.request(peer_binary=None), adapters)
        self.assertEqual(caught.exception.code, "router-binary-unavailable")
        self.assertEqual(adapters.processes.spawned, [])

    def test_the_receipt_path_is_read_from_the_daemon_configuration(self) -> None:
        receipt = self.base / "configured-receipt.json"
        write_bytes(
            self.daemon_config,
            b'[ssu2]\nenabled = true\nqualification_receipt_path = "'
            + str(receipt).encode("utf-8")
            + b'"\n',
        )
        adapters = self.adapters()
        executor.execute_run(self.request(daemon_receipt=None), adapters)
        self.assertEqual(adapters.receipts.paths, [receipt])

    def test_a_daemon_configuration_without_a_receipt_path_refuses(self) -> None:
        adapters = self.adapters()
        with self.assertRaises(executor.ExecutorFailure) as caught:
            executor.execute_run(self.request(daemon_receipt=None), adapters)
        self.assertEqual(caught.exception.code, "local-receipt-unavailable")
        self.assertEqual(adapters.processes.spawned, [])

    def test_a_malformed_epoch_refuses(self) -> None:
        adapters = self.adapters()
        with self.assertRaises(executor.ExecutorFailure) as caught:
            executor.execute_run(self.request(epoch="not-hex"), adapters)
        self.assertEqual(caught.exception.code, "epoch-invalid")
        self.assertEqual(adapters.processes.spawned, [])

    def test_an_open_inventory_refuses(self) -> None:
        self.manifest.chmod(0o644)
        adapters = self.adapters()
        with self.assertRaises(executor.ExecutorFailure) as caught:
            executor.execute_run(self.request(), adapters)
        self.assertEqual(caught.exception.code, "inventory-permissions-too-open")
        self.assertEqual(adapters.processes.spawned, [])


class LocalReceiptTests(ExecutorTestCase):
    def test_a_tampered_local_receipt_fails_the_collection_closed(self) -> None:
        cases = (
            ({"schema": "i2pr-plan476-local-receipt-v0"}, "local-receipt-schema-invalid"),
            ({"error": "receipt-overflow"}, "receipt-overflow"),
            ({"error": "permit-consent-revoked"}, "permit-consent-revoked"),
            ({"phases": [{"phase": "invented_stage", "count": 1}]}, "local-receipt-phase-invalid"),
            (
                {"phases": [{"phase": "request_received", "count": 1}, {"phase": "permit_checked", "count": 1}]},
                "local-receipt-phase-order-invalid",
            ),
            ({"phases": [{"phase": "permit_checked", "count": -1}]}, "local-receipt-count-invalid"),
            ({"live_qualification": "passed"}, "local-receipt-qualification-invalid"),
            ({"session_observed": "yes"}, "local-receipt-flag-invalid"),
        )
        for overrides, expected in cases:
            with self.subTest(expected=expected):
                adapters = self.adapters(receipt=self.local_receipt(**overrides))
                with self.assertRaises(executor.ExecutorFailure) as caught:
                    executor.execute_run(self.request(), adapters)
                self.assertEqual(caught.exception.code, expected)
                self.assertFalse((self.base / "receipt.json").exists())

    def test_an_incomplete_local_observation_fails_the_collection_closed(self) -> None:
        receipt = self.local_receipt()
        receipt["phases"] = [row for row in receipt["phases"] if row["phase"] != "request_received"]
        adapters = self.adapters(receipt=receipt)
        with self.assertRaises(executor.ExecutorFailure) as caught:
            executor.execute_run(self.request(), adapters)
        self.assertEqual(caught.exception.code, "receipt-sequence-incomplete")
        self.assertFalse((self.base / "receipt.json").exists())


class ReconcileTests(ExecutorTestCase):
    def pair(self, **overrides) -> tuple[Path, Path]:
        left = write_json(self.base / "left.json", self.host_receipt("i2pr", **overrides))
        right = write_json(
            self.base / "right.json",
            self.host_receipt(
                "i2pd",
                peer_router_info_digest="d" * 64,
                local_router_info_digest="c" * 64,
            ),
        )
        return left, right

    def test_a_joined_pair_writes_validated_evidence(self) -> None:
        left = write_json(
            self.base / "left.json",
            self.host_receipt(
                "i2pr",
                phases=[
                    {"phase": "permit_checked", "count": 1},
                    {"phase": "peer_session_accepted", "count": 1},
                    {"phase": "peer_session_initiated", "count": 1},
                    {"phase": "peer_authenticated", "count": 2},
                    {"phase": "request_sent", "count": 1},
                    {"phase": "request_received", "count": 1},
                ],
                session_observed=True,
            ),
        )
        right = write_json(
            self.base / "right.json",
            self.host_receipt(
                "i2pd",
                peer_router_info_digest="d" * 64,
                local_router_info_digest="c" * 64,
            ),
        )
        out = self.base / "evidence.json"
        result = executor.reconcile_receipts([left, right], out)
        self.assertEqual(result["result"], "lane-evidence-written")
        evidence = json.loads(out.read_text(encoding="utf-8"))
        self.assertEqual(evidence["schema"], "i2pr-plan495-lane-evidence-v1")
        self.assertEqual(gate.validate_evidence(evidence), [])
        self.assertEqual([row["tier"] for row in evidence["tiers"]], list(range(9)))
        states = {row["tier"]: row["state"] for row in evidence["tiers"]}
        self.assertEqual(states[0], "pass")
        self.assertEqual(states[2], "pass")
        self.assertEqual(states[3], "pass")
        self.assertEqual(states[4], "not-attempted")
        self.assertEqual(states[7], "not-attempted")
        self.assertEqual(states[8], "not-attempted")
        self.assertEqual(evidence["outcome"], "pass")
        self.assertEqual(evidence["cause"], None)
        self.assertEqual(evidence["attempts_consumed"], 1)
        self.assertTrue(evidence["attempts"][0]["consumes_budget"])
        self.assertEqual(oct(out.stat().st_mode & 0o777), "0o600")

    def test_a_silent_pair_is_a_timeout_and_not_a_pass(self) -> None:
        left = write_json(self.base / "left.json", self.host_receipt("i2pr"))
        right = write_json(
            self.base / "right.json",
            self.host_receipt(
                "i2pd",
                peer_router_info_digest="d" * 64,
                local_router_info_digest="c" * 64,
                peer_ri_persisted=False,
            ),
        )
        out = self.base / "evidence.json"
        executor.reconcile_receipts([left, right], out)
        evidence = json.loads(out.read_text(encoding="utf-8"))
        self.assertEqual(evidence["outcome"], "timeout")
        self.assertEqual(evidence["cause"], "unattributed")
        states = {row["tier"]: row["state"] for row in evidence["tiers"]}
        self.assertEqual(states[0], "fail")
        self.assertEqual(states[1], "not-attempted")
        self.assertEqual(gate.validate_evidence(evidence), [])

    def test_reconcile_rejects_unusable_pairs(self) -> None:
        left, right = self.pair()
        stale = write_json(
            self.base / "stale.json", self.host_receipt("i2pd", run_epoch="0" * 64)
        )
        duplicate = write_json(self.base / "duplicate.json", self.host_receipt("i2pr"))
        unknown_role = write_json(
            self.base / "unknown.json", dict(self.host_receipt("i2pr"), role="i2px")
        )
        same_host = write_json(
            self.base / "same-host.json", self.host_receipt("i2pd", host_digest="b" * 64)
        )
        same_endpoint = write_json(
            self.base / "same-endpoint.json", self.host_receipt("i2pd", endpoint_digest="0" * 64)
        )
        wrong_peer = write_json(
            self.base / "wrong-peer.json",
            self.host_receipt("i2pd", peer_router_info_digest="c" * 64, local_router_info_digest="c" * 64),
        )
        wrong_network = write_json(
            self.base / "wrong-network.json", self.host_receipt("i2pd", network_id="3")
        )
        out = self.base / "evidence.json"
        cases = (
            ([left, stale], "run-epoch-mismatch"),
            ([left, duplicate], "receipt-role-invalid"),
            ([left], "independent-peer-receipt-required"),
            ([left, unknown_role], "receipt-role-invalid"),
            ([left, same_host], "host-identities-not-distinct"),
            ([left, same_endpoint], "host-endpoints-collide"),
            ([left, wrong_peer], "peer-router-info-mismatch"),
            ([left, wrong_network], "network-id-mismatch"),
        )
        for receipts, expected in cases:
            with self.subTest(expected=expected):
                with self.assertRaises(executor.ExecutorFailure) as caught:
                    executor.reconcile_receipts(receipts, out)
                self.assertEqual(caught.exception.code, expected)
                self.assertFalse(out.exists())

    def test_reconcile_rejects_a_tampered_host_receipt(self) -> None:
        cases = (
            ({"schema": "i2pr-plan476-host-receipt-v0"}, "receipt-schema-invalid"),
            ({"run_epoch": "short"}, "receipt-digest-invalid"),
            ({"network_id": "3"}, "network-id-mismatch"),
            ({"live_qualification": "passed"}, "receipt-qualification-invalid"),
            ({"session_observed": "yes"}, "receipt-flag-invalid"),
            ({"phases": [{"phase": "invented_stage", "count": 1}]}, "receipt-phase-invalid"),
            (
                {"phases": [{"phase": "request_received", "count": 1}, {"phase": "permit_checked", "count": 1}]},
                "receipt-phase-order-invalid",
            ),
            ({"phases": [{"phase": "permit_checked", "count": -1}]}, "receipt-count-invalid"),
        )
        out = self.base / "evidence.json"
        for overrides, expected in cases:
            with self.subTest(expected=expected):
                left = write_json(
                    self.base / "left.json", self.host_receipt("i2pr", **overrides)
                )
                right = write_json(
                    self.base / "right.json",
                    self.host_receipt(
                        "i2pd",
                        peer_router_info_digest="d" * 64,
                        local_router_info_digest="c" * 64,
                    ),
                )
                with self.assertRaises(executor.ExecutorFailure) as caught:
                    executor.reconcile_receipts([left, right], out)
                self.assertEqual(caught.exception.code, expected)
                self.assertFalse(out.exists())

    def test_the_evidence_validator_rejection_prevents_writing(self) -> None:
        left, right = self.pair()
        out = self.base / "evidence.json"

        class RejectingGate:
            def __init__(self, real: object) -> None:
                self._real = real

            def __getattr__(self, name: str):
                return getattr(self._real, name)

            def validate_evidence(self, record: dict) -> list[str]:
                return ["evidence-outcome-class-invalid"]

        with self.assertRaises(executor.ExecutorFailure) as caught:
            executor.reconcile_receipts([left, right], out, gate=RejectingGate(gate))
        self.assertEqual(caught.exception.code, "evidence-outcome-class-invalid")
        self.assertFalse(out.exists())


class CommandLineTests(ExecutorTestCase):
    def run_cli(self, argv: list[str]) -> tuple[int, str, str]:
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = executor.main(argv)
        return code, out.getvalue(), err.getvalue()

    def test_the_run_command_without_the_live_flag_refuses(self) -> None:
        code, out, _err = self.run_cli(["run", "--role", "i2pr"])
        self.assertEqual(code, 1)
        self.assertEqual(out.count("\n"), 1)
        payload = json.loads(out)
        self.assertEqual(payload["schema"], "i2pr-plan476-executor-result-v1")
        self.assertEqual(payload["result"], "environment-unavailable")
        self.assertEqual(payload["reason"], "live-flag-required")
        self.assertEqual(payload["live_qualification"], "not-run")

    def test_the_run_command_without_its_inputs_refuses(self) -> None:
        code, out, _err = self.run_cli(
            ["run", "--live", "--role", "i2pr", "--manifest", str(self.manifest)]
        )
        self.assertEqual(code, 1)
        payload = json.loads(out)
        self.assertEqual(payload["reason"], "run-input-missing")

    def test_a_missing_manifest_refuses(self) -> None:
        code, out, _err = self.run_cli(
            [
                "run",
                "--live",
                "--role",
                "i2pr",
                "--profile",
                str(FIXTURES / "lab-profile-i2pd.json"),
                "--peer-config",
                str(FIXTURES / "peer-config-i2pd.json"),
                "--epoch",
                "0" * 32,
                "--out",
                str(self.base / "receipt.json"),
            ]
        )
        self.assertEqual(code, 1)
        payload = json.loads(out)
        self.assertEqual(payload["reason"], "authorization-missing")

    def test_the_reconcile_command_without_two_receipts_refuses(self) -> None:
        code, out, _err = self.run_cli(
            ["reconcile", "--receipt", str(self.base / "left.json"), "--out", str(self.base / "e.json")]
        )
        self.assertEqual(code, 1)
        payload = json.loads(out)
        self.assertEqual(payload["reason"], "independent-peer-receipt-required")

    def test_the_self_test_mode_passes(self) -> None:
        code, out, _err = self.run_cli(["--self-test"])
        self.assertEqual(code, 0)
        self.assertIn("plan476-live-executor-self-test: passed", out)

    def test_the_reconcile_command_joins_two_receipts(self) -> None:
        left = write_json(self.base / "left.json", self.host_receipt("i2pr"))
        right = write_json(
            self.base / "right.json",
            self.host_receipt(
                "i2pd", peer_router_info_digest="d" * 64, local_router_info_digest="c" * 64
            ),
        )
        out = self.base / "evidence.json"
        code, stdout, _err = self.run_cli(
            [
                "reconcile",
                "--receipt",
                str(left),
                "--receipt",
                str(right),
                "--out",
                str(out),
            ]
        )
        self.assertEqual(code, 0)
        payload = json.loads(stdout)
        self.assertEqual(payload["result"], "lane-evidence-written")
        self.assertTrue(out.exists())
        evidence = json.loads(out.read_text(encoding="utf-8"))
        self.assertEqual(gate.validate_evidence(evidence), [])

    def test_a_refusal_never_prints_a_path_an_address_or_a_host_id(self) -> None:
        _code, out, _err = self.run_cli(["run", "--role", "i2pr"])
        for forbidden in ("192.168.44", "lan0", str(self.base), "0" * 32):
            self.assertNotIn(forbidden, out)


class AdapterTests(ExecutorTestCase):
    def test_the_fake_adapters_expose_no_socket_or_process_api(self) -> None:
        collector = executor.FakeReceiptCollector({})
        probe = executor.FakeHostProbe()
        processes = executor.FakeProcessAdapter()
        for adapter in (collector, probe, processes):
            for forbidden in ("socket", "subprocess", "connect", "Popen", "urlopen", "run"):
                if isinstance(adapter, executor.FakeProcessAdapter) and forbidden == "run":
                    continue
                self.assertFalse(hasattr(adapter, forbidden), forbidden)

    def test_the_fake_process_adapter_records_instead_of_starting(self) -> None:
        adapter = executor.FakeProcessAdapter()
        handle = adapter.spawn(["/bin/true"])
        self.assertEqual(adapter.spawned, [("/bin/true",)])
        adapter.terminate(handle)
        self.assertEqual(adapter.terminated, [("/bin/true",)])

    def test_the_i2pd_base32_matches_the_pinned_encoding(self) -> None:
        # A 32-byte RouterHash encodes to 52 characters with no padding.
        encoded = executor.i2pd_base32(bytes(range(32)))
        self.assertEqual(len(encoded), 52)
        self.assertTrue(all(character in executor.I2PD_BASE32_ALPHABET for character in encoded))
        self.assertEqual(
            executor.i2pd_base32(b"\x00" * 32),
            executor.I2PD_BASE32_ALPHABET[0] * 52,
        )

    def test_the_peer_router_info_persistence_check_counts_exactly_one_file(self) -> None:
        import hashlib

        payload = b"peer-router-info"
        name = executor.i2pd_base32(hashlib.sha256(payload).digest())
        netdb = self.base / "netDb"
        bucket = netdb / "rAB"
        bucket.mkdir(parents=True)
        self.assertFalse(executor._peer_router_info_persisted(netdb, name))
        (bucket / f"routerInfo-{name}.dat").write_bytes(b"peer")
        self.assertTrue(executor._peer_router_info_persisted(netdb, name))
        (bucket / f"routerInfo-{name}.dat").unlink()
        (bucket / f"routerInfo-{name}.dat").write_bytes(b"peer")
        (netdb / "rCD").mkdir()
        (netdb / "rCD" / f"routerInfo-{name}.dat").write_bytes(b"peer")
        self.assertFalse(executor._peer_router_info_persisted(netdb, name))


if __name__ == "__main__":
    unittest.main()
