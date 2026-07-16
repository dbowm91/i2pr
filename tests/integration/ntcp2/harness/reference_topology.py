"""Privileged reference-pair topology owner for Plan 041.

The Plan 041 reference-pair topology is the privileged dual-namespace/veth
backend (``privileged-dual-netns-veth``) restricted to reference-only control
runs. Plan 046 keeps this backend for explicit qualification work but it is
never the default evidence lane for the i2pr mixed-router evidence path.
"""

from __future__ import annotations

import hashlib
import ipaddress
import json
import os
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Any

try:
    from .interop_topology import (
        PRIVILEGED_PRIVILEGE_MODEL,
        PRIVILEGED_TOPOLOGY_KIND,
        ProcessPlacement,
    )
    from .reference_scenario import ReferencePairScenario
except ImportError:  # unittest discovery loads this directory as a flat path.
    from interop_topology import (  # type: ignore
        PRIVILEGED_PRIVILEGE_MODEL,
        PRIVILEGED_TOPOLOGY_KIND,
        ProcessPlacement,
    )
    from reference_scenario import ReferencePairScenario  # type: ignore


class ReferenceTopologyError(RuntimeError):
    """The reference-pair topology could not be created or verified safely."""

    def __init__(self, code: str):
        super().__init__(code)
        self.code = code


def canonical_reference_firewall_rules(
    *, local_address: str, peer_address: str, local_port: int, peer_port: int, can_initiate: bool
) -> str:
    """Return the exact namespace policy for one directional crosscheck."""

    if ipaddress.ip_address(local_address).version != 4 or ipaddress.ip_address(peer_address).version != 4:
        raise ReferenceTopologyError("reference-topology-only-supports-ipv4")
    output = "    ip daddr %s tcp dport %d ct state new accept\n" % (peer_address, peer_port) if can_initiate else ""
    input_rule = "    ip saddr %s tcp dport %d ct state new accept\n" % (peer_address, local_port) if not can_initiate else ""
    return f"""flush ruleset
table inet i2pr_reference_pair {{
  chain output {{
    type filter hook output priority 0; policy drop;
    oifname \"lo\" accept
    ct state established,related accept
{output}  }}
  chain input {{
    type filter hook input priority 0; policy drop;
    iifname \"lo\" accept
    ct state established,related accept
{input_rule}  }}
}}
"""


@dataclass(frozen=True)
class ReferenceTopologyDescription:
    run_id: str
    short_run_id: str
    java_namespace: str
    i2pd_namespace: str
    java_address: str
    i2pd_address: str
    java_port: int
    i2pd_port: int
    dial_initiator: str
    java_rules_sha256: str
    i2pd_rules_sha256: str

    def digest(self) -> str:
        value = {
            "run_id_class": "disposable-reference-pair",
            "short_run_id": self.short_run_id,
            "java_namespace": self.java_namespace,
            "i2pd_namespace": self.i2pd_namespace,
            "java_address": self.java_address,
            "i2pd_address": self.i2pd_address,
            "java_port": self.java_port,
            "i2pd_port": self.i2pd_port,
            "dial_initiator": self.dial_initiator,
            "java_rules_sha256": self.java_rules_sha256,
            "i2pd_rules_sha256": self.i2pd_rules_sha256,
        }
        return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


