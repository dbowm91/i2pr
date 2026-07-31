"""Strict launcher scenario and status protocol shared by the Python harness."""

from __future__ import annotations

import ipaddress
import json
import re
import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Any


SCENARIO_SCHEMA = "i2pr-launcher-scenario-v2"
SCENARIO_SCHEMA_VERSION = 2
STATUS_SCHEMA = 1
MAX_SCENARIO_BYTES = 64 * 1024
MAX_DEADLINE_MILLISECONDS = 3_600_000
PRIVATE_NETWORK_ID = 99

# Plan 065: required primary fields for a Plan 065 primary direction.
# The strict parser refuses any scenario that is missing the
# DeliveryStatus message ID, the expected Router Hashes, the
# reference_driver_mode, or the run_identity_sha256 fields.
SCENARIO_FIELDS = frozenset(
    {
        "schema",
        "schema_version",
        "scenario_id",
        "run_id",
        "role",
        "address_family",
        "local_address",
        "local_port",
        "peer_address",
        "peer_port",
        "network_id",
        "state_dir",
        "peer_router_info",
        "handshake_deadline_ms",
        "read_deadline_ms",
        "write_deadline_ms",
        "queue_deadline_ms",
        "drain_deadline_ms",
        "padding_profile",
        "smoke_message_profile",
        "deterministic_seed",
        "expected_result_class",
        "status_path",
        "delivery_status_message_id",
        "expected_sender_router_hash_sha256",
        "expected_receiver_router_hash_sha256",
        "reference_driver_mode",
        "run_identity_sha256",
    }
)
# Optional fields a renderer may add to a Plan 045 scenario schema. They are
# allowlisted explicitly; any other unknown field is rejected. The pair of
# directional data-phase selectors replaces the prior round-trip DeliveryStatus
# expectation that Java I2P and i2pd do not implement for inbound smoke
# scenarios.
ACCEPTED_OPTIONAL_SCENARIO_FIELDS = frozenset(
    {
        "data_phase_mode",
        "data_phase_required_peer_action",
        "data_phase_timeout_ms",
        "expected_observation",
    }
)
# Plan 065: the allowlisted reference driver mode set. Only the
# source-locked direct helpers from Plan 063 (Java) and Plan 064 (i2pd)
# satisfy a primary direction; SAM, HTTP, I2PControl, support-topology,
# and synthetic-fallback modes are explicitly rejected.
REFERENCE_DRIVER_MODES = frozenset({"java-direct-driver", "i2pd-direct-driver"})
REFERENCE_DRIVER_MODE_BY_DIRECTION = {
    "i2pr-to-java-ipv4": "java-direct-driver",
    "java-to-i2pr-ipv4": "java-direct-driver",
    "i2pr-to-i2pd-ipv4": "i2pd-direct-driver",
    "i2pd-to-i2pr-ipv4": "i2pd-direct-driver",
}
STATUS_FIELDS = frozenset(
    {"schema", "type", "scenario_id", "phase", "result", "reason_code", "counters"}
)
COUNTER_FIELDS = frozenset(
    {
        "listener_ready",
        "authenticated",
        "frames_sent",
        "frames_received",
        "i2np_sent",
        "i2np_received",
        # Plan 065: the per-run DeliveryStatus correlation counters. The
        # canonical mixed-runner cross-checks the message ID and the
        # expected peer Router Hash against the trigger record.
        "delivery_status_message_id",
        "expected_peer_router_hash_sha256",
    }
)
SCENARIO_ID = re.compile(r"^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$")
HEX64 = re.compile(r"^[0-9a-f]{64}$")

