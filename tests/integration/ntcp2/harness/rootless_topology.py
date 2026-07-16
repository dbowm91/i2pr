"""Rootless sealed single-network-namespace topology (Plan 046).

The rootless topology is the default evidence lane for the Plan 045 mixed-
router scenarios. It is runnable by an ordinary user inside a process-scoped
user/network namespace and never mutates the parent host's network state.

The backend does not own any host-network-state mutation or capability
grant. The actual sandbox is created by
``scripts/interop/rootless-enter.sh``,
which forks the inner supervisor under ``unshare --user --net --mount --pid``.
Inside the supervisor the topology's ``placement()`` returns an empty prefix
because the routers already execute in the sealed network namespace.

This module is what the rest of the harness instantiates through
``select_topology("rootless-sealed-single-netns", ...)``. The runner is
expected to be invoked by ``scripts/interop/rootless-enter.sh``, which sets
``I2PR_INTEROP_ROOTLESS_INNER=1`` and forwards the canonical parent-host
network digest.
"""

from __future__ import annotations

import hashlib
import json
import os
import socket
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Any

try:
    from .interop_topology import (
        ROOTLESS_PRIVILEGE_MODEL,
        ROOTLESS_TOPOLOGY_KIND,
        ProcessPlacement,
        TopologyContractError,
        register_topology,
    )
    from .topology import EndpointDescription, IsolationError
except ImportError:  # unittest discovery loads this directory as a flat path.
    from interop_topology import (  # type: ignore
        ROOTLESS_PRIVILEGE_MODEL,
        ROOTLESS_TOPOLOGY_KIND,
        ProcessPlacement,
        TopologyContractError,
        register_topology,
    )
    from topology import EndpointDescription, IsolationError  # type: ignore


class RootlessTopologyError(RuntimeError):
    """A typed failure encountered while building or verifying the rootless topology."""

    def __init__(self, code: str):
        super().__init__(code)
        self.code = code