class ReferencePairTopology:
    """Create exactly Java and i2pd namespaces joined by one synthetic veth."""

    topology_kind = PRIVILEGED_TOPOLOGY_KIND
    privilege_model = PRIVILEGED_PRIVILEGE_MODEL

    def __init__(self, scenario: ReferencePairScenario, run_id: str):
        if not run_id or len(run_id) > 80 or not all(c.isalnum() or c == "-" for c in run_id):
            raise ReferenceTopologyError("invalid-reference-run-id")
        self.scenario = scenario
        self.run_id = run_id
        self.short_run_id = hashlib.sha256(run_id.encode()).hexdigest()[:8]
        self.java_namespace = f"java-{self.short_run_id}"
        self.i2pd_namespace = f"i2pd-{self.short_run_id}"
        token = hashlib.sha256(f"reference-private-041\0{run_id}".encode()).hexdigest()[:8]
        self.java_if = f"jv{token}a"
        self.i2pd_if = f"iv{token}b"
        self.created_namespaces: list[str] = []
        self.java_rules = canonical_reference_firewall_rules(
            local_address=scenario.java.address,
            peer_address=scenario.i2pd.address,
            local_port=scenario.java.port,
            peer_port=scenario.i2pd.port,
            can_initiate=scenario.dial_initiator == "java_i2p",
        )
        self.i2pd_rules = canonical_reference_firewall_rules(
            local_address=scenario.i2pd.address,
            peer_address=scenario.java.address,
            local_port=scenario.i2pd.port,
            peer_port=scenario.java.port,
            can_initiate=scenario.dial_initiator == "i2pd",
        )
        self._description = ReferenceTopologyDescription(
            run_id=run_id,
            short_run_id=self.short_run_id,
            java_namespace=self.java_namespace,
            i2pd_namespace=self.i2pd_namespace,
            java_address=scenario.java.address,
            i2pd_address=scenario.i2pd.address,
            java_port=scenario.java.port,
            i2pd_port=scenario.i2pd.port,
            dial_initiator=scenario.dial_initiator,
            java_rules_sha256=self._digest(self._canonical_rules(self.java_rules)),
            i2pd_rules_sha256=self._digest(self._canonical_rules(self.i2pd_rules)),
        )

    @staticmethod
    def _digest(value: str) -> str:
        return hashlib.sha256(value.encode()).hexdigest()

    @staticmethod
    def _canonical_rules(rules: str) -> str:
        return "\n".join(" ".join(line.split()) for line in rules.splitlines() if line.strip()) + "\n"

    @property
    def description(self) -> ReferenceTopologyDescription:
        return self._description

    @property
    def _prefix(self) -> list[str]:
        return [] if os.geteuid() == 0 else ["sudo", "-n"]

    def placement(self, actor: str) -> ProcessPlacement:
        """Return the ``ip netns exec <ns>`` placement for this backend."""

        if actor == "java_i2p":
            namespace = self.java_namespace
        elif actor == "i2pd":
            namespace = self.i2pd_namespace
        else:
            raise ReferenceTopologyError("unknown-actor")
        return ProcessPlacement(
            topology_kind=self.topology_kind,
            actor=actor,
            command_prefix=tuple(self._prefix + ["ip", "netns", "exec", namespace]),
        )

    def _run(self, args: list[str], *, input_text: str | None = None, capture: bool = False) -> str:
        completed = subprocess.run(
            self._prefix + args,
            input=input_text,
            text=True,
            stdout=subprocess.PIPE if capture else subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
        if completed.returncode != 0:
            raise ReferenceTopologyError("reference-topology-command-failed")
        return completed.stdout if capture else ""

    def _install_firewall(self, namespace: str, rules: str) -> None:
        self._run(["ip", "netns", "exec", namespace, "nft", "-f", "-"], input_text=rules)

    def create(self) -> None:
        if self.created_namespaces:
            raise ReferenceTopologyError("reference-topology-already-created")
        try:
            self._run(["ip", "netns", "add", self.java_namespace])
            self.created_namespaces.append(self.java_namespace)
            self._run(["ip", "netns", "add", self.i2pd_namespace])
            self.created_namespaces.append(self.i2pd_namespace)
            self._run(["ip", "link", "add", self.java_if, "type", "veth", "peer", "name", self.i2pd_if])
            self._run(["ip", "link", "set", self.java_if, "netns", self.java_namespace])
            self._run(["ip", "link", "set", self.i2pd_if, "netns", self.i2pd_namespace])
            for namespace, interface, address in (
                (self.java_namespace, self.java_if, "192.0.2.1/30"),
                (self.i2pd_namespace, self.i2pd_if, "192.0.2.2/30"),
            ):
                self._run(["ip", "-n", namespace, "link", "set", "lo", "up"])
                self._run(["ip", "-n", namespace, "link", "set", interface, "name", "peer0"])
                self._run(["ip", "-n", namespace, "link", "set", "peer0", "up"])
                self._run(["ip", "-n", namespace, "addr", "add", address, "dev", "peer0"])
            self._install_firewall(self.java_namespace, self.java_rules)
            self._install_firewall(self.i2pd_namespace, self.i2pd_rules)
            self.verify()
        except Exception as exc:
            self.destroy()
            if isinstance(exc, ReferenceTopologyError):
                raise
            raise ReferenceTopologyError("reference-topology-create-failed") from exc

    def _ruleset_digest(self, namespace: str) -> str:
        rules = self._run(["ip", "netns", "exec", namespace, "nft", "list", "ruleset"], capture=True)
        return self._digest(self._canonical_rules(rules))

    def verify(self) -> None:
        host_links = self._run(["ip", "-o", "link", "show"], capture=True)
        if self.java_if in host_links or self.i2pd_if in host_links:
            raise ReferenceTopologyError("reference-topology-host-veth-present")
        for namespace, local, peer, rules, expected_digest in (
            (self.java_namespace, "192.0.2.1", "192.0.2.2", self.java_rules, self.description.java_rules_sha256),
            (self.i2pd_namespace, "192.0.2.2", "192.0.2.1", self.i2pd_rules, self.description.i2pd_rules_sha256),
        ):
            interfaces = self._run(["ip", "-n", namespace, "-o", "link", "show"], capture=True)
            names = sorted(line.split(": ", 1)[1].split("@", 1)[0] for line in interfaces.splitlines() if ": " in line)
            if names != ["lo", "peer0"]:
                raise ReferenceTopologyError("reference-topology-interface-drift")
            addresses = self._run(["ip", "-n", namespace, "-4", "-o", "addr", "show", "dev", "peer0"], capture=True)
            if f" {local}/30 " not in f" {addresses} ":
                raise ReferenceTopologyError("reference-topology-address-drift")
            routes = self._run(["ip", "-n", namespace, "route", "show"], capture=True)
            if routes.count("192.0.2.0/30 dev peer0") != 1 or "default" in routes:
                raise ReferenceTopologyError("reference-topology-route-drift")
            public_probe = subprocess.run(
                self._prefix + ["ip", "netns", "exec", namespace, "ip", "route", "get", "1.1.1.1"],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
            if public_probe.returncode == 0:
                raise ReferenceTopologyError("reference-topology-public-route-present")
            forwarding = self._run(["ip", "netns", "exec", namespace, "sysctl", "-n", "net.ipv4.ip_forward"], capture=True)
            if forwarding.strip() != "0":
                raise ReferenceTopologyError("reference-topology-forwarding-enabled")
            if self._ruleset_digest(namespace) != expected_digest:
                raise ReferenceTopologyError("reference-topology-firewall-drift")
            if self._digest(rules) == "0" * 64:
                raise ReferenceTopologyError("reference-topology-empty-firewall")
            if peer not in rules:
                raise ReferenceTopologyError("reference-topology-peer-policy-missing")

    def destroy(self) -> str:
        failures = 0
        for namespace in reversed(self.created_namespaces):
            result = subprocess.run(self._prefix + ["ip", "netns", "del", namespace], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
            if result.returncode not in (0, 1):
                failures += 1
        self.created_namespaces.clear()
        for interface in (self.java_if, self.i2pd_if):
            result = subprocess.run(self._prefix + ["ip", "link", "del", interface], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
            if result.returncode not in (0, 1):
                failures += 1
        return "failed" if failures else "clean"

    def residual_state(self) -> bool:
        """Return whether any owned namespace or host veth survived teardown."""

        namespaces = subprocess.run(self._prefix + ["ip", "netns", "list"], capture_output=True, text=True, check=False)
        if namespaces.returncode != 0:
            return True
        if any(name in namespaces.stdout.split() for name in (self.java_namespace, self.i2pd_namespace)):
            return True
        links = subprocess.run(self._prefix + ["ip", "-o", "link", "show"], capture_output=True, text=True, check=False)
        return links.returncode != 0 or any(interface in links.stdout for interface in (self.java_if, self.i2pd_if))
