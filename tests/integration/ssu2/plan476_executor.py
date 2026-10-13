#!/usr/bin/env python3
"""Offline, fail-closed Plan 476 pair receipt state machine.

This module is deliberately transport-free.  Its simulator proves manifest,
epoch, role, digest, phase-order, revocation, and two-endpoint reconciliation
rules without opening sockets or starting either router.
"""

from __future__ import annotations

from dataclasses import dataclass
import hashlib
import re


ROLES = frozenset(("i2pr", "i2pd"))
PHASES = (
    "authorized", "router_started", "peer_authenticated", "request_received",
    "reply_received", "router_stopped", "withdrawal_verified",
)
HEX64 = re.compile(r"[0-9a-f]{64}\Z")


class ExecutorError(ValueError):
    """Sanitized fail-closed executor error."""

    def __init__(self, code: str) -> None:
        super().__init__(code)
        self.code = code


@dataclass(frozen=True)
class HostAuthorization:
    role: str
    host_digest: str
    router_info_digest: str
    binary_digest: str
    endpoint_digest: str
    network_id: str
    consent: bool
    run_epoch: str


@dataclass(frozen=True)
class Receipt:
    role: str
    host_digest: str
    router_info_digest: str
    binary_digest: str
    endpoint_digest: str
    network_id: str
    run_epoch: str
    phase: str
    sequence: int
    packet_count: int


def _digest(value: str) -> bool:
    return isinstance(value, str) and HEX64.fullmatch(value) is not None


def validate_authorizations(hosts: tuple[HostAuthorization, ...]) -> None:
    if len(hosts) != 2 or {host.role for host in hosts} != ROLES:
        raise ExecutorError("host-pair-invalid")
    for host in hosts:
        if not host.consent:
            raise ExecutorError("authorization-revoked")
        if host.network_id != "2":
            raise ExecutorError("network-id-mismatch")
        if not all(_digest(value) for value in (
            host.host_digest, host.router_info_digest, host.binary_digest,
            host.endpoint_digest, host.run_epoch,
        )):
            raise ExecutorError("authorization-digest-invalid")
    if len({host.host_digest for host in hosts}) != 2:
        raise ExecutorError("host-identities-not-distinct")
    if len({host.router_info_digest for host in hosts}) != 2:
        raise ExecutorError("router-identities-not-distinct")
    if len({host.endpoint_digest for host in hosts}) != 2:
        raise ExecutorError("host-endpoints-collide")
    if len({host.run_epoch for host in hosts}) != 1:
        raise ExecutorError("run-epoch-mismatch")
    if len({host.binary_digest for host in hosts if host.role == "i2pd"}) != 1:
        raise ExecutorError("reference-binary-mismatch")


def reconcile_receipts(
    hosts: tuple[HostAuthorization, ...], receipts: tuple[Receipt, ...],
) -> dict[str, object]:
    """Validate two independently captured receipts; return counts only."""
    validate_authorizations(hosts)
    if len(receipts) < 2 or {row.role for row in receipts} != ROLES:
        raise ExecutorError("independent-peer-receipt-required")
    by_role = {host.role: host for host in hosts}
    last_sequence = {role: -1 for role in ROLES}
    seen_phase = {role: -1 for role in ROLES}
    for receipt in receipts:
        auth = by_role[receipt.role]
        if not auth.consent:
            raise ExecutorError("authorization-revoked")
        if receipt.run_epoch != auth.run_epoch:
            raise ExecutorError("run-epoch-mismatch")
        if any(getattr(receipt, field) != getattr(auth, field) for field in (
            "host_digest", "router_info_digest", "binary_digest",
            "endpoint_digest", "network_id",
        )):
            raise ExecutorError("receipt-authorization-mismatch")
        if receipt.phase not in PHASES:
            raise ExecutorError("receipt-phase-invalid")
        if not isinstance(receipt.sequence, int) or isinstance(receipt.sequence, bool) or receipt.sequence < 0:
            raise ExecutorError("receipt-sequence-invalid")
        if receipt.sequence <= last_sequence[receipt.role]:
            raise ExecutorError("receipt-replay")
        if not isinstance(receipt.packet_count, int) or isinstance(receipt.packet_count, bool) or receipt.packet_count < 0:
            raise ExecutorError("receipt-count-invalid")
        index = PHASES.index(receipt.phase)
        if index < seen_phase[receipt.role]:
            raise ExecutorError("receipt-phase-regression")
        if receipt.phase == "peer_authenticated" and receipt.packet_count == 0:
            raise ExecutorError("authenticated-session-unobserved")
        last_sequence[receipt.role] = receipt.sequence
        seen_phase[receipt.role] = index
    phase_counts = {
        phase: sum(row.phase == phase for row in receipts)
        for phase in PHASES
    }
    return {
        "schema": "i2pr-plan476-pair-receipt-v1",
        "result": "simulated-reconciliation-passed",
        "independent_roles": len({row.role for row in receipts}),
        "receipt_count": len(receipts),
        "packet_count": sum(row.packet_count for row in receipts),
        "phase_counts": phase_counts,
        "live_qualification": "not-run",
    }


def digest_label(label: str) -> str:
    """Deterministic simulator-only opaque label digest."""
    return hashlib.sha256(label.encode("utf-8")).hexdigest()


class FakeReceiptAdapter:
    """In-process adapter that has no socket or process API by construction."""

    def __init__(self, hosts: tuple[HostAuthorization, ...]) -> None:
        validate_authorizations(hosts)
        self.hosts = {row.role: row for row in hosts}

    def run(self) -> tuple[Receipt, ...]:
        output = []
        for role in ("i2pr", "i2pd"):
            host = self.hosts[role]
            for sequence, phase, packet_count in (
                (0, "authorized", 0), (1, "router_started", 0),
                (2, "peer_authenticated", 1), (3, "request_received", 1),
                (4, "reply_received", 1), (5, "router_stopped", 1),
                (6, "withdrawal_verified", 1),
            ):
                output.append(Receipt(
                    role=role, host_digest=host.host_digest,
                    router_info_digest=host.router_info_digest,
                    binary_digest=host.binary_digest,
                    endpoint_digest=host.endpoint_digest,
                    network_id=host.network_id, run_epoch=host.run_epoch,
                    phase=phase, sequence=sequence, packet_count=packet_count,
                ))
        return tuple(output)
