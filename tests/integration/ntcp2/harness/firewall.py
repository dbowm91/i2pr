"""Pure firewall policy generation for the disposable two-namespace link."""

from __future__ import annotations

import hashlib


def canonical_firewall_rules(
    *,
    local_ipv4: str,
    peer_ipv4: str,
    local_port: int,
    peer_port: int,
    local_ipv6: str | None = None,
    peer_ipv6: str | None = None,
) -> str:
    ipv6 = ""
    if local_ipv6 is not None and peer_ipv6 is not None:
        ipv6 = (
            f"    ip6 daddr {peer_ipv6} tcp dport {peer_port} accept\n"
            f"    ip6 saddr {peer_ipv6} tcp dport {local_port} accept\n"
        )
    return f"""flush ruleset
table inet i2pr_interop {{
  chain output {{
    type filter hook output priority 0; policy drop;
    oifname \"lo\" accept
    ct state established,related accept
    ip daddr {peer_ipv4} tcp dport {peer_port} accept
{ipv6}  }}
  chain input {{
    type filter hook input priority 0; policy drop;
    iifname \"lo\" accept
    ct state established,related accept
    ip saddr {peer_ipv4} tcp dport {local_port} accept
  }}
}}
"""


def policy_digest(rules: str) -> str:
    return hashlib.sha256(rules.encode("utf-8")).hexdigest()


def normalize_ruleset(rules: str) -> str:
    return "\n".join(" ".join(line.split()) for line in rules.splitlines() if line.strip()) + "\n"
