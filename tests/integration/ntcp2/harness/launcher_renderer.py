"""Strict launcher scenario renderer for Plan 044 mixed-router directions."""

from __future__ import annotations

import ipaddress
import re
from pathlib import Path
from typing import Any

try:
    from .launcher_protocol import (
        PRIVATE_NETWORK_ID,
        REFERENCE_DRIVER_MODE_BY_DIRECTION,
        REFERENCE_DRIVER_MODES,
        SCENARIO_SCHEMA,
        SCENARIO_SCHEMA_VERSION,
        LauncherScenarioError,
        load_launcher_scenario,
    )
except ImportError:
    from launcher_protocol import (  # type: ignore
        PRIVATE_NETWORK_ID,
        REFERENCE_DRIVER_MODE_BY_DIRECTION,
        REFERENCE_DRIVER_MODES,
        SCENARIO_SCHEMA,
        SCENARIO_SCHEMA_VERSION,
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
_VALID_SMOKES = frozenset({"delivery-status", "fixed-12-byte-payload"})
_VALID_RESULTS = frozenset({
    "authenticated-handshake-and-bounded-i2np-exchange",
    "authenticated-handshake-and-directional-data-phase",
})
_VALID_DATA_PHASE_MODES = frozenset(
    {
        "handshake-only",
        "initiator-data-only",
        "responder-data-only",
        "round-trip-delivery-status",
    }
)
_VALID_PEER_ACTIONS = frozenset(
    {"observe-receive", "ignore-receive", "non-echo-completion"}
)
_VALID_OBSERVATIONS = frozenset(
    {
        "i2pr-sent-and-acknowledged",
        "i2pr-received-from-peer",
        "i2pr-sent-only",
        "i2pr-received-only",
        "no-data-phase-required",
    }
)
_MIXED_SCENARIO_ID = re.compile(r"^[a-z0-9](?:[a-z0-9-]{0,60}[a-z0-9])?$")
_RUN_ID = re.compile(r"^[a-z0-9](?:[a-z0-9-]{0,46}[a-z0-9])?$")
_HEX64 = re.compile(r"^[0-9a-f]{64}$")


class RenderError(ValueError):
    """A mixed direction cannot be rendered into a valid launcher scenario."""


def render_scenario_toml(
    *,
    execution_id: str,
    run_id: str,
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
    data_phase_mode: str = "round-trip-delivery-status",
    data_phase_required_peer_action: str = "non-echo-completion",
    data_phase_timeout_ms: int | None = None,
    expected_observation: str = "i2pr-sent-and-acknowledged",
    delivery_status_message_id: int,
    expected_sender_router_hash_sha256: str,
    expected_receiver_router_hash_sha256: str,
    reference_driver_mode: str,
    run_identity_sha256: str,
) -> str:
    _validate_inputs(
        execution_id=execution_id,
        run_id=run_id,
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
        data_phase_mode=data_phase_mode,
        data_phase_required_peer_action=data_phase_required_peer_action,
        data_phase_timeout_ms=data_phase_timeout_ms,
        expected_observation=expected_observation,
        delivery_status_message_id=delivery_status_message_id,
        expected_sender_router_hash_sha256=expected_sender_router_hash_sha256,
        expected_receiver_router_hash_sha256=expected_receiver_router_hash_sha256,
        reference_driver_mode=reference_driver_mode,
        run_identity_sha256=run_identity_sha256,
    )
    lines = [
        "[scenario]",
        f'schema = "{SCENARIO_SCHEMA}"',
        f"schema_version = {SCENARIO_SCHEMA_VERSION}",
        f'scenario_id = "{execution_id}"',
        f'run_id = "{run_id}"',
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
    lines.append(f'data_phase_mode = "{data_phase_mode}"')
    lines.append(f'data_phase_required_peer_action = "{data_phase_required_peer_action}"')
    if data_phase_timeout_ms is not None:
        lines.append(f"data_phase_timeout_ms = {data_phase_timeout_ms}")
    lines.append(f'expected_observation = "{expected_observation}"')
    lines.append(f"delivery_status_message_id = {delivery_status_message_id}")
    lines.append(f'expected_sender_router_hash_sha256 = "{expected_sender_router_hash_sha256}"')
    lines.append(f'expected_receiver_router_hash_sha256 = "{expected_receiver_router_hash_sha256}"')
    lines.append(f'reference_driver_mode = "{reference_driver_mode}"')
    lines.append(f'run_identity_sha256 = "{run_identity_sha256}"')
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
    run_id: str,
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
    data_phase_mode: str,
    data_phase_required_peer_action: str,
    data_phase_timeout_ms: int | None,
    expected_observation: str,
    delivery_status_message_id: int,
    expected_sender_router_hash_sha256: str,
    expected_receiver_router_hash_sha256: str,
    reference_driver_mode: str,
    run_identity_sha256: str,
) -> None:
    if not isinstance(execution_id, str) or not _MIXED_SCENARIO_ID.fullmatch(execution_id):
        raise RenderError("execution-id-invalid")
    if not isinstance(run_id, str) or not _RUN_ID.fullmatch(run_id):
        raise RenderError("run-id-invalid")
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
    if data_phase_mode not in _VALID_DATA_PHASE_MODES:
        raise RenderError("unsupported-data-phase-mode")
    if data_phase_required_peer_action not in _VALID_PEER_ACTIONS:
        raise RenderError("unsupported-data-phase-peer-action")
    if expected_observation not in _VALID_OBSERVATIONS:
        raise RenderError("unsupported-expected-observation")
    if data_phase_timeout_ms is not None and (
        not isinstance(data_phase_timeout_ms, int) or data_phase_timeout_ms <= 0
    ):
        raise RenderError("data-phase-timeout-invalid")
    _validate_relative_path(status_path, "status_path")
    # Plan 065: DeliveryStatus message ID is mandatory and must be a
    # nonzero unsigned 32-bit integer.
    if (
        isinstance(delivery_status_message_id, bool)
        or not isinstance(delivery_status_message_id, int)
        or delivery_status_message_id < 1
        or delivery_status_message_id > 0xFFFFFFFF
    ):
        raise RenderError("delivery-status-message-id-invalid")
    # Plan 065: expected sender and receiver Router Hashes are
    # mandatory and must be 64 lowercase hex characters with nonzero
    # provenance and no self-reference.
    _validate_router_hash("expected_sender_router_hash_sha256", expected_sender_router_hash_sha256)
    _validate_router_hash(
        "expected_receiver_router_hash_sha256", expected_receiver_router_hash_sha256
    )
    if expected_sender_router_hash_sha256 == expected_receiver_router_hash_sha256:
        raise RenderError("expected-router-hash-self-reference")
    # Plan 065: reference_driver_mode must be one of the two
    # source-locked direct helpers and must match the direction encoded
    # by ``execution_id``.
    if reference_driver_mode not in REFERENCE_DRIVER_MODES:
        raise RenderError("unsupported-reference-driver-mode")
    expected_mode_for_direction = REFERENCE_DRIVER_MODE_BY_DIRECTION.get(execution_id)
    if expected_mode_for_direction is None:
        raise RenderError("scenario-id-not-allowlisted")
    if reference_driver_mode != expected_mode_for_direction:
        raise RenderError("reference-driver-mode-direction-mismatch")
    if (
        not isinstance(run_identity_sha256, str)
        or not _HEX64.fullmatch(run_identity_sha256)
        or run_identity_sha256 == "0" * 64
    ):
        raise RenderError("run-identity-sha256-invalid")


def _validate_router_hash(label: str, value: str) -> None:
    if (
        not isinstance(value, str)
        or not _HEX64.fullmatch(value)
        or value == "0" * 64
    ):
        raise RenderError(f"{label}-invalid")


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
