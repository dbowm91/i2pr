"""Strict launcher scenario renderer for Plan 044 mixed-router directions."""

from __future__ import annotations

import ipaddress
import re
from pathlib import Path
from typing import Any

try:
    from .launcher_protocol import (
        PRIVATE_NETWORK_ID,
        SCENARIO_SCHEMA,
        LauncherScenarioError,
        load_launcher_scenario,
    )
except ImportError:
    from launcher_protocol import (  # type: ignore
        PRIVATE_NETWORK_ID,
        SCENARIO_SCHEMA,
        LauncherScenarioError,
        load_launcher_scenario,
    )


_SYNTHETIC_IPV4_NETWORK = ipaddress.ip_network("192.0.2.0/24")
_SYNTHETIC_IPV6_NETWORK = ipaddress.ip_network("2001:db8:36::/64")
_DEFAULT_HANDSHAKE_MS = 30_000
_DEFAULT_READ_MS = 5_000
_DEFAULT_WRITE_MS = 5_000
_DEFAULT_QUEUE_MS = 2_000
_DEFAULT_DRAIN_MS = 2_000
_VALID_ROLES = frozenset({"initiator", "responder"})
_VALID_FAMILIES = frozenset({"ipv4", "ipv6"})
_VALID_PADDING = frozenset({"minimum-variable-maximum"})
_VALID_SMOKES = frozenset({"delivery-status"})
_VALID_RESULTS = frozenset({
    "authenticated-handshake-and-bounded-i2np-exchange",
})
_MIXED_SCENARIO_ID = re.compile(r"^[a-z0-9](?:[a-z0-9-]{0,60}[a-z0-9])?$")


class RenderError(ValueError):
    """A mixed direction cannot be rendered into a valid launcher scenario."""


def render_scenario_toml(
    *,
    execution_id: str,
    role: str,
    address_family: str,
    local_address: str,
    local_port: int,
    peer_address: str | None,
    peer_port: int | None,
    state_dir: str,
    peer_router_info: str | None,
    padding_profile: str = "minimum-variable-maximum",
    smoke_message_profile: str = "delivery-status",
    deterministic_seed: int | None = None,
    expected_result_class: str = "authenticated-handshake-and-bounded-i2np-exchange",
    status_path: str = "status.jsonl",
    handshake_deadline_ms: int = _DEFAULT_HANDSHAKE_MS,
    read_deadline_ms: int = _DEFAULT_READ_MS,
    write_deadline_ms: int = _DEFAULT_WRITE_MS,
    queue_deadline_ms: int = _DEFAULT_QUEUE_MS,
    drain_deadline_ms: int = _DEFAULT_DRAIN_MS,
) -> str:
    _validate_inputs(
        execution_id=execution_id,
        role=role,
        address_family=address_family,
        local_address=local_address,
        local_port=local_port,
        peer_address=peer_address,
        peer_port=peer_port,
        state_dir=state_dir,
        peer_router_info=peer_router_info,
        padding_profile=padding_profile,
        smoke_message_profile=smoke_message_profile,
        deterministic_seed=deterministic_seed,
        expected_result_class=expected_result_class,
        status_path=status_path,
    )
    lines = [
        "[scenario]",
        f"schema = {SCENARIO_SCHEMA}",
        f'scenario_id = "{execution_id}"',
        f'role = "{role}"',
        f'address_family = "{address_family}"',
        f'local_address = "{local_address}"',
        f"local_port = {local_port}",
    ]
    if peer_address is not None and peer_port is not None:
        lines.append(f'peer_address = "{peer_address}"')
        lines.append(f"peer_port = {peer_port}")
    else:
        lines.append('peer_address = ""')
        lines.append("peer_port = 0")
    lines.append(f"network_id = {PRIVATE_NETWORK_ID}")
    lines.append(f'state_dir = "{state_dir}"')
    if peer_router_info is not None:
        lines.append(f'peer_router_info = "{peer_router_info}"')
    else:
        lines.append('peer_router_info = ""')
    lines.append(f"handshake_deadline_ms = {handshake_deadline_ms}")
    lines.append(f"read_deadline_ms = {read_deadline_ms}")
    lines.append(f"write_deadline_ms = {write_deadline_ms}")
    lines.append(f"queue_deadline_ms = {queue_deadline_ms}")
    lines.append(f"drain_deadline_ms = {drain_deadline_ms}")
    lines.append(f'padding_profile = "{padding_profile}"')
    lines.append(f'smoke_message_profile = "{smoke_message_profile}"')
    if deterministic_seed is not None:
        lines.append(f"deterministic_seed = {deterministic_seed}")
    else:
        lines.append("deterministic_seed = 0")
    lines.append(f'expected_result_class = "{expected_result_class}"')
    lines.append(f'status_path = "{status_path}"')
    return "\n".join(lines) + "\n"