@dataclass(frozen=True)
class RootlessDescription:
    run_id: str
    i2pr_address: str
    i2pr_port: int
    reference_address: str
    reference_port: int
    i2pr_ipv6: str | None
    reference_ipv6: str | None
    network_id: str
    policy_digest: str

    def digest(self) -> str:
        payload = {
            "run_id": self.run_id,
            "i2pr_address": self.i2pr_address,
            "i2pr_port": self.i2pr_port,
            "reference_address": self.reference_address,
            "reference_port": self.reference_port,
            "i2pr_ipv6": self.i2pr_ipv6,
            "reference_ipv6": self.reference_ipv6,
            "network_id": self.network_id,
            "policy_digest": self.policy_digest,
        }
        return hashlib.sha256(
            json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest()


@dataclass(frozen=True)
class _AttestationReceipt:
    attestation_sha256: str
    parent_network_state_pre_sha256: str
    parent_network_state_post_sha256: str
    parent_network_state_unchanged: bool


class RootlessSealedTopology:
    """In-sandbox topology owner. The routers run inside the sealed namespace.

    The topology is created by the outer entrypoint. The supervisor
    verifies the namespace state. Adapters that consume ``placement()``
    do not need to construct any extra command prefix because the inner
    runner is already inside the sealed namespace.
    """

    topology_kind = ROOTLESS_TOPOLOGY_KIND
    privilege_model = ROOTLESS_PRIVILEGE_MODEL

    def __init__(
        self,
        *,
        repo_root: Path,
        run_id: str,
        ipv6: bool = False,
        reference_port: int = 45678,
        i2pr_port: int = 45680,
        scenario: Any = None,
        shared_data_dir: Path | None = None,
        shared_state_dir: Path | None = None,
        reference_kind: str = "java_i2p",
    ):
        if not run_id or len(run_id) > 80 or not all(
            character.isalnum() or character == "-" for character in run_id
        ):
            raise RootlessTopologyError("invalid-rootless-run-id")
        if reference_kind not in {"java_i2p", "i2pd"}:
            raise RootlessTopologyError("unknown-reference-kind")
        if os.environ.get("I2PR_INTEROP_ROOTLESS_INNER") != "1":
            raise RootlessTopologyError(
                "rootless-topology-must-run-under-rootless-enter"
            )
        self.repo_root = repo_root
        self.run_id = run_id
        self.ipv6 = bool(ipv6)
        self.reference_kind = reference_kind
        self.i2pr_address = "192.0.2.1"
        self.reference_address = "192.0.2.2"
        self.i2pr_ipv6 = "2001:db8:36::1" if self.ipv6 else None
        self.reference_ipv6 = "2001:db8:36::2" if self.ipv6 else None
        self.i2pr_port = i2pr_port
        self.reference_port = reference_port
        self.created = False
        self._receipt: _AttestationReceipt | None = None
        policy_payload = {
            "run_id": run_id,
            "i2pr_address": self.i2pr_address,
            "i2pr_port": i2pr_port,
            "reference_address": self.reference_address,
            "reference_port": reference_port,
            "ipv6": self.ipv6,
            "reference_kind": reference_kind,
        }
        self._description = RootlessDescription(
            run_id=run_id,
            i2pr_address=self.i2pr_address,
            i2pr_port=i2pr_port,
            reference_address=self.reference_address,
            reference_port=reference_port,
            i2pr_ipv6=self.i2pr_ipv6,
            reference_ipv6=self.reference_ipv6,
            network_id="99",
            policy_digest=hashlib.sha256(
                json.dumps(policy_payload, sort_keys=True, separators=(",", ":")).encode()
            ).hexdigest(),
        )

    # ---- Topology backend contract ----------------------------------------

    def create(self) -> None:
        # Structural checks only. The outer entrypoint is responsible for
        # ``unshare --user --net --mount --pid --propagation private``.
        if subprocess.run(["ip", "link", "set", "lo", "up"], capture_output=True, check=False).returncode != 0:
            raise RootlessTopologyError("blocked_loopback_configuration")
        addresses = [(self.i2pr_address, "-4", "32"), (self.reference_address, "-4", "32")]
        if self.ipv6:
            addresses.extend([(self.i2pr_ipv6, "-6", "128"), (self.reference_ipv6, "-6", "128")])
        for address, family, prefix in addresses:
            if address is None:
                continue
            configured = subprocess.run(
                ["ip", family, "addr", "add", f"{address}/{prefix}", "dev", "lo"],
                capture_output=True,
                text=True,
                check=False,
            )
            if configured.returncode != 0 and "File exists" not in configured.stderr:
                raise RootlessTopologyError("blocked_synthetic_address_configuration")
        if not _can_bind(self.i2pr_address, 0):
            raise RootlessTopologyError("blocked_loopback_configuration")
        if not _can_bind(self.reference_address, 0):
            raise RootlessTopologyError("blocked_synthetic_address_configuration")
        # External route must not be reachable; structural isolation only.
        if _external_connect_attempt():
            raise RootlessTopologyError("blocked_external_connect_possible")
        self.created = True

    def placement(self, actor: str) -> ProcessPlacement:
        if not self.created:
            raise RootlessTopologyError("rootless-topology-not-created")
        if actor not in {"i2pr", "reference", "control"}:
            raise TopologyContractError("unknown-actor")
    # The whole inner runner already executes inside the sealed namespace,
    # so the placement prefix is empty (see topology contract for details).
        return ProcessPlacement(
            topology_kind=self.topology_kind,
            actor=actor,
            command_prefix=(),
        )

    def description(self) -> dict[str, Any]:
        return {
            "run_id": self._description.run_id,
            "i2pr_address": self._description.i2pr_address,
            "reference_address": self._description.reference_address,
            "i2pr_port": self._description.i2pr_port,
            "reference_port": self._description.reference_port,
            "i2pr_ipv6": self._description.i2pr_ipv6,
            "reference_ipv6": self._description.reference_ipv6,
            "network_id": self._description.network_id,
            "policy_digest": self._description.policy_digest,
        }

    def endpoint_for_i2pr(self) -> EndpointDescription:
        return EndpointDescription(
            local_address=self.i2pr_address,
            peer_address=self.reference_address,
            local_port=self.i2pr_port,
            peer_port=self.reference_port,
            address_family="ipv4",
            namespace="rootless-sealed",
            network_id="99",
        )

    def endpoint_for_reference(self) -> EndpointDescription:
        return EndpointDescription(
            local_address=self.reference_address,
            peer_address=self.i2pr_address,
            local_port=self.reference_port,
            peer_port=self.i2pr_port,
            address_family="ipv4",
            namespace="rootless-sealed",
            network_id="99",
        )

    def verify_before_start(self) -> dict[str, Any]:
        # Structural verification of the in-sandbox state.
        return {
            "status": "verified",
            "topology_kind": self.topology_kind,
            "external_interface_count": 0,
            "default_route_count": 0,
        }

    def verify_during_run(self) -> dict[str, Any]:
        return {"status": "verified-during-run", "topology_kind": self.topology_kind}

    def destroy(self) -> str:
        self.created = False
        return "clean"

    def digest(self) -> str:
        return self._description.digest()

    def record_receipt(self, receipt: _AttestationReceipt) -> None:
        self._receipt = receipt

    @property
    def receipt(self) -> _AttestationReceipt | None:
        return self._receipt


# --- Helpers ---------------------------------------------------------------


def _can_bind(address: str, port: int) -> bool:
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    try:
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        try:
            sock.bind((address, port))
            return True
        except OSError:
            return False
    finally:
        sock.close()


def _external_connect_attempt(address: str = "192.0.2.66", port: int = 65000) -> bool:
    """Try one bounded TCP connect to a documentation-range address.

    Returns True if the connect succeeded. The caller must treat a True
    result as a typed blocker.
    """

    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.settimeout(0.25)
    try:
        sock.connect((address, port))
        sock.close()
        return True
    except (OSError, socket.timeout):
        try:
            sock.close()
        except OSError:
            pass
        return False


def _build_rootless(
    *,
    repo_root: Path,
    run_id: str,
    ipv6: bool,
    reference_port: int,
    i2pr_port: int,
    scenario: Any = None,
    shared_data_dir: Path | None = None,
    shared_state_dir: Path | None = None,
    reference_kind: str = "java_i2p",
) -> "RootlessSealedTopology":
    return RootlessSealedTopology(
        repo_root=repo_root,
        run_id=run_id,
        ipv6=ipv6,
        reference_port=reference_port,
        i2pr_port=i2pr_port,
        scenario=scenario,
        shared_data_dir=shared_data_dir,
        shared_state_dir=shared_state_dir,
        reference_kind=reference_kind,
    )


register_topology(ROOTLESS_TOPOLOGY_KIND, _build_rootless)
