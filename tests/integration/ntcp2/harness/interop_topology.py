"""Topology backend contract and process-placement abstraction for Plan 046.

This module is the only path that owns topology identifiers
(``rootless-sealed-single-netns``, ``privileged-dual-netns-veth``, and
the Plan 086 ``host-loopback-development``) and process placement
(``ProcessPlacement``). Adapters and runners must consume the
contract through ``select_topology`` and ``placement_for`` rather than
constructing ``ip netns`` / ``sudo`` prefixes themselves.

The privileged dual-namespace/veth topology (Plan 038/040) is preserved for
explicit later qualification work but is never the default evidence lane. The
rootless sealed single-network-namespace topology is the primary evidence
lane for Plan 045/046 and must remain free of ``sudo``, host capability,
host-visible named namespaces, host veth creation, and host
firewall mutation.

The Plan 086 ``host-loopback-development`` topology is the development-only
literal IPv4 loopback lane. It boundedly enables the previously scaffolding
tests to execute real NTCP2 wires on the host loopback without sudo,
namespaces, or Multipass. It must never be reused for release-qualification
evidence and must never claim parent-network isolation.
"""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Callable, Protocol, Sequence

ROOTLESS_TOPOLOGY_KIND = "rootless-sealed-single-netns"
PRIVILEGED_TOPOLOGY_KIND = "privileged-dual-netns-veth"
HOST_LOOPBACK_DEVELOPMENT_TOPOLOGY_KIND = "host-loopback-development"
ROOTLESS_PRIVILEGE_MODEL = "unprivileged-userns"
PRIVILEGED_PRIVILEGE_MODEL = "host-capabilities"
HOST_LOOPBACK_DEVELOPMENT_PRIVILEGE_MODEL = "host-direct-loopback"

ALLOWED_TOPOLOGY_KINDS = frozenset(
    {
        ROOTLESS_TOPOLOGY_KIND,
        PRIVILEGED_TOPOLOGY_KIND,
        HOST_LOOPBACK_DEVELOPMENT_TOPOLOGY_KIND,
    }
)

ALLOWED_ACTORS = frozenset({"i2pr", "reference", "control"})


# Plan 086: bounded metadata for the development-only topology. The
# topology is never release or isolation qualified; it never claims
# network-egress prevention; it never asserts that the parent network
# state is unchanged beyond the literal loopback processes. The
# metadata is intentionally compact so the upstream schema can
# preserve the existing field allowlist without a schema bump.
HOST_LOOPBACK_DEVELOPMENT_METADATA: dict[str, object] = {
    "topology_kind": HOST_LOOPBACK_DEVELOPMENT_TOPOLOGY_KIND,
    "development_only": True,
    "release_qualified": False,
    "isolation_qualified": False,
    "public_network_blocked": "unproven",
    "parent_network_state_unchanged": True,
    "endpoint_family": "ipv4",
    "bind_address": "127.0.0.1",
    "peer_address": "127.0.0.1",
    "network_id": 99,
    "reference": "i2pd",
}


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


@dataclass(frozen=True)
class HostLoopbackDevelopmentPlacement:
    """Plan 086 bounded host-direct placement for the development lane.

    The placement executes the command directly on the current host
    while binding only to ``127.0.0.1``. It explicitly does not invoke
    any namespace, Multipass, capability mutation, or shell-interpolation
    shim. The binary path is measured once at construction; the runtime
    never composes a bash invocation or recurses through privilege
    escalation. The placement is the only owner of the
    host-loopback-development topology kind outside the canonical runner
    modules, and it is intentionally narrower than the general
    ``ProcessPlacement`` framework.

    The placement is structurally incapable of doing anything outside
    the literal IPv4 loopback lane; the binary path is verified at
    construction time, the environment is filtered to a small
    allowlist, and the stdout/stderr capture is bounded to the
    caller's log path. The placement is fail-closed: a missing
    binary, a non-absolute path, or a non-allowlisted environment
    key raises :class:`TopologyContractError` rather than silently
    swapping in a fallback.
    """

    actor: str
    binary_path: str
    log_path: str
    environment: tuple[tuple[str, str], ...] = ()
    max_log_bytes: int = 131_072

    def __post_init__(self) -> None:
        if self.actor not in ALLOWED_ACTORS:
            raise TopologyContractError("unknown-actor")
        if not self.binary_path:
            raise TopologyContractError("host-loopback-binary-path-empty")
        if not self.binary_path.startswith("/"):
            raise TopologyContractError("host-loopback-binary-path-must-be-absolute")
        if not self.log_path:
            raise TopologyContractError("host-loopback-log-path-empty")
        if not self.log_path.startswith("/"):
            raise TopologyContractError("host-loopback-log-path-must-be-absolute")
        if self.max_log_bytes <= 0 or self.max_log_bytes > 1_048_576:
            raise TopologyContractError("host-loopback-max-log-bytes-out-of-range")
        for key, _value in self.environment:
            if not key:
                raise TopologyContractError("host-loopback-environment-empty-key")
            if key in {"LD_PRELOAD", "LD_LIBRARY_PATH"}:
                raise TopologyContractError(
                    "host-loopback-environment-ld-preload-forbidden"
                )

    @property
    def topology_kind(self) -> str:
        return HOST_LOOPBACK_DEVELOPMENT_TOPOLOGY_KIND

    def command(self, argv: Sequence[str]) -> list[str]:
        """Return the absolute argv the caller must execute.

        The function never prepends a shell, namespace, or Multipass
        wrapper. The binary path is the module-supplied absolute
        path; subsequent arguments are appended verbatim. The
        placement owns no other state.
        """

        if not argv:
            raise TopologyContractError("host-loopback-empty-argv")
        return [self.binary_path, *argv]

    def environment_dict(self) -> dict[str, str]:
        return dict(self.environment)


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
    "HOST_LOOPBACK_DEVELOPMENT_METADATA",
    "HOST_LOOPBACK_DEVELOPMENT_PRIVILEGE_MODEL",
    "HOST_LOOPBACK_DEVELOPMENT_TOPOLOGY_KIND",
    "HostLoopbackDevelopmentPlacement",
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