PADDING_PROFILES = frozenset(
    {"minimum-variable-maximum", "representative", "boundary-and-maximum-plus-one"}
)
# Plan 045 directional data-phase modes. `initiator-data-only` means the
# i2pr initiator sends exactly one fixed I2NP message and records
# observation on its side; the reference responder is not required to send
# back a DeliveryStatus. `responder-data-only` means i2pr only receives
# the data phase that the reference initiator sends; i2pr does not echo.
DATA_PHASE_MODES = frozenset(
    {
        "handshake-only",
        "initiator-data-only",
        "responder-data-only",
        "round-trip-delivery-status",
    }
)
DATA_PHASE_PEER_ACTIONS = frozenset(
    {"observe-receive", "ignore-receive", "non-echo-completion"}
)
EXPECTED_OBSERVATIONS = frozenset(
    {
        "i2pr-sent-and-acknowledged",
        "i2pr-received-from-peer",
        "i2pr-sent-only",
        "i2pr-received-only",
        "no-data-phase-required",
    }
)
SMOKE_MESSAGE_PROFILES = frozenset(
    {"delivery-status", "fixed-12-byte-payload"}
)
EXPECTED_RESULTS = frozenset(
    {
        "authenticated-handshake-and-bounded-i2np-exchange",
        "authenticated-handshake-and-bounded-i2np-exchange-or-explicit-environment-skip",
        "authenticated-handshake-and-directional-data-phase",
        "typed-rejection-with-bounded-cleanup",
        "deterministic-winner-and-loser-drain",
    }
)
STATUS_PHASES = frozenset({"listener_ready", "terminal"})
STATUS_RESULTS = frozenset(
    {"ready", "passed", "blocked", "rejected", "timeout", "authentication_failed", "cleanup_failed"}
)
STATUS_REASONS = frozenset(
    {
        "listener_bound",
        "state_invalid",
        "peer_router_info_invalid",
        "unsupported_padding_profile",
        "listener_failed",
        "handshake_authenticated",
        "i2np_exchange_complete",
        "directional_data_phase_complete",
        "handshake_failed",
        "dial_failed",
        "data_phase_failed",
        "data_phase_timeout",
        "data_phase_observation_incomplete",
        "timeout",
        "cleanup_complete",
        "invalid_scenario_config",
        "scenario_role_mismatch",
        "status_output_unavailable",
        # Plan 052 G1: bounded responder-stage classification.
        "responder_tcp_accept_missing",
        "responder_admission_rejected",
        "responder_message1_decode_failed",
        "responder_message1_options_invalid",
        "responder_noise_state_failed",
        "responder_session_created_write_failed",
        "responder_session_confirmed_part1_failed",
        "responder_session_confirmed_part2_failed",
        "responder_router_identity_verification_failed",
        "responder_handshake_timeout",
        "responder_authenticated_link_install_failed",
        "responder_data_frame_read_failed",
        "responder_i2np_decode_failed",
        # Plan 065: bounded sender-side and receiver-side typed failures.
        "sender_delivery_status_message_id_zero",
        "sender_router_identity_mismatch",
        "sender_delivery_status_construction_failed",
        "sender_frame_queue_ambiguous",
        "sender_frame_write_failed",
        "sender_multiple_primary_delivery_status_emitted",
        "sender_cancellation_observed",
        "receiver_frame_read_failed",
        "receiver_frame_authentication_failed",
        "receiver_i2np_decode_failed",
        "receiver_delivery_status_missing",
        "receiver_delivery_status_id_mismatch",
        "receiver_delivery_status_duplicate",
        "receiver_peer_identity_mismatch",
        "receiver_delivery_status_timestamp_invalid",
    }
)


class LauncherScenarioError(ValueError):
    """A scenario is malformed or violates the launcher boundary."""


class LauncherStatusError(ValueError):
    """A launcher output line is not a valid typed status record."""


@dataclass(frozen=True)
class LauncherScenario:
    schema: str
    schema_version: int
    scenario_id: str
    run_id: str
    role: str
    address_family: str
    local_address: str
    local_port: int
    peer_address: str | None
    peer_port: int | None
    network_id: int
    run_root: Path
    state_dir: Path
    peer_router_info: Path | None
    handshake_deadline_ms: int
    read_deadline_ms: int
    write_deadline_ms: int
    queue_deadline_ms: int
    drain_deadline_ms: int
    padding_profile: str
    smoke_message_profile: str
    deterministic_seed: int | None
    expected_result_class: str
    status_path: Path
    data_phase_mode: str = "round-trip-delivery-status"
    data_phase_required_peer_action: str = "non-echo-completion"
    data_phase_timeout_ms: int | None = None
    expected_observation: str = "i2pr-sent-and-acknowledged"
    delivery_status_message_id: int = 0
    expected_sender_router_hash_sha256: str = ""
    expected_receiver_router_hash_sha256: str = ""
    reference_driver_mode: str = ""
    run_identity_sha256: str = ""


