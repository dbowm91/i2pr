"""Strict scenario model for the Plan 041 reference-only crosscheck."""

from __future__ import annotations

import ipaddress
import re
import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Any


JAVA_REVISION = "2800040deee9bb376567b671ef2e9c34cf3e30b6"
I2PD_REVISION = "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e"
PRIVATE_NETWORK_ID = 99
PRIVATE_NETWORK_POLICY = "explicit-non-public"

_SCENARIO_KEYS = frozenset(
    {
        "schema_version",
        "scenario_id",
        "java_reference",
        "java_revision",
        "i2pd_reference",
        "i2pd_revision",
        "private_network_id",
        "private_network_policy",
        "address_family",
        "java_address",
        "java_port",
        "i2pd_address",
        "i2pd_port",
        "startup_order",
        "router_info_exchange_order",
        "dial_initiator",
        "handshake_deadline_seconds",
        "observation_method",
        "expected_authenticated_link_count",
        "cleanup_policy",
    }
)
_SCENARIO_ID = re.compile(r"^reference-(?:java-i2pd|i2pd-java)-ipv4$")
_REVISION = re.compile(r"^[0-9a-f]{40}$")
_ORDER = ("java_i2p", "i2pd")


class ReferenceScenarioError(ValueError):
    """A reference-pair scenario is malformed or outside the contract."""


@dataclass(frozen=True)
class ReferenceEndpoint:
    """One literal endpoint in the synthetic reference-pair subnet."""

    reference: str
    address: str
    port: int


@dataclass(frozen=True)
class ReferencePairScenario:
    """Validated, non-secret instructions for one reference-only run."""

    schema_version: int
    scenario_id: str
    java_reference: str
    java_revision: str
    i2pd_reference: str
    i2pd_revision: str
    private_network_id: int
    private_network_policy: str
    address_family: str
    java: ReferenceEndpoint
    i2pd: ReferenceEndpoint
    startup_order: tuple[str, str]
    router_info_exchange_order: tuple[str, str]
    dial_initiator: str
    handshake_deadline_seconds: int
    observation_method: str
    expected_authenticated_link_count: int
    cleanup_policy: str

    @property
    def reference_revisions(self) -> dict[str, str]:
        return {"java_i2p": self.java_revision, "i2pd": self.i2pd_revision}

    def endpoint(self, reference: str) -> ReferenceEndpoint:
        if reference == "java_i2p":
            return self.java
        if reference == "i2pd":
            return self.i2pd
        raise ReferenceScenarioError("unknown reference identifier")


def _require_exact_keys(value: dict[str, Any]) -> None:
    if frozenset(value) != _SCENARIO_KEYS:
        unknown = sorted(frozenset(value) - _SCENARIO_KEYS)
        missing = sorted(_SCENARIO_KEYS - frozenset(value))
        raise ReferenceScenarioError(f"scenario shape drift: unknown={unknown}, missing={missing}")


def _endpoint(reference: str, address: Any, port: Any) -> ReferenceEndpoint:
    if not isinstance(address, str) or not isinstance(port, int) or isinstance(port, bool):
        raise ReferenceScenarioError("endpoint has an invalid address or port")
    try:
        parsed = ipaddress.ip_address(address)
    except ValueError as exc:
        raise ReferenceScenarioError("endpoint address is not a literal IP") from exc
    if parsed.version != 4 or not parsed.is_private:
        raise ReferenceScenarioError("reference endpoint is outside the synthetic IPv4 policy")
    if not 1 <= port <= 65535:
        raise ReferenceScenarioError("reference endpoint port is outside 1..=65535")
    return ReferenceEndpoint(reference, address, port)


def load_reference_scenario(path: Path) -> ReferencePairScenario:
    """Load and validate one dedicated Plan 041 scenario file."""

    try:
        document = tomllib.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, tomllib.TOMLDecodeError) as exc:
        raise ReferenceScenarioError("reference scenario is not valid TOML") from exc
    if set(document) != {"scenario"} or not isinstance(document["scenario"], dict):
        raise ReferenceScenarioError("reference scenario must contain one [scenario] table")
    value = document["scenario"]
    _require_exact_keys(value)
    if value["schema_version"] != 1:
        raise ReferenceScenarioError("unsupported reference scenario schema")
    if not isinstance(value["scenario_id"], str) or not _SCENARIO_ID.fullmatch(value["scenario_id"]):
        raise ReferenceScenarioError("invalid reference scenario ID")
    if value["java_reference"] != "java_i2p" or value["i2pd_reference"] != "i2pd":
        raise ReferenceScenarioError("non-canonical reference identifiers")
    if value["java_revision"] != JAVA_REVISION or value["i2pd_revision"] != I2PD_REVISION:
        raise ReferenceScenarioError("reference revision does not match the lock")
    if not _REVISION.fullmatch(value["java_revision"]) or not _REVISION.fullmatch(value["i2pd_revision"]):
        raise ReferenceScenarioError("reference revision is not a full object ID")
    if value["private_network_id"] != PRIVATE_NETWORK_ID:
        raise ReferenceScenarioError("reference network ID is not the reviewed private value")
    if value["private_network_policy"] != PRIVATE_NETWORK_POLICY:
        raise ReferenceScenarioError("reference network-ID policy is not explicit")
    if value["address_family"] != "ipv4":
        raise ReferenceScenarioError("only the bounded IPv4 crosscheck is supported")
    java = _endpoint("java_i2p", value["java_address"], value["java_port"])
    i2pd = _endpoint("i2pd", value["i2pd_address"], value["i2pd_port"])
    if java.address == i2pd.address or java.port == i2pd.port:
        raise ReferenceScenarioError("reference endpoints must use distinct addresses and ports")
    for key in ("startup_order", "router_info_exchange_order"):
        order = value[key]
        if not isinstance(order, list) or tuple(order) not in (_ORDER, _ORDER[::-1]):
            raise ReferenceScenarioError(f"{key} must contain each reference exactly once")
    if value["dial_initiator"] not in _ORDER:
        raise ReferenceScenarioError("dial initiator is not a canonical reference")
    if not isinstance(value["handshake_deadline_seconds"], int) or not 1 <= value["handshake_deadline_seconds"] <= 300:
        raise ReferenceScenarioError("handshake deadline is outside the bounded range")
    if value["observation_method"] != "dual-authoritative-authenticated-state":
        raise ReferenceScenarioError("reference scenario lacks dual authenticated observation")
    if value["expected_authenticated_link_count"] != 1:
        raise ReferenceScenarioError("reference scenario must expect exactly one link")
    if value["cleanup_policy"] != "delete-run-root-and-verify-zero-residual":
        raise ReferenceScenarioError("reference scenario lacks strict cleanup policy")
    return ReferencePairScenario(
        schema_version=value["schema_version"],
        scenario_id=value["scenario_id"],
        java_reference=value["java_reference"],
        java_revision=value["java_revision"],
        i2pd_reference=value["i2pd_reference"],
        i2pd_revision=value["i2pd_revision"],
        private_network_id=value["private_network_id"],
        private_network_policy=value["private_network_policy"],
        address_family=value["address_family"],
        java=java,
        i2pd=i2pd,
        startup_order=tuple(value["startup_order"]),
        router_info_exchange_order=tuple(value["router_info_exchange_order"]),
        dial_initiator=value["dial_initiator"],
        handshake_deadline_seconds=value["handshake_deadline_seconds"],
        observation_method=value["observation_method"],
        expected_authenticated_link_count=value["expected_authenticated_link_count"],
        cleanup_policy=value["cleanup_policy"],
    )
