from __future__ import annotations

import importlib.util
from dataclasses import replace
from pathlib import Path
import sys
import unittest


SCRIPT = Path(__file__).with_name("plan476_executor.py")
SPEC = importlib.util.spec_from_file_location("plan476_executor", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
plan476 = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = plan476
SPEC.loader.exec_module(plan476)


def auth(role: str, marker: str) -> object:
    return plan476.HostAuthorization(
        role=role,
        host_digest=plan476.digest_label(f"host-{marker}"),
        router_info_digest=plan476.digest_label(f"ri-{marker}"),
        binary_digest=plan476.digest_label("pinned-i2pd" if role == "i2pd" else "i2pr"),
        endpoint_digest=plan476.digest_label(f"endpoint-{marker}"),
        network_id="2", consent=True,
        run_epoch=plan476.digest_label("epoch-1"),
    )


class Plan476ExecutorTests(unittest.TestCase):
    def setUp(self) -> None:
        self.hosts = (auth("i2pr", "a"), auth("i2pd", "b"))

    def test_simulator_reconciles_two_complete_independent_receipts(self) -> None:
        adapter = plan476.FakeReceiptAdapter(self.hosts)
        receipts = adapter.run()
        result = plan476.reconcile_receipts(self.hosts, receipts)
        self.assertEqual(result["result"], "simulated-reconciliation-passed")
        self.assertEqual(result["independent_roles"], 2)
        self.assertEqual(result["receipt_count"], 14)
        self.assertEqual(result["packet_count"], 10)
        self.assertEqual(result["live_qualification"], "not-run")

    def test_missing_peer_receipt_fails(self) -> None:
        receipts = plan476.FakeReceiptAdapter(self.hosts).run()
        with self.assertRaisesRegex(plan476.ExecutorError, "independent-peer-receipt-required"):
            plan476.reconcile_receipts(self.hosts, tuple(row for row in receipts if row.role == "i2pr"))

    def test_authorization_manifest_negative_matrix(self) -> None:
        cases = (
            ((self.hosts[0], self.hosts[0]), "host-pair-invalid"),
            ((self.hosts[0],), "host-pair-invalid"),
            ((self.hosts[0], replace(self.hosts[1], consent=False)), "authorization-revoked"),
            ((self.hosts[0], replace(self.hosts[1], network_id="3")), "network-id-mismatch"),
            ((self.hosts[0], replace(self.hosts[1], host_digest=self.hosts[0].host_digest)), "host-identities-not-distinct"),
            ((self.hosts[0], replace(self.hosts[1], router_info_digest=self.hosts[0].router_info_digest)), "router-identities-not-distinct"),
            ((self.hosts[0], replace(self.hosts[1], endpoint_digest=self.hosts[0].endpoint_digest)), "host-endpoints-collide"),
            ((self.hosts[0], replace(self.hosts[1], run_epoch=plan476.digest_label("epoch-2"))), "run-epoch-mismatch"),
            ((self.hosts[0], replace(self.hosts[1], host_digest="bad")), "authorization-digest-invalid"),
        )
        for hosts, expected in cases:
            with self.subTest(expected=expected):
                with self.assertRaisesRegex(plan476.ExecutorError, expected):
                    plan476.validate_authorizations(hosts)

    def test_receipt_negative_matrix(self) -> None:
        receipts = plan476.FakeReceiptAdapter(self.hosts).run()
        cases = (
            (tuple(replace(row, run_epoch=plan476.digest_label("stale")) if row.role == "i2pd" else row for row in receipts), "run-epoch-mismatch"),
            (tuple(replace(row, endpoint_digest=self.hosts[0].endpoint_digest) if row.role == "i2pd" else row for row in receipts), "receipt-authorization-mismatch"),
            (tuple(replace(row, sequence=0) if row.role == "i2pd" and row.sequence == 1 else row for row in receipts), "receipt-replay"),
            (tuple(replace(row, phase="unknown") if row.role == "i2pd" and row.sequence == 2 else row for row in receipts), "receipt-phase-invalid"),
            (tuple(replace(row, packet_count=-1) if row.role == "i2pd" and row.sequence == 2 else row for row in receipts), "receipt-count-invalid"),
            (tuple(replace(row, packet_count=0) if row.role == "i2pd" and row.sequence == 2 else row for row in receipts), "authenticated-session-unobserved"),
            (tuple(replace(row, phase="router_started") if row.role == "i2pd" and row.sequence == 3 else row for row in receipts), "receipt-phase-regression"),
        )
        for candidate, expected in cases:
            with self.subTest(expected=expected):
                with self.assertRaisesRegex(plan476.ExecutorError, expected):
                    plan476.reconcile_receipts(self.hosts, candidate)


if __name__ == "__main__":
    unittest.main()
