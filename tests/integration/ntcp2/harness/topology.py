"""Privileged dual-network-namespace/veth topology owner.

This module owns the Plan 038/040/044/045 privileged topology. Plan 046
introduces ``rootless-sealed-single-netns`` as the primary evidence lane and
renames the legacy topology to ``privileged-dual-netns-veth``. The legacy
backend remains explicit and opt-in; it is never the default fallback.

Adapters and runners must not import this module directly for process
placement. Use ``interop_topology.select_topology`` and
``ProcessPlacement.command`` instead.
"""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

try:
    from .firewall import canonical_firewall_rules, normalize_ruleset, policy_digest
    from .interop_topology import (
        PRIVILEGED_PRIVILEGE_MODEL,
        PRIVILEGED_TOPOLOGY_KIND,
        ProcessPlacement,
        register_topology,
    )
except ImportError:  # unittest discovery loads this directory as a flat path.
    from firewall import canonical_firewall_rules, normalize_ruleset, policy_digest  # type: ignore
    from interop_topology import (  # type: ignore
        PRIVILEGED_PRIVILEGE_MODEL,
        PRIVILEGED_TOPOLOGY_KIND,
        ProcessPlacement,
        register_topology,
    )


class IsolationError(RuntimeError):
    """A namespace could not be created or verified safely."""

    def __init__(self, code: str):
        super().__init__(code)
        self.code = code


def topology_token(run_id: str, network_id: str = "synthetic-private-036") -> str:
    """Return a short collision-resistant token suitable for interface names."""

    if not run_id or len(run_id) > 128:
        raise IsolationError("invalid-topology-run-id")
    return hashlib.sha256(f"{network_id}\0{run_id}".encode()).hexdigest()[:8]


@dataclass(frozen=True)
class EndpointDescription:
    local_address: str
    peer_address: str
    local_port: int
    peer_port: int
    address_family: str
    namespace: str
    network_id: str


@dataclass(frozen=True)
class TopologyDescription:
    run_id: str
    network_id: str
    token: str
    i2pr_namespace: str
    reference_namespace: str
    i2pr_ipv4: str
    reference_ipv4: str
    i2pr_ipv6: str | None
    reference_ipv6: str | None
    i2pr_port: int
    reference_port: int
    ipv6: bool
    policy_digest: str

    def endpoint_for_reference(self) -> EndpointDescription:
        return EndpointDescription(
            local_address=self.reference_ipv6 if self.ipv6 else self.reference_ipv4,
            peer_address=self.i2pr_ipv6 if self.ipv6 else self.i2pr_ipv4,
            local_port=self.reference_port,
            peer_port=self.i2pr_port,
            address_family="ipv6" if self.ipv6 else "ipv4",
            namespace=self.reference_namespace,
            network_id=self.network_id,
        )

    def digest(self) -> str:
        value = {
            "network_id": self.network_id,
            "token": self.token,
            "i2pr_namespace": self.i2pr_namespace,
            "reference_namespace": self.reference_namespace,
            "i2pr_ipv4": self.i2pr_ipv4,
            "reference_ipv4": self.reference_ipv4,
            "i2pr_ipv6": self.i2pr_ipv6,
            "reference_ipv6": self.reference_ipv6,
            "i2pr_port": self.i2pr_port,
            "reference_port": self.reference_port,
            "ipv6": self.ipv6,
            "policy_digest": self.policy_digest,
        }
        return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