def load_launcher_scenario(path: Path) -> LauncherScenario:
    """Read and validate the exact launcher scenario schema."""

    try:
        if not path.is_file() or path.stat().st_size > MAX_SCENARIO_BYTES:
            raise LauncherScenarioError("scenario-file-invalid")
        raw = tomllib.loads(path.read_text(encoding="utf-8"))
    except LauncherScenarioError:
        raise
    except (OSError, UnicodeError, tomllib.TOMLDecodeError) as exc:
        raise LauncherScenarioError("scenario-toml-invalid") from exc
    if set(raw) != {"scenario"} or not isinstance(raw["scenario"], dict):
        raise LauncherScenarioError("scenario-table-invalid")
    value = raw["scenario"]
    fields = frozenset(value)
    if not fields.issuperset(SCENARIO_FIELDS):
        raise LauncherScenarioError("scenario-fields-missing")
    extra = fields - SCENARIO_FIELDS
    if not extra.issubset(ACCEPTED_OPTIONAL_SCENARIO_FIELDS):
        raise LauncherScenarioError("scenario-fields-invalid")
    if value["schema"] != SCENARIO_SCHEMA:
        raise LauncherScenarioError("scenario-schema-unsupported")
    if value["schema_version"] != SCENARIO_SCHEMA_VERSION:
        raise LauncherScenarioError("scenario-schema-version-unsupported")
    scenario_id = value["scenario_id"]
    if not isinstance(scenario_id, str) or len(scenario_id.encode()) > 64 or not SCENARIO_ID.fullmatch(scenario_id):
        raise LauncherScenarioError("scenario-id-invalid")
    run_id = value["run_id"]
    if not isinstance(run_id, str) or len(run_id.encode()) > 64 or not SCENARIO_ID.fullmatch(run_id):
        raise LauncherScenarioError("run-id-invalid")
    role = value["role"]
    family = value["address_family"]
    if role not in {"initiator", "responder"} or family not in {"ipv4", "ipv6"}:
        raise LauncherScenarioError("scenario-role-or-family-invalid")
    local_address = _synthetic_address(value["local_address"], family)
    local_port = _port(value["local_port"])
    peer_address_value = value["peer_address"]
    peer_port_value = value["peer_port"]
    if isinstance(peer_address_value, str) and peer_address_value == "":
        peer_address_value = None
    if isinstance(peer_port_value, int) and peer_port_value == 0:
        peer_port_value = None
    if (peer_address_value is None) != (peer_port_value is None):
        raise LauncherScenarioError("peer-endpoint-incomplete")
    if role == "initiator" and peer_address_value is None:
        raise LauncherScenarioError("initiator-peer-missing")
    if role == "responder" and peer_address_value is not None:
        raise LauncherScenarioError("responder-peer-present")
    peer_address = None
    peer_port = None
    if peer_address_value is not None and peer_port_value is not None:
        peer_address = _synthetic_address(peer_address_value, family)
        peer_port = _port(peer_port_value)
        if peer_address == local_address and peer_port == local_port:
            raise LauncherScenarioError("duplicate-endpoint")
    network_id = value["network_id"]
    if isinstance(network_id, bool) or not isinstance(network_id, int) or network_id != PRIVATE_NETWORK_ID:
        raise LauncherScenarioError("network-id-unsupported")
    run_root = path.resolve().parent
    state_dir = _confined_path(run_root, value["state_dir"])
    if state_dir.exists() and not state_dir.is_dir():
        raise LauncherScenarioError("state-path-is-file")
    peer_router_info = None
    peer_router_info_raw = value["peer_router_info"]
    if isinstance(peer_router_info_raw, str) and peer_router_info_raw == "":
        peer_router_info_raw = None
    if peer_router_info_raw is not None:
        peer_router_info = _confined_path(run_root, peer_router_info_raw)
    if role == "initiator" and peer_router_info is None:
        raise LauncherScenarioError("initiator-router-info-missing")
    if role == "responder" and peer_router_info is not None:
        raise LauncherScenarioError("responder-router-info-present")
    deadlines = {
        key: _deadline(value[key])
        for key in ("handshake_deadline_ms", "read_deadline_ms", "write_deadline_ms", "queue_deadline_ms", "drain_deadline_ms")
    }
    if value["padding_profile"] not in PADDING_PROFILES:
        raise LauncherScenarioError("padding-profile-invalid")
    if value["smoke_message_profile"] not in SMOKE_MESSAGE_PROFILES:
        raise LauncherScenarioError("smoke-message-profile-invalid")
    seed = value["deterministic_seed"]
    if seed is not None and (isinstance(seed, bool) or not isinstance(seed, int) or seed < 0):
        raise LauncherScenarioError("deterministic-seed-invalid")
    if value["expected_result_class"] not in EXPECTED_RESULTS:
        raise LauncherScenarioError("expected-result-invalid")
    data_phase_mode = value.get("data_phase_mode", "round-trip-delivery-status")
    if data_phase_mode not in DATA_PHASE_MODES:
        raise LauncherScenarioError("data-phase-mode-invalid")
    peer_action = value.get("data_phase_required_peer_action", "non-echo-completion")
    if peer_action not in DATA_PHASE_PEER_ACTIONS:
        raise LauncherScenarioError("data-phase-peer-action-invalid")
    observation = value.get("expected_observation", "i2pr-sent-and-acknowledged")
    if observation not in EXPECTED_OBSERVATIONS:
        raise LauncherScenarioError("expected-observation-invalid")
    timeout_ms = value.get("data_phase_timeout_ms")
    if timeout_ms is not None and (
        isinstance(timeout_ms, bool)
        or not isinstance(timeout_ms, int)
        or timeout_ms <= 0
        or timeout_ms > MAX_DEADLINE_MILLISECONDS
    ):
        raise LauncherScenarioError("data-phase-timeout-invalid")
    status_path = _confined_path(run_root, value["status_path"])
    if status_path.exists() and status_path.is_dir():
        raise LauncherScenarioError("status-path-is-directory")
    # Plan 065: DeliveryStatus message ID is mandatory and nonzero for
    # every primary direction. The strict parser refuses zero or
    # negative IDs and any value outside ``1..=0xffffffff``.
    message_id = value["delivery_status_message_id"]
    if (
        isinstance(message_id, bool)
        or not isinstance(message_id, int)
        or message_id < 1
        or message_id > 0xFFFFFFFF
    ):
        raise LauncherScenarioError("delivery-status-message-id-invalid")
    # Plan 065: expected sender and receiver Router Hashes are mandatory
    # and must be 64 lowercase hex characters.
    expected_sender = value["expected_sender_router_hash_sha256"]
    expected_receiver = value["expected_receiver_router_hash_sha256"]
    if (
        not isinstance(expected_sender, str)
        or not HEX64.fullmatch(expected_sender)
        or expected_sender == "0" * 64
    ):
        raise LauncherScenarioError("expected-sender-router-hash-invalid")
    if (
        not isinstance(expected_receiver, str)
        or not HEX64.fullmatch(expected_receiver)
        or expected_receiver == "0" * 64
    ):
        raise LauncherScenarioError("expected-receiver-router-hash-invalid")
    if expected_sender == expected_receiver:
        raise LauncherScenarioError("expected-router-hash-self-reference")
    # Plan 065: reference_driver_mode must be one of the two
    # source-locked direct helpers and must match the direction encoded
    # by ``scenario_id``. SAM, HTTP, I2PControl, support-topology, and
    # synthetic-fallback modes are explicitly rejected for primary
    # directions.
    reference_driver_mode = value["reference_driver_mode"]
    if reference_driver_mode not in REFERENCE_DRIVER_MODES:
        raise LauncherScenarioError("reference-driver-mode-invalid")
    expected_mode_for_direction = REFERENCE_DRIVER_MODE_BY_DIRECTION.get(scenario_id)
    if expected_mode_for_direction is None:
        raise LauncherScenarioError("scenario-id-not-allowlisted")
    if reference_driver_mode != expected_mode_for_direction:
        raise LauncherScenarioError("reference-driver-mode-direction-mismatch")
    # Plan 065: run_identity_sha256 is mandatory and must be 64
    # lowercase hex characters.
    run_identity = value["run_identity_sha256"]
    if (
        not isinstance(run_identity, str)
        or not HEX64.fullmatch(run_identity)
        or run_identity == "0" * 64
    ):
        raise LauncherScenarioError("run-identity-sha256-invalid")
    return LauncherScenario(
        schema=value["schema"],
        schema_version=value["schema_version"],
        scenario_id=scenario_id,
        run_id=run_id,
        role=role,
        address_family=family,
        local_address=str(local_address),
        local_port=local_port,
        peer_address=str(peer_address) if peer_address is not None else None,
        peer_port=peer_port,
        network_id=network_id,
        run_root=run_root,
        state_dir=state_dir,
        peer_router_info=peer_router_info,
        handshake_deadline_ms=deadlines["handshake_deadline_ms"],
        read_deadline_ms=deadlines["read_deadline_ms"],
        write_deadline_ms=deadlines["write_deadline_ms"],
        queue_deadline_ms=deadlines["queue_deadline_ms"],
        drain_deadline_ms=deadlines["drain_deadline_ms"],
        padding_profile=value["padding_profile"],
        smoke_message_profile=value["smoke_message_profile"],
        deterministic_seed=seed,
        expected_result_class=value["expected_result_class"],
        status_path=status_path,
        data_phase_mode=data_phase_mode,
        data_phase_required_peer_action=peer_action,
        data_phase_timeout_ms=timeout_ms,
        expected_observation=observation,
        delivery_status_message_id=message_id,
        expected_sender_router_hash_sha256=expected_sender,
        expected_receiver_router_hash_sha256=expected_receiver,
        reference_driver_mode=reference_driver_mode,
        run_identity_sha256=run_identity,
    )