def render_and_validate(
    run_root: Path,
    **kwargs: Any,
) -> Path:
    content = render_scenario_toml(**kwargs)
    scenario_path = run_root / "scenario.toml"
    run_root.mkdir(parents=True, exist_ok=True)
    scenario_path.write_text(content, encoding="utf-8")
    try:
        load_launcher_scenario(scenario_path)
    except LauncherScenarioError as exc:
        scenario_path.unlink(missing_ok=True)
        raise RenderError(f"rendered-scenario-validation-failed: {exc}") from exc
    return scenario_path


def _validate_inputs(
    *,
    execution_id: str,
    role: str,
    address_family: str,
    local_address: str,
    local_port: int,
    peer_address: str | None,
    peer_port: str | int | None,
    state_dir: str,
    peer_router_info: str | None,
    padding_profile: str,
    smoke_message_profile: str,
    deterministic_seed: int | None,
    expected_result_class: str,
    status_path: str,
) -> None:
    if not isinstance(execution_id, str) or not _MIXED_SCENARIO_ID.fullmatch(execution_id):
        raise RenderError("execution-id-invalid")
    if role not in _VALID_ROLES:
        raise RenderError("role-invalid")
    if address_family not in _VALID_FAMILIES:
        raise RenderError("address-family-invalid")
    _validate_address(local_address, address_family, "local")
    _validate_port(local_port)
    if role == "initiator":
        if peer_address is None or peer_port is None:
            raise RenderError("initiator-peer-missing")
        _validate_address(peer_address, address_family, "peer")
        _validate_port(int(peer_port))
        if peer_address == local_address and int(peer_port) == local_port:
            raise RenderError("duplicate-endpoint")
    else:
        if peer_address is not None or peer_port is not None:
            raise RenderError("responder-peer-present")
    _validate_relative_path(state_dir, "state_dir")
    if peer_router_info is not None:
        _validate_relative_path(peer_router_info, "peer_router_info")
    elif role == "initiator":
        raise RenderError("initiator-router-info-missing")
    if padding_profile not in _VALID_PADDING:
        raise RenderError("unsupported-padding-profile")
    if smoke_message_profile not in _VALID_SMOKES:
        raise RenderError("unsupported-smoke-message-profile")
    if deterministic_seed is not None and (not isinstance(deterministic_seed, int) or deterministic_seed < 0):
        raise RenderError("deterministic-seed-invalid")
    if expected_result_class not in _VALID_RESULTS:
        raise RenderError("unsupported-expected-result")
    _validate_relative_path(status_path, "status_path")


def _validate_address(value: str, family: str, label: str) -> None:
    if not isinstance(value, str):
        raise RenderError(f"{label}-address-not-string")
    try:
        addr = ipaddress.ip_address(value)
    except ValueError as exc:
        raise RenderError(f"{label}-address-not-literal") from exc
    if family == "ipv4" and not isinstance(addr, ipaddress.IPv4Address):
        raise RenderError(f"{label}-address-family-mismatch")
    if family == "ipv6" and not isinstance(addr, ipaddress.IPv6Address):
        raise RenderError(f"{label}-address-family-mismatch")
    network = _SYNTHETIC_IPV4_NETWORK if family == "ipv4" else _SYNTHETIC_IPV6_NETWORK
    if addr not in network:
        raise RenderError(f"{label}-address-outside-synthetic-range")


def _validate_port(value: int) -> None:
    if isinstance(value, bool) or not isinstance(value, int) or not 1 <= value <= 65535:
        raise RenderError("port-invalid")


def _validate_relative_path(value: str, label: str) -> None:
    if not isinstance(value, str) or not value:
        raise RenderError(f"{label}-invalid")
    p = Path(value)
    if p.is_absolute():
        raise RenderError(f"{label}-absolute")
    if ".." in p.parts:
        raise RenderError(f"{label}-parent-traversal")
    if any(part == "" for part in p.parts):
        raise RenderError(f"{label}-empty-part")