class PrivilegedDualNamespaceTopology:
    """Create exactly two namespaces joined by one private veth pair.

    This is the Plan 038/040/044/045 privileged topology. It is renamed
    ``privileged-dual-netns-veth`` in Plan 046 and is no longer the default
    evidence lane. New code paths should select ``rootless-sealed-single-netns``
    through ``select_topology`` instead. ``NamespaceTopology`` is preserved as
    an alias for backwards compatibility with the existing test suite.
    """

    topology_kind = PRIVILEGED_TOPOLOGY_KIND
    privilege_model = PRIVILEGED_PRIVILEGE_MODEL

    def __init__(
        self,
        repo_root: Path,
        run_id: str,
        ipv6: bool,
        *,
        reference_port: int = 45678,
        i2pr_port: int = 45680,
        **_unused: Any,
    ):
        if not run_id or not all(character.isalnum() or character == "-" for character in run_id):
            raise IsolationError("invalid-namespace-run-id")
        if len(run_id) > 80:
            raise IsolationError("namespace-run-id-too-long")
        self.repo_root = repo_root
        self.run_id = run_id
        self.ipv6 = ipv6
        self.network_id = "synthetic-private-036"
        self.i2pr_namespace = f"i2pr-{run_id}"
        self.reference_namespace = f"ref-{run_id}"
        self.token = topology_token(run_id, self.network_id)
        self.i2pr_if = f"i2pr-v{self.token}a"
        self.reference_if = f"ref-v{self.token}b"
        if len(self.i2pr_if.encode()) > 15 or len(self.reference_if.encode()) > 15:
            raise IsolationError("topology-interface-name-too-long")
        self.reference_port = reference_port
        self.i2pr_port = i2pr_port
        self.created_namespaces: list[str] = []
        self.host_interfaces: list[str] = [self.i2pr_if, self.reference_if]
        self.created = False
        self._i2pr_rules = canonical_firewall_rules(
            local_ipv4="192.0.2.1", peer_ipv4="192.0.2.2", local_port=i2pr_port, peer_port=reference_port,
            local_ipv6="2001:db8:36::1" if ipv6 else None, peer_ipv6="2001:db8:36::2" if ipv6 else None,
        )
        self._reference_rules = canonical_firewall_rules(
            local_ipv4="192.0.2.2", peer_ipv4="192.0.2.1", local_port=reference_port, peer_port=i2pr_port,
            local_ipv6="2001:db8:36::2" if ipv6 else None, peer_ipv6="2001:db8:36::1" if ipv6 else None,
        )
        self._description = TopologyDescription(
            run_id=run_id, network_id=self.network_id, token=self.token,
            i2pr_namespace=self.i2pr_namespace, reference_namespace=self.reference_namespace,
            i2pr_ipv4="192.0.2.1", reference_ipv4="192.0.2.2",
            i2pr_ipv6="2001:db8:36::1" if ipv6 else None,
            reference_ipv6="2001:db8:36::2" if ipv6 else None,
            i2pr_port=i2pr_port, reference_port=reference_port, ipv6=ipv6,
            policy_digest=policy_digest(self._i2pr_rules + self._reference_rules),
        )

    @property
    def description(self) -> TopologyDescription:
        return self._description

    @property
    def _prefix(self) -> list[str]:
        return [] if os.geteuid() == 0 else ["sudo", "-n"]

    def placement(self, actor: str) -> ProcessPlacement:
        """Return the ``ip netns exec <ns>`` placement for this backend."""

        if actor == "i2pr":
            namespace = self.i2pr_namespace
        elif actor == "reference":
            namespace = self.reference_namespace
        else:
            raise IsolationError("unknown-actor")
        return ProcessPlacement(
            topology_kind=self.topology_kind,
            actor=actor,
            command_prefix=tuple(self._prefix + ["ip", "netns", "exec", namespace]),
        )

    def _run(self, args: list[str], *, input_text: str | None = None, capture: bool = False) -> str:
        result = subprocess.run(
            self._prefix + args, input=input_text, text=True,
            stdout=subprocess.PIPE if capture else subprocess.DEVNULL,
            stderr=subprocess.DEVNULL, check=False,
        )
        if result.returncode != 0:
            raise IsolationError("namespace-command-failed")
        return result.stdout if capture else ""

    def _firewall(self, namespace: str, rules: str) -> None:
        self._run(["ip", "netns", "exec", namespace, "nft", "-f", "-"], input_text=rules)

    def create(self) -> None:
        if self.created:
            raise IsolationError("namespace-already-created")
        try:
            self._run(["ip", "netns", "add", self.i2pr_namespace])
            self.created_namespaces.append(self.i2pr_namespace)
            self._run(["ip", "netns", "add", self.reference_namespace])
            self.created_namespaces.append(self.reference_namespace)
            self._run(["ip", "link", "add", self.i2pr_if, "type", "veth", "peer", "name", self.reference_if])
            self._run(["ip", "link", "set", self.i2pr_if, "netns", self.i2pr_namespace])
            self._run(["ip", "link", "set", self.reference_if, "netns", self.reference_namespace])
            for namespace in (self.i2pr_namespace, self.reference_namespace):
                self._run(["ip", "-n", namespace, "link", "set", "lo", "up"])
            self._run(["ip", "-n", self.i2pr_namespace, "link", "set", self.i2pr_if, "name", "peer0"])
            self._run(["ip", "-n", self.reference_namespace, "link", "set", self.reference_if, "name", "peer0"])
            for namespace in (self.i2pr_namespace, self.reference_namespace):
                self._run(["ip", "-n", namespace, "link", "set", "peer0", "up"])
                if self.ipv6:
                    self._run(["ip", "-n", namespace, "link", "set", "dev", "peer0", "addrgenmode", "none"])
            self._run(["ip", "-n", self.i2pr_namespace, "addr", "add", "192.0.2.1/30", "dev", "peer0"])
            self._run(["ip", "-n", self.reference_namespace, "addr", "add", "192.0.2.2/30", "dev", "peer0"])
            if self.ipv6:
                self._run(["ip", "-n", self.i2pr_namespace, "-6", "addr", "add", "2001:db8:36::1/64", "dev", "peer0"])
                self._run(["ip", "-n", self.reference_namespace, "-6", "addr", "add", "2001:db8:36::2/64", "dev", "peer0"])
            self._firewall(self.i2pr_namespace, self._i2pr_rules)
            self._firewall(self.reference_namespace, self._reference_rules)
            self.created = True
            self.verify()
            self.firewall_self_test()
        except Exception as exc:
            self.destroy()
            if isinstance(exc, IsolationError):
                raise
            raise IsolationError("namespace-create-failed") from exc

    def _ruleset_digest(self, namespace: str) -> str:
        rules = self._run(["ip", "netns", "exec", namespace, "nft", "list", "ruleset"], capture=True)
        return hashlib.sha256(normalize_ruleset(rules).encode()).hexdigest()

    def verify(self) -> None:
        script = self.repo_root / "scripts/interop/verify-isolation.sh"
        for namespace, local, peer, local_port, peer_port in (
            (self.i2pr_namespace, "192.0.2.1", "192.0.2.2", self.i2pr_port, self.reference_port),
            (self.reference_namespace, "192.0.2.2", "192.0.2.1", self.reference_port, self.i2pr_port),
        ):
            args = [str(script), "--namespace", namespace, "--local-ipv4", local, "--peer-ipv4", peer,
                    "--local-port", str(local_port), "--peer-port", str(peer_port),
                    "--ruleset-digest", self._ruleset_digest(namespace)]
            if self.ipv6:
                local6 = "2001:db8:36::1" if local.endswith(".1") else "2001:db8:36::2"
                peer6 = "2001:db8:36::2" if local.endswith(".1") else "2001:db8:36::1"
                args.extend(["--ipv6", "--local-ipv6", local6, "--peer-ipv6", peer6])
            result = subprocess.run(self._prefix + ["bash", *args], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            if result.returncode != 0:
                raise IsolationError("isolation-preflight-failed")

    def firewall_self_test(self) -> None:
        """Exercise only the synthetic peer path before any router starts."""

        code = ("import socket; s=socket.socket(); s.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1); "
                f"s.bind(('192.0.2.2',{self.reference_port})); s.listen(1); print('ready',flush=True); "
                "s.settimeout(3); c,_=s.accept(); c.close(); s.close()")
        process = subprocess.Popen(self._prefix + ["ip", "netns", "exec", self.reference_namespace, "python3", "-c", code],
                                   stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        try:
            deadline = time.monotonic() + 2
            ready = False
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise IsolationError("firewall-canary-exited")
                if process.stdout is not None and process.stdout.readline().strip() == "ready":
                    ready = True
                    break
                time.sleep(0.01)
            if not ready:
                raise IsolationError("firewall-canary-timeout")
            for address, port, error in (("192.0.2.2", self.reference_port, "firewall-peer-port-blocked"),
                                         ("192.0.2.2", self.reference_port + 1, "firewall-non-peer-port-allowed"),
                                         ("203.0.113.1", self.reference_port, "firewall-public-route-allowed")):
                connect = "import socket; s=socket.create_connection((%r,%d),0.5); s.close()" % (address, port)
                result = subprocess.run(self._prefix + ["ip", "netns", "exec", self.i2pr_namespace, "python3", "-c", connect], check=False)
                if (port == self.reference_port and address == "192.0.2.2" and result.returncode != 0) or (port != self.reference_port or address != "192.0.2.2") and result.returncode == 0:
                    raise IsolationError(error)
        finally:
            if process.poll() is None:
                process.terminate()
            try:
                process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=2)

    def destroy(self) -> str:
        failures = 0
        for namespace in reversed(self.created_namespaces):
            result = subprocess.run(self._prefix + ["ip", "netns", "del", namespace], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
            if result.returncode not in (0, 1):
                failures += 1
        self.created_namespaces.clear()
        for interface in self.host_interfaces:
            result = subprocess.run(self._prefix + ["ip", "link", "del", interface], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
            if result.returncode not in (0, 1):
                failures += 1
        self.created = False
        return "failed" if failures else "clean"


# Backwards-compatible alias for the Plan 038/040/044/045 callers that still
# import ``NamespaceTopology``. New code should select the topology through
# ``select_topology`` instead.
NamespaceTopology = PrivilegedDualNamespaceTopology


def _build_privileged(
    *,
    repo_root: Path,
    run_id: str,
    ipv6: bool,
    reference_port: int,
    i2pr_port: int,
    **_unused: Any,
) -> "PrivilegedDualNamespaceTopology":
    return PrivilegedDualNamespaceTopology(
        repo_root=repo_root,
        run_id=run_id,
        ipv6=ipv6,
        reference_port=reference_port,
        i2pr_port=i2pr_port,
    )


register_topology(PRIVILEGED_TOPOLOGY_KIND, _build_privileged)


def _digest(self: Any) -> str:
    return self.description.digest()


PrivilegedDualNamespaceTopology.digest = _digest  # type: ignore[attr-defined]


def _verify_before_start(self: Any) -> dict[str, Any]:
    self.verify()
    self.firewall_self_test()
    return {"status": "verified", "topology_kind": self.topology_kind}


def _verify_during_run(self: Any) -> dict[str, Any]:
    return {"status": "verified-during-run", "topology_kind": self.topology_kind}


PrivilegedDualNamespaceTopology.verify_before_start = _verify_before_start  # type: ignore[attr-defined]
PrivilegedDualNamespaceTopology.verify_during_run = _verify_during_run  # type: ignore[attr-defined]
