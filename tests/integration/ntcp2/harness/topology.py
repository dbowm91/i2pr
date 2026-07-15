"""Fail-closed Linux network namespace topology owner."""

from __future__ import annotations

import os
import subprocess
from pathlib import Path


class IsolationError(RuntimeError):
    """A namespace could not be created or verified safely."""

    def __init__(self, code: str):
        super().__init__(code)
        self.code = code


class NamespaceTopology:
    """Create exactly two namespaces joined by one private veth pair."""

    def __init__(self, repo_root: Path, run_id: str, ipv6: bool):
        self.repo_root = repo_root
        self.run_id = run_id
        self.ipv6 = ipv6
        self.i2pr_namespace = f"i2pr-{run_id}"
        self.reference_namespace = f"ref-{run_id}"
        self.created = False

    @property
    def _prefix(self) -> list[str]:
        return [] if os.geteuid() == 0 else ["sudo", "-n"]

    def _run(self, args: list[str], *, input_text: str | None = None) -> None:
        result = subprocess.run(
            self._prefix + args,
            input=input_text,
            text=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
        if result.returncode != 0:
            raise IsolationError("namespace-command-failed")

    def _firewall(self, namespace: str, peer: str) -> None:
        ipv6_rule = "    ip6 daddr 2001:db8:36::/64 accept\n" if self.ipv6 else ""
        rules = f"""flush ruleset
table inet i2pr_interop {{
  chain output {{
    type filter hook output priority 0; policy drop;
    oifname "lo" accept
    ct state established,related accept
    ip daddr {peer} tcp dport {{ 45678, 45679 }} accept
    ip daddr {peer} icmp type echo-request accept
{ipv6_rule}  }}
  chain input {{
    type filter hook input priority 0; policy drop;
    iifname "lo" accept
    ct state established,related accept
    ip saddr {peer} tcp sport {{ 45678, 45679 }} accept
{ipv6_rule.replace('daddr', 'saddr')}  }}
}}
"""
        self._run(["ip", "netns", "exec", namespace, "nft", "-f", "-"], input_text=rules)

    def create(self) -> None:
        if self.created:
            raise IsolationError("namespace-already-created")
        try:
            self._run(["ip", "netns", "add", self.i2pr_namespace])
            self._run(["ip", "netns", "add", self.reference_namespace])
            self._run(["ip", "link", "add", f"veth-{self.run_id}-a", "type", "veth", "peer", "name", f"veth-{self.run_id}-b"])
            self._run(["ip", "link", "set", f"veth-{self.run_id}-a", "netns", self.i2pr_namespace])
            self._run(["ip", "link", "set", f"veth-{self.run_id}-b", "netns", self.reference_namespace])
            for namespace in (self.i2pr_namespace, self.reference_namespace):
                self._run(["ip", "-n", namespace, "link", "set", "lo", "up"])
            self._run(["ip", "-n", self.i2pr_namespace, "link", "set", f"veth-{self.run_id}-a", "name", "peer0"])
            self._run(["ip", "-n", self.reference_namespace, "link", "set", f"veth-{self.run_id}-b", "name", "peer0"])
            for namespace in (self.i2pr_namespace, self.reference_namespace):
                self._run(["ip", "-n", namespace, "link", "set", "peer0", "up"])
            self._run(["ip", "-n", self.i2pr_namespace, "addr", "add", "192.0.2.1/30", "dev", "peer0"])
            self._run(["ip", "-n", self.reference_namespace, "addr", "add", "192.0.2.2/30", "dev", "peer0"])
            if self.ipv6:
                self._run(["ip", "-n", self.i2pr_namespace, "-6", "addr", "add", "2001:db8:36::1/64", "dev", "peer0"])
                self._run(["ip", "-n", self.reference_namespace, "-6", "addr", "add", "2001:db8:36::2/64", "dev", "peer0"])
            self._firewall(self.i2pr_namespace, "192.0.2.2")
            self._firewall(self.reference_namespace, "192.0.2.1")
            self.created = True
            self.verify()
        except Exception as exc:
            self.destroy()
            if isinstance(exc, IsolationError):
                raise
            raise IsolationError("namespace-create-failed")

    def verify(self) -> None:
        script = self.repo_root / "scripts/interop/verify-isolation.sh"
        for namespace in (self.i2pr_namespace, self.reference_namespace):
            args = [str(script), "--namespace", namespace]
            if self.ipv6:
                args.append("--ipv6")
            result = subprocess.run(self._prefix + ["bash", *args], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            if result.returncode != 0:
                raise IsolationError("isolation-preflight-failed")

    def destroy(self) -> str:
        forced = False
        for namespace in (self.i2pr_namespace, self.reference_namespace):
            result = subprocess.run(
                self._prefix + ["ip", "netns", "del", namespace],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
            forced = forced or result.returncode not in (0, 1)
        self.created = False
        return "forced" if forced else "clean"
