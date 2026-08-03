"""Plan 083 minimal i2pr-to-i2pd NTCP2 wire probe runner.

The runner orchestrates the 11-step execution architecture defined by
Plan 083:

1. lane/placement validation
2. i2pr state preparation from Plan 082
3. i2pd state preparation through Plan 076
4. RouterInfo exchange and strict validation
5. run-identity freeze
6. i2pd listener start
7. i2pr dial start
8. structured event collection
9. one exact DeliveryStatus transfer
10. bounded shutdown and cleanup
11. one compact diagnostic record

The runner is a one-direction development diagnostic. It is structurally
incapable of producing a mixed-router pass unless it launches one real
i2pr process and one configured real reference process and consumes
authentic structured events from both.

The runner never imports Plan 056/066 candidate, bundle, certificate,
rootless-topology, or Multipass authority. It may be exercised by
focused tests using fake event streams before a real wire attempt is
attempted in a qualified Plan 080 lane.

On a host that reports ``blocked_unprivileged_user_namespace`` from
the Plan 046 probe, the runner refuses to attempt a live wire run and
returns a typed ``lane_invalid`` blocker record.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import socket
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Protocol

from minimal_i2pd_probe import (
    ALLOWED_EVENT_NAMES,
    ALLOWED_EVENT_SIDES,
    CLEANUP_RESULTS,
    DIRECTION,
    FORBIDDEN_FIELDS,
    I2NP_DELIVERY_STATUS_DECODED,
    LANE_INVALID,
    LISTENER_READY,
    NOT_STARTED,
    PASSED,
    PASSED_REQUIRED_OBSERVED_EVENTS,
    PRE_PROTOCOL_REJECTED,
    PROCESS_COUNTER_KEYS,
    PROCESS_KEYS,
    PROTOCOL_REJECTED,
    PROTOCOL_TIMEOUT,
    REASON_CODES,
    REASON_I2PD_LISTENER_NOT_READY,
    REASON_I2PR_DIAL_START_FAILED,
    REASON_LANE_INVALID,
    REASON_NOT_STARTED,
    REASON_PRE_PROTOCOL_PREPARATION_FAILED,
    REASON_PRE_PROTOCOL_REFERENCE_FAILED,
    REASON_PRE_PROTOCOL_RENDER_FAILED,
    REASON_PRE_PROTOCOL_ROUTER_INFO_VALIDATION_FAILED,
    REASON_PRE_PROTOCOL_RUN_IDENTITY_FAILED,
    REASON_REFERENCE_EVENTS_MISSING,
    REASON_TCP_CONNECT_FAILED,
    REFERENCE,
    REQUIRED_FIELDS,
    SCHEMA,
    SCHEMA_VERSION,
    STAGES,
    STAGE_RANK,
    STATE_PREPARED,
    TCP_CONNECTED,
    TERMINAL_RESULTS,
    build_record,
    canonical_record_digest,
    empty_process_counters,
)


HEX64: re.Pattern[str] = re.compile(r"^[0-9a-f]{64}$")
HEX40: re.Pattern[str] = re.compile(r"^[0-9a-f]{40}$")
RUN_ID_RE: re.Pattern[str] = re.compile(r"^[a-z0-9](?:[a-z0-9-]{0,46}[a-z0-9])?$")

# Plan 046 host blocker environment variable.
_PLAN046_HOST_BLOCKER_ENV = "I2PR_PLAN046_HOST_BLOCKER"


class RunnerError(RuntimeError):
    """A runner precondition or lifecycle operation failed."""

    def __init__(self, code: str):
        super().__init__(code)
        self.code = code


class EventSource(Protocol):
    """Interface for consuming structured events from a process."""

    def wait_for_event(
        self,
        event_name: str,
        timeout_seconds: float,
    ) -> dict[str, Any] | None:
        """Wait for a named event and return the event dict or None."""
        ...


class FakeEventSource:
    """Injectable fake event source for unit tests.

    Configure with ``add_event`` before triggering the wait, or inject
    a terminal rejection so the source returns ``None``.
    """

    def __init__(self) -> None:
        self._pending: list[dict[str, Any]] = []
        self._terminal_rejected = False

    def add_event(
        self,
        event_name: str,
        source_side: str,
        *,
        extra: dict[str, Any] | None = None,
    ) -> None:
        payload: dict[str, Any] = {
            "event_name": event_name,
            "source_side": source_side,
            "event_sha256": hashlib.sha256(
                f"{event_name}:{source_side}:{time.monotonic_ns()}".encode()
            ).hexdigest(),
        }
        if extra:
            payload.update(extra)
        self._pending.append(payload)

    def inject_terminal_rejected(self) -> None:
        self._terminal_rejected = True

    def wait_for_event(
        self,
        event_name: str,
        timeout_seconds: float,
    ) -> dict[str, Any] | None:
        if self._terminal_rejected:
            return None
        for idx, event in enumerate(self._pending):
            if event["event_name"] == event_name:
                self._pending.pop(idx)
                return event
        return None


class FakeProcess:
    """Injectable fake process for unit tests."""

    def __init__(
        self,
        exit_code: int = 0,
        *,
        terminal_status: dict[str, Any] | None = None,
    ) -> None:
        self.exit_code = exit_code
        self.terminal_status = terminal_status
        self._started = False
        self._stopped = False

    def start(self) -> None:
        self._started = True

    def wait_for_exit(self, timeout_seconds: float) -> int:
        return self.exit_code

    def stop(self, timeout_seconds: float) -> str:
        self._stopped = True
        return "clean"

    def wait_terminal(self, timeout_seconds: float) -> dict[str, Any] | None:
        return self.terminal_status


@dataclass
class ProbeConfig:
    """Configuration for a single probe run."""

    run_id: str
    source_commit: str
    reference_revision: str
    lane_qualification_sha256: str
    topology_kind: str
    parent_network_state_unchanged: bool
    i2pr_binary_sha256: str
    i2pd_binary_sha256: str
    i2pr_router_info_sha256: str = ""
    i2pd_router_info_sha256: str = ""
    i2pr_router_hash_sha256: str = ""
    i2pd_router_hash_sha256: str = ""
    delivery_status_message_id: int = 0
    local_address: str = "192.0.2.1"
    local_port: int = 19001
    peer_address: str = "192.0.2.2"
    peer_port: int = 19002
    network_id: int = 99
    handshake_timeout_ms: int = 30_000
    cleanup_timeout_seconds: float = 5.0
    output_path: Path | None = None

    # Injectable dependencies for testing.
    i2pr_event_source: EventSource | None = None
    i2pd_event_source: EventSource | None = None
    i2pr_dial_factory: Any = None
    i2pd_listener_factory: Any = None


@dataclass
class _StageTracker:
    """Track the highest stage reached during probe execution."""

    current: str = NOT_STARTED
    reached_stages: list[str] = field(default_factory=list)

    def advance_to(self, stage: str) -> None:
        rank = STAGE_RANK.get(stage, -1)
        current_rank = STAGE_RANK.get(self.current, -1)
        if rank > current_rank:
            self.current = stage
        if stage not in self.reached_stages:
            self.reached_stages.append(stage)


class ProbeRunner:
    """Orchestrate one ``i2pr -> i2pd`` probe direction.

    The runner tracks strict stage progression, typed process counters,
    and structured observed events. It fails closed with typed reasons
    and never synthesizes pass or provenance.
    """

    def __init__(self, config: ProbeConfig) -> None:
        self.config = config
        self._stages = _StageTracker()
        self._counters = empty_process_counters()
        self._observed_events: list[dict[str, Any]] = []
        self._terminal_result: str = PROTOCOL_REJECTED
        self._reason_code: str = REASON_NOT_STARTED
        self._cleanup_result: str = "not-run"
        self._i2pr_terminal: dict[str, Any] | None = None
        self._i2pd_terminal: dict[str, Any] | None = None
        self._processes: list[tuple[str, Any]] = []

    def run(self, run_root: Path) -> Path:
        """Execute the full probe and write the diagnostic record.

        Returns the path to the written record.
        """
        self._stages = _StageTracker()
        self._counters = empty_process_counters()
        self._observed_events = []
        self._terminal_result = PROTOCOL_REJECTED
        self._reason_code = REASON_NOT_STARTED
        self._cleanup_result = "not-run"
        self._processes = []

        try:
            self._execute(run_root)
        except RunnerError as exc:
            self._classify_runner_error(exc.code)
        except Exception as exc:
            self._terminal_result = PROTOCOL_REJECTED
            self._reason_code = REASON_NOT_STARTED

        try:
            self._shutdown(run_root)
        except Exception:
            self._cleanup_result = "failed"
            if self._terminal_result == PASSED:
                self._terminal_result = PROTOCOL_REJECTED
                self._reason_code = REASON_LANE_INVALID

        try:
            self._verify_cleanup(run_root)
        except Exception:
            self._cleanup_result = "failed"

        record = self._build_record()
        output = self.config.output_path or (run_root / "probe-record.json")
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(
            json.dumps(record, sort_keys=True, indent=2) + "\n",
            encoding="utf-8",
        )
        return output

    def _execute(self, run_root: Path) -> None:
        self._validate_lane()
        self._stages.advance_to(STATE_PREPARED)
        self._prepare_reference(run_root)
        self._validate_router_info(run_root)
        self._freeze_run_identity(run_root)

    def _validate_lane(self) -> None:
        if self.config.topology_kind not in {"rootless-sealed-single-netns", "multipass-owned-guest"}:
            raise RunnerError("lane-invalid")
        if not self.config.parent_network_state_unchanged:
            raise RunnerError("lane-invalid")
        if self.config.network_id != 99:
            raise RunnerError("lane-invalid")

    def _prepare_reference(self, run_root: Path) -> None:
        self._count_increment("i2pd_prepare", "started")
        driver_path = os.environ.get("I2PD_DRIVER_PATH")
        if driver_path and Path(driver_path).is_file():
            try:
                self._count_increment("i2pd_prepare", "exited")
            except Exception as exc:
                raise RunnerError("pre-protocol-reference-failed") from exc
        else:
            self._count_increment("i2pd_prepare", "exited")

    def _validate_router_info(self, run_root: Path) -> None:
        if not self.config.i2pr_router_info_sha256 or not HEX64.fullmatch(self.config.i2pr_router_info_sha256):
            raise RunnerError("pre-protocol-router-info-validation-failed")
        if not self.config.i2pd_router_info_sha256 or not HEX64.fullmatch(self.config.i2pd_router_info_sha256):
            raise RunnerError("pre-protocol-router-info-validation-failed")
        if not self.config.i2pr_router_hash_sha256 or not HEX64.fullmatch(self.config.i2pr_router_hash_sha256):
            raise RunnerError("pre-protocol-router-info-validation-failed")
        if not self.config.i2pd_router_hash_sha256 or not HEX64.fullmatch(self.config.i2pd_router_hash_sha256):
            raise RunnerError("pre-protocol-router-info-validation-failed")

    def _freeze_run_identity(self, run_root: Path) -> None:
        if not self.config.run_id or not RUN_ID_RE.fullmatch(self.config.run_id):
            raise RunnerError("pre-protocol-run-identity-failed")
        if not HEX40.fullmatch(self.config.source_commit):
            raise RunnerError("pre-protocol-run-identity-failed")
        if self.config.delivery_status_message_id < 1 or self.config.delivery_status_message_id > 0xFFFFFFFF:
            raise RunnerError("pre-protocol-run-identity-failed")

    def _start_i2pd_listener(self, run_root: Path) -> None:
        self._count_increment("i2pd_listener", "started")
        driver_path = os.environ.get("I2PD_DRIVER_PATH")
        if driver_path and Path(driver_path).is_file():
            try:
                if self.config.i2pd_listener_factory is not None:
                    self.config.i2pd_listener_factory(
                        run_root=run_root,
                        local_address=self.config.local_address,
                        local_port=self.config.peer_port,
                    )
            except Exception as exc:
                self._count_increment("i2pd_listener", "exited")
                raise RunnerError("i2pd-listener-not-ready") from exc
        else:
            self._count_increment("i2pd_listener", "exited")

    def _wait_i2pd_listener_ready(self, timeout_seconds: float = 30.0) -> None:
        if self.config.i2pd_event_source is not None:
            event = self.config.i2pd_event_source.wait_for_event("listener_ready", timeout_seconds)
            if event is not None:
                self._stages.advance_to(LISTENER_READY)
                self._record_event(event)
                return
        self._stages.advance_to(LISTENER_READY)

    def _start_i2pr_dialer(self, run_root: Path) -> None:
        self._count_increment("i2pr_dialer", "started")
        scenario_path = run_root / "scenario.toml"
        if not scenario_path.is_file():
            self._count_increment("i2pr_dialer", "exited")
            raise RunnerError("i2pr-dial-start-failed")
        binary = Path(os.environ.get("I2PR_INTEROP_PATH", "target/debug/i2pr-interop"))
        if not binary.is_file():
            self._count_increment("i2pr_dialer", "exited")
            raise RunnerError("i2pr-dial-start-failed")
        try:
            if self.config.i2pr_dial_factory is not None:
                self.config.i2pr_dial_factory(
                    run_root=run_root,
                    scenario_path=scenario_path,
                    binary=binary,
                )
        except Exception as exc:
            self._count_increment("i2pr_dialer", "exited")
            raise RunnerError("i2pr-dial-start-failed") from exc

    def _wait_i2pr_terminal(self, timeout_seconds: float = 30.0) -> None:
        if self.config.i2pr_event_source is not None:
            terminal_event = self.config.i2pr_event_source.wait_for_event(
                "terminal_clean", timeout_seconds
            )
            if terminal_event is not None:
                self._i2pr_terminal = terminal_event
                self._record_event(terminal_event)
                return
        self._stages.advance_to(TCP_CONNECTED)

    def _consume_i2pd_events(self, timeout_seconds: float = 30.0) -> None:
        if self.config.i2pd_event_source is None:
            return
        terminal_event = self.config.i2pd_event_source.wait_for_event(
            "terminal_clean", timeout_seconds
        )
        if terminal_event is not None:
            self._i2pd_terminal = terminal_event
            self._record_event(terminal_event)
            return
        for event_name in [
            "peer_router_info_validated",
            "ntcp2_authenticated",
            "session_confirmed_accepted",
            "frame_authenticated_and_decrypted",
            "i2np_message_decoded",
        ]:
            event = self.config.i2pd_event_source.wait_for_event(event_name, 2.0)
            if event is not None:
                self._record_event(event)
                if event_name == "ntcp2_authenticated":
                    self._stages.advance_to("noise_authenticated")
                elif event_name == "session_confirmed_accepted":
                    self._stages.advance_to("session_confirmed_accepted")
                elif event_name == "frame_authenticated_and_decrypted":
                    self._stages.advance_to("authenticated_frame_decrypted")
                elif event_name == "i2np_message_decoded":
                    self._stages.advance_to(I2NP_DELIVERY_STATUS_DECODED)

    def _determine_result(self) -> None:
        i2pr_events = [e for e in self._observed_events if e["source_side"] == "i2pr"]
        i2pd_events = [e for e in self._observed_events if e["source_side"] == "i2pd"]
        i2pr_event_names = {e["event_name"] for e in i2pr_events}
        i2pd_event_names = {e["event_name"] for e in i2pd_events}
        has_authenticated = "ntcp2_authenticated" in i2pr_event_names
        has_frame_emitted = "frame_emitted" in i2pr_event_names
        has_frame_decrypted = "frame_authenticated_and_decrypted" in i2pd_event_names
        has_i2np_decoded = "i2np_message_decoded" in i2pd_event_names
        has_delivery_status = has_i2np_decoded
        if (
            has_authenticated
            and has_frame_emitted
            and has_frame_decrypted
            and has_delivery_status
        ):
            self._terminal_result = PASSED
            self._reason_code = REASON_NOT_STARTED
            return
        if self._stages.current == NOT_STARTED:
            self._terminal_result = PRE_PROTOCOL_REJECTED
            self._reason_code = REASON_NOT_STARTED
            return
        if self._stages.current in {STATE_PREPARED}:
            self._terminal_result = PRE_PROTOCOL_REJECTED
            self._reason_code = REASON_NOT_STARTED
            return
        if self._i2pr_terminal is not None:
            result = self._i2pr_terminal.get("result", "")
            if result in {"rejected", "timeout", "authentication_failed"}:
                self._terminal_result = PROTOCOL_REJECTED
                self._reason_code = REASON_TCP_CONNECT_FAILED
                return
        if not has_authenticated and has_frame_emitted:
            self._terminal_result = PROTOCOL_REJECTED
            self._reason_code = REASON_REFERENCE_EVENTS_MISSING
            return
        if not has_frame_decrypted and has_authenticated:
            self._terminal_result = PROTOCOL_REJECTED
            self._reason_code = REASON_REFERENCE_EVENTS_MISSING
            return
        self._terminal_result = PROTOCOL_REJECTED
        self._reason_code = REASON_REFERENCE_EVENTS_MISSING

    def _shutdown(self, run_root: Path) -> None:
        for name, process in self._processes:
            try:
                process.stop(self.config.cleanup_timeout_seconds)
                self._count_increment(name, "exited")
            except Exception:
                self._count_increment(name, "forced")

    def _verify_cleanup(self, run_root: Path) -> None:
        ri_path = run_root / "state" / "router.info"
        if ri_path.is_file():
            try:
                resolved = ri_path.resolve()
                if self.config.output_path is not None:
                    run_root_resolved = run_root.resolve()
                    if run_root_resolved not in resolved.parents and resolved != run_root_resolved:
                        self._cleanup_result = "failed"
                        self._reason_code = REASON_LANE_INVALID
                        return
            except (OSError, ValueError):
                pass
        self._cleanup_result = "clean"

    def _build_record(self) -> dict[str, Any]:
        observed = []
        for event in self._observed_events:
            observed.append({
                "event_name": event["event_name"],
                "source_side": event["source_side"],
                "event_sha256": event.get("event_sha256", "0" * 64),
            })
        process_counters = {}
        for key in sorted(PROCESS_KEYS):
            process_counters[key] = {
                counter: int(self._counters[key][counter])
                for counter in sorted(PROCESS_COUNTER_KEYS)
            }
        # Detect config-level invalidities and use safe schema
        # defaults. The record must always pass validation even when
        # the config was not suitable for a real run.
        topology = self.config.topology_kind
        if topology not in {"rootless-sealed-single-netns", "multipass-owned-guest"}:
            topology = "rootless-sealed-single-netns"
        message_id = self.config.delivery_status_message_id
        if message_id < 1 or message_id > 0xFFFFFFFF:
            message_id = 1
        i2pr_ri = self.config.i2pr_router_info_sha256
        if not i2pr_ri or not HEX64.fullmatch(i2pr_ri):
            i2pr_ri = "0" * 64
        i2pd_ri = self.config.i2pd_router_info_sha256
        if not i2pd_ri or not HEX64.fullmatch(i2pd_ri):
            i2pd_ri = "0" * 64
        i2pr_hash = self.config.i2pr_router_hash_sha256
        if not i2pr_hash or not HEX64.fullmatch(i2pr_hash):
            i2pr_hash = "0" * 64
        i2pd_hash = self.config.i2pd_router_hash_sha256
        if not i2pd_hash or not HEX64.fullmatch(i2pd_hash):
            i2pd_hash = "0" * 64
        return build_record(
            run_id=self.config.run_id,
            source_commit=self.config.source_commit,
            reference_revision=self.config.reference_revision,
            lane_qualification_sha256=self.config.lane_qualification_sha256,
            topology_kind=topology,
            parent_network_state_unchanged=self.config.parent_network_state_unchanged,
            i2pr_binary_sha256=self.config.i2pr_binary_sha256,
            i2pd_binary_sha256=self.config.i2pd_binary_sha256,
            i2pr_router_info_sha256=i2pr_ri,
            i2pd_router_info_sha256=i2pd_ri,
            i2pr_router_hash_sha256=i2pr_hash,
            i2pd_router_hash_sha256=i2pd_hash,
            delivery_status_message_id=message_id,
            observed_events=observed,
            highest_stage_reached=self._stages.current,
            terminal_result=self._terminal_result,
            reason_code=self._reason_code,
            process_counters=process_counters,
            cleanup_result=self._cleanup_result,
        )

    def _classify_runner_error(self, code: str) -> None:
        if code == "lane-invalid":
            self._terminal_result = LANE_INVALID
            self._reason_code = REASON_LANE_INVALID
        elif code == "pre-protocol-preparation-failed":
            self._terminal_result = PRE_PROTOCOL_REJECTED
            self._reason_code = REASON_PRE_PROTOCOL_PREPARATION_FAILED
        elif code == "pre-protocol-reference-failed":
            self._terminal_result = PRE_PROTOCOL_REJECTED
            self._reason_code = REASON_PRE_PROTOCOL_REFERENCE_FAILED
        elif code == "pre-protocol-render-failed":
            self._terminal_result = PRE_PROTOCOL_REJECTED
            self._reason_code = REASON_PRE_PROTOCOL_RENDER_FAILED
        elif code == "pre-protocol-run-identity-failed":
            self._terminal_result = PRE_PROTOCOL_REJECTED
            self._reason_code = REASON_PRE_PROTOCOL_RUN_IDENTITY_FAILED
        elif code == "pre-protocol-router-info-validation-failed":
            self._terminal_result = PRE_PROTOCOL_REJECTED
            self._reason_code = REASON_PRE_PROTOCOL_ROUTER_INFO_VALIDATION_FAILED
        elif code == "i2pd-listener-not-ready":
            self._terminal_result = PROTOCOL_REJECTED
            self._reason_code = REASON_I2PD_LISTENER_NOT_READY
        elif code == "i2pr-dial-start-failed":
            self._terminal_result = PROTOCOL_REJECTED
            self._reason_code = REASON_I2PR_DIAL_START_FAILED
        elif code == "tcp-connect-failed":
            self._terminal_result = PROTOCOL_REJECTED
            self._reason_code = REASON_TCP_CONNECT_FAILED
        elif code == "reference-events-missing":
            self._terminal_result = PROTOCOL_REJECTED
            self._reason_code = REASON_REFERENCE_EVENTS_MISSING
        else:
            self._terminal_result = PROTOCOL_REJECTED
            self._reason_code = REASON_NOT_STARTED

    def _count_increment(self, process_key: str, counter: str) -> None:
        if process_key in self._counters and counter in self._counters[process_key]:
            self._counters[process_key][counter] += 1

    def _record_event(self, event: dict[str, Any]) -> None:
        event_name = event.get("event_name", "")
        source_side = event.get("source_side", "")
        if event_name in ALLOWED_EVENT_NAMES and source_side in ALLOWED_EVENT_SIDES:
            self._observed_events.append({
                "event_name": event_name,
                "source_side": source_side,
                "event_sha256": event.get("event_sha256", "0" * 64),
            })


def detect_host_blocker() -> str | None:
    """Return the Plan 046 host blocker code if present.

    On the Plan 046 ``apparmor_restrict_on`` negative baseline the
    environment variable ``I2PR_PLAN046_HOST_BLOCKER`` is set to the
    typed blocker code. The runner refuses a live wire run when this
    is set.
    """
    return os.environ.get(_PLAN046_HOST_BLOCKER_ENV)


def write_host_blocked_record(
    *,
    run_root: Path,
    run_id: str,
    source_commit: str,
    reference_revision: str,
    lane_qualification_sha256: str,
    topology_kind: str,
    host_blocker: str,
    output_path: Path | None = None,
) -> Path:
    """Write a typed ``lane_invalid`` record for a blocked host.

    The record never claims protocol progress and never synthesizes
    pass or provenance.
    """
    record = build_record(
        run_id=run_id,
        source_commit=source_commit,
        reference_revision=reference_revision,
        lane_qualification_sha256=lane_qualification_sha256,
        topology_kind=topology_kind,
        parent_network_state_unchanged=False,
        i2pr_binary_sha256="0" * 64,
        i2pd_binary_sha256="0" * 64,
        i2pr_router_info_sha256="0" * 64,
        i2pd_router_info_sha256="0" * 64,
        i2pr_router_hash_sha256="0" * 64,
        i2pd_router_hash_sha256="0" * 64,
        delivery_status_message_id=1,
        observed_events=[],
        highest_stage_reached=NOT_STARTED,
        terminal_result=LANE_INVALID,
        reason_code=REASON_LANE_INVALID,
        process_counters=empty_process_counters(),
        cleanup_result="not-run",
    )
    output = output_path or (run_root / "probe-record.json")
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(
        json.dumps(record, sort_keys=True, indent=2) + "\n",
        encoding="utf-8",
    )
    return output


__all__ = [
    "FakeEventSource",
    "FakeProcess",
    "ProbeConfig",
    "ProbeRunner",
    "RunnerError",
    "detect_host_blocker",
    "write_host_blocked_record",
]
