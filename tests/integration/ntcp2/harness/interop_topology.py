"""Topology backend contract and process-placement abstraction for Plan 046.

This module is the only path that owns topology identifiers
(``rootless-sealed-single-netns`` and ``privileged-dual-netns-veth``) and
process placement (``ProcessPlacement``). Adapters and runners must consume
the contract through ``select_topology`` and ``placement_for`` rather than
constructing ``ip netns`` / ``sudo`` prefixes themselves.

The privileged dual-namespace/veth topology (Plan 038/040) is preserved for
explicit later qualification work but is never the default evidence lane. The
rootless sealed single-network-namespace topology is the primary evidence
lane for Plan 045/046 and must remain free of ``sudo``, host capability,
host-visible named namespaces, host veth creation, and host
firewall mutation.
"""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Callable, Protocol, Sequence

ROOTLESS_TOPOLOGY_KIND = "rootless-sealed-single-netns"
PRIVILEGED_TOPOLOGY_KIND = "privileged-dual-netns-veth"
ROOTLESS_PRIVILEGE_MODEL = "unprivileged-userns"
PRIVILEGED_PRIVILEGE_MODEL = "host-capabilities"

ALLOWED_TOPOLOGY_KINDS = frozenset(
    {ROOTLESS_TOPOLOGY_KIND, PRIVILEGED_TOPOLOGY_KIND}
)

ALLOWED_ACTORS = frozenset({"i2pr", "reference", "control"})


class TopologyContractError(ValueError):
    """A topology backend contract was violated by a caller or backend."""

    def __init__(self, code: str):
        super().__init__(code)
        self.code = code


@dataclass(frozen=True)
class ProcessPlacement:
    """Where a child process must execute.

    ``topology_kind`` selects the backend; ``actor`` selects the role
    within the topology; ``command_prefix`` is the fixed, backend-supplied
    prefix required for the child to enter the correct execution
    context. Adapters must not construct any prefix themselves.
    """

    topology_kind: str
    actor: str
    command_prefix: tuple[str, ...] = ()

    def __post_init__(self) -> None:
        if self.topology_kind not in ALLOWED_TOPOLOGY_KINDS:
            raise TopologyContractError("unknown-topology-kind")
        if self.actor not in ALLOWED_ACTORS:
            raise TopologyContractError("unknown-actor")

    def command(self, argv: Sequence[str]) -> list[str]:
        """Return the full command list for this placement."""

        return [*self.command_prefix, *argv]


class InteropTopology(Protocol):
    """Narrow backend contract every topology backend must satisfy."""

    topology_kind: str
    privilege_model: str

    def create(self) -> None: ...
    def placement(self, actor: str) -> ProcessPlacement: ...
    def description(self) -> dict[str, Any]: ...
    def verify_before_start(self) -> dict[str, Any]: ...
    def verify_during_run(self) -> dict[str, Any]: ...
    def destroy(self) -> str: ...
    def digest(self) -> str: ...


@dataclass
class _Registry:
    builders: dict[str, Callable[..., InteropTopology]] = field(default_factory=dict)


_REGISTRY = _Registry()


def register_topology(name: str, factory: Callable[..., InteropTopology]) -> None:
    """Register a topology backend by its canonical identifier."""

    if name not in ALLOWED_TOPOLOGY_KINDS:
        raise TopologyContractError("unknown-topology-kind")
    _REGISTRY.builders[name] = factory


def select_topology(
    topology_kind: str,
    *,
    repo_root: Path,
    run_id: str,
    ipv6: bool = False,
    reference_port: int = 45678,
    i2pr_port: int = 45680,
    scenario: Any = None,
    shared_data_dir: Path | None = None,
    shared_state_dir: Path | None = None,
    reference_kind: str | None = None,
) -> InteropTopology:
    """Construct the requested topology backend.

    The default topology is the rootless sealed single-network-namespace
    backend. The privileged dual-namespace/veth backend remains available
    for explicit qualification work but is never the default fallback.
    """

    if topology_kind not in ALLOWED_TOPOLOGY_KINDS:
        raise TopologyContractError("unknown-topology-kind")
    factory = _REGISTRY.builders.get(topology_kind)
    if factory is None:
        raise TopologyContractError("topology-backend-not-registered")
    return factory(
        repo_root=repo_root,
        run_id=run_id,
        ipv6=ipv6,
        reference_port=reference_port,
        i2pr_port=i2pr_port,
        scenario=scenario,
        shared_data_dir=shared_data_dir,
        shared_state_dir=shared_state_dir,
        reference_kind=reference_kind or "java_i2p",
    )


def _topology_digest(payload: dict[str, Any]) -> str:
    return hashlib.sha256(
        json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def normalize_description(topology_kind: str, payload: dict[str, Any]) -> dict[str, Any]:
    """Return a digest-stable description dict for any topology backend."""

    enriched = dict(payload)
    enriched["topology_kind"] = topology_kind
    return enriched


__all__ = [
    "ALLOWED_ACTORS",
    "ALLOWED_TOPOLOGY_KINDS",
    "InteropTopology",
    "PRIVILEGED_PRIVILEGE_MODEL",
    "PRIVILEGED_TOPOLOGY_KIND",
    "ProcessPlacement",
    "ROOTLESS_PRIVILEGE_MODEL",
    "ROOTLESS_TOPOLOGY_KIND",
    "TopologyContractError",
    "normalize_description",
    "register_topology",
    "select_topology",
]