def parse_status_line(line: str) -> dict[str, Any]:
    """Validate one launcher JSON status line without retaining diagnostics."""

    try:
        value = json.loads(line)
    except (UnicodeError, json.JSONDecodeError) as exc:
        raise LauncherStatusError("status-json-invalid") from exc
    if not isinstance(value, dict) or frozenset(value) != STATUS_FIELDS:
        raise LauncherStatusError("status-shape-invalid")
    if value["schema"] != STATUS_SCHEMA or value["type"] != "i2pr-interop-status":
        raise LauncherStatusError("status-schema-invalid")
    if not isinstance(value["scenario_id"], str) or not SCENARIO_ID.fullmatch(value["scenario_id"]):
        raise LauncherStatusError("status-scenario-id-invalid")
    phase = value["phase"]
    result = value["result"]
    reason = value["reason_code"]
    if phase not in STATUS_PHASES or result not in STATUS_RESULTS or reason not in STATUS_REASONS:
        raise LauncherStatusError("status-category-invalid")
    if phase == "listener_ready" and (result != "ready" or reason != "listener_bound"):
        raise LauncherStatusError("status-readiness-invalid")
    if phase == "terminal" and result == "ready":
        raise LauncherStatusError("status-terminal-ready-invalid")
    counters = value["counters"]
    if not isinstance(counters, dict) or frozenset(counters) != COUNTER_FIELDS:
        raise LauncherStatusError("status-counters-invalid")
    # Plan 065: the per-run DeliveryStatus message ID counter is an
    # unsigned 32-bit integer. The strict parser refuses bool, negative,
    # or out-of-range values; the canonical mixed-runner cross-checks
    # the counter against the trigger record.
    message_id = counters["delivery_status_message_id"]
    if (
        isinstance(message_id, bool)
        or not isinstance(message_id, int)
        or message_id < 0
        or message_id > 0xFFFFFFFF
    ):
        raise LauncherStatusError("status-counter-delivery-status-message-id-out-of-range")
    peer_hash = counters["expected_peer_router_hash_sha256"]
    if not isinstance(peer_hash, str) or (peer_hash and not HEX64.fullmatch(peer_hash)):
        raise LauncherStatusError("status-counter-expected-peer-router-hash-invalid")
    for counter_name in (
        "listener_ready",
        "authenticated",
        "frames_sent",
        "frames_received",
        "i2np_sent",
        "i2np_received",
    ):
        counter = counters[counter_name]
        if isinstance(counter, bool) or not isinstance(counter, int) or not 0 <= counter <= 1_000_000:
            raise LauncherStatusError("status-counter-out-of-range")
    return value


