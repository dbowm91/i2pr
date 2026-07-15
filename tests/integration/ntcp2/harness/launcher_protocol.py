"""Strict launcher scenario and status protocol shared by the Python harness."""

from __future__ import annotations

import ipaddress
import json
import re
import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Any


SCENARIO_SCHEMA = 1
STATUS_SCHEMA = 1
MAX_SCENARIO_BYTES = 64 * 1024
MAX_DEADLINE_MILLISECONDS = 3_600_000
PRIVATE_NETWORK_ID = 99

SCENARIO_FIELDS = frozenset(
    {
        "schema",
        "scenario_id",
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
    }
)
STATUS_FIELDS = frozenset(
    {"schema", "type", "scenario_id", "phase", "result", "reason_code", "counters"}
)
COUNTER_FIELDS = frozenset(
    {"listener_ready", "authenticated", "frames_sent", "frames_received", "i2np_sent", "i2np_received"}
)
SCENARIO_ID = re.compile(r"^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$")

PADDING_PROFILES = frozenset(
    {"minimum-variable-maximum", "representative", "boundary-and-maximum-plus-one"}
)
EXPECTED_RESULTS = frozenset(
    {
        "authenticated-handshake-and-bounded-i2np-exchange",
        "authenticated-handshake-and-bounded-i2np-exchange-or-explicit-environment-skip",
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
        "handshake_failed",
        "dial_failed",
        "data_phase_failed",
        "timeout",
        "cleanup_complete",
        "invalid_scenario_config",
        "scenario_role_mismatch",
        "status_output_unavailable",
    }
)


class LauncherScenarioError(ValueError):
    """A scenario is malformed or violates the launcher boundary."""


class LauncherStatusError(ValueError):
    """A launcher output line is not a valid typed status record."""


@dataclass(frozen=True)
class LauncherScenario:
    schema: int
    scenario_id: str
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
    if frozenset(value) != SCENARIO_FIELDS:
        raise LauncherScenarioError("scenario-fields-invalid")
    if value["schema"] != SCENARIO_SCHEMA:
        raise LauncherScenarioError("scenario-schema-unsupported")
    scenario_id = value["scenario_id"]
    if not isinstance(scenario_id, str) or len(scenario_id.encode()) > 64 or not SCENARIO_ID.fullmatch(scenario_id):
        raise LauncherScenarioError("scenario-id-invalid")
    role = value["role"]
    family = value["address_family"]
    if role not in {"initiator", "responder"} or family not in {"ipv4", "ipv6"}:
        raise LauncherScenarioError("scenario-role-or-family-invalid")
    local_address = _synthetic_address(value["local_address"], family)
    local_port = _port(value["local_port"])
    peer_address_value = value["peer_address"]
    peer_port_value = value["peer_port"]
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
    if value["peer_router_info"] is not None:
        peer_router_info = _confined_path(run_root, value["peer_router_info"])
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
    if value["smoke_message_profile"] != "delivery-status":
        raise LauncherScenarioError("smoke-message-profile-invalid")
    seed = value["deterministic_seed"]
    if seed is not None and (isinstance(seed, bool) or not isinstance(seed, int) or seed < 0):
        raise LauncherScenarioError("deterministic-seed-invalid")
    if value["expected_result_class"] not in EXPECTED_RESULTS:
        raise LauncherScenarioError("expected-result-invalid")
    status_path = _confined_path(run_root, value["status_path"])
    if status_path.exists() and status_path.is_dir():
        raise LauncherScenarioError("status-path-is-directory")
    return LauncherScenario(
        schema=value["schema"],
        scenario_id=scenario_id,
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
    for counter in counters.values():
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