def _synthetic_address(value: Any, family: str) -> ipaddress._BaseAddress:
    if not isinstance(value, str):
        raise LauncherScenarioError("address-not-literal")
    try:
        address = ipaddress.ip_address(value)
    except ValueError as exc:
        raise LauncherScenarioError("address-not-literal") from exc
    if (family == "ipv4" and not isinstance(address, ipaddress.IPv4Address)) or (
        family == "ipv6" and not isinstance(address, ipaddress.IPv6Address)
    ):
        raise LauncherScenarioError("address-family-mismatch")
    allowed = (
        address in ipaddress.ip_network("192.0.2.0/24") and int(address) & 0xFF != 0
        if family == "ipv4"
        else address in ipaddress.ip_network("2001:db8:36::/64") and not address.is_unspecified
    )
    if not allowed:
        raise LauncherScenarioError("address-outside-synthetic-range")
    return address


def _port(value: Any) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or not 1 <= value <= 65535:
        raise LauncherScenarioError("port-invalid")
    return value


def _deadline(value: Any) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or not 1 <= value <= MAX_DEADLINE_MILLISECONDS:
        raise LauncherScenarioError("deadline-invalid")
    return value


def _confined_path(run_root: Path, value: Any) -> Path:
    if not isinstance(value, str) or not value or "\x00" in value:
        raise LauncherScenarioError("path-invalid")
    relative = Path(value)
    if relative.is_absolute() or any(part in {"..", ""} for part in relative.parts if part != "."):
        raise LauncherScenarioError("path-invalid")
    candidate = (run_root / relative).resolve(strict=False)
    try:
        candidate.relative_to(run_root)
    except ValueError as exc:
        raise LauncherScenarioError("path-outside-run-root") from exc
    return candidate
