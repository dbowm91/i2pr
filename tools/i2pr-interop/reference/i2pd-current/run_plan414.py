#!/usr/bin/env python3
"""Run at most one forward and one reverse Plan 414 loopback attempt."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import secrets
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import uuid

PIN = "635b013a612ff47278ef02acf8580a28e10e26c5"
TREE_SHA256 = "dbffcb2960766cf07cc87a5a56377472122ae577832f2ff5dfaa51611eb82f98"
TOPOLOGY = "current-network-loopback"
TIMEOUT_MS = 30_000
MAX_ATTEMPTS_PER_DIRECTION = 1


class RunError(RuntimeError):
    def __init__(self, reason: str):
        super().__init__(reason)
        self.reason = reason


def directions_for(selection: str) -> tuple[str, ...]:
    if selection == "forward":
        return ("forward",)
    if selection == "reverse":
        return ("reverse",)
    if selection == "both":
        return ("forward", "reverse")
    raise RunError("direction-selection-invalid")


def scenario_identity_values(direction: str, i2pr_hash: str,
                             i2pd_hash: str) -> tuple[str, str]:
    if direction not in {"forward", "reverse"}:
        raise RunError("scenario-direction-invalid")
    if (len(i2pr_hash) != 64 or len(i2pd_hash) != 64
            or i2pr_hash == i2pd_hash):
        raise RunError("scenario-router-identities-invalid")
    # The launcher is always the I2NP sender for the correlated response,
    # even when stock i2pd initiates the NTCP2 connection.
    return i2pr_hash, i2pd_hash


def validate_scenario_identity_values(direction: str, sender_hash: str,
                                      receiver_hash: str, i2pr_hash: str,
                                      i2pd_hash: str) -> None:
    expected = scenario_identity_values(direction, i2pr_hash, i2pd_hash)
    if (sender_hash, receiver_hash) != expected:
        raise RunError("scenario-router-identity-role-mismatch")


def safe_reason(value: object) -> str | None:
    if not isinstance(value, str) or not value or len(value) > 80:
        return None
    if all(ch.islower() or ch.isdigit() or ch in "-_" for ch in value):
        return value
    return None


def project_stages(path: Path, reference: bool) -> list[dict[str, str]]:
    try:
        records = json_lines(path)
    except RunError:
        return []
    projected = []
    for record in records[:128]:
        if not isinstance(record, dict):
            continue
        if reference:
            kind = safe_reason(record.get("event_kind"))
            reason = safe_reason(record.get("reason_code"))
            if kind is not None:
                projected.append({"event_kind": kind, **({"reason_code": reason} if reason else {})})
        else:
            phase = safe_reason(record.get("phase"))
            result = safe_reason(record.get("result"))
            reason = safe_reason(record.get("reason_code"))
            if phase is not None:
                row = {"phase": phase}
                if result:
                    row["result"] = result
                if reason:
                    row["reason_code"] = reason
                projected.append(row)
    return projected


def write_evidence(path: Path, attempts: list[dict[str, object]], result: str) -> None:
    record = {
        "schema": "i2pr-plan417-evidence-v1",
        "reference_revision": PIN,
        "max_attempts_per_direction": MAX_ATTEMPTS_PER_DIRECTION,
        "directions": [sanitize_attempt(attempt) for attempt in attempts],
        "result": result,
    }
    temporary = path.with_name(path.name + ".tmp")
    descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
        handle.write(json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n")
        handle.flush()
        os.fsync(handle.fileno())
    temporary.replace(path)


def sanitize_attempt(attempt: dict[str, object]) -> dict[str, object]:
    allowed = {
        "direction", "attempt_number", "attempted", "result", "reason_code",
        "cleanup_result", "helper_return_code", "launcher_return_code",
        "observer_result", "observer_reason_code", "decoded_delivery_status_count",
        "decrypted_frame_count", "i2np_block_count", "session_request_received_count",
        "session_created_received_count", "session_confirmed_received_count",
        "session_confirmed_sent_count", "session_request_aead_failure_count",
        "session_created_aead_failure_count", "session_confirmed_aead_failure_count",
        "session_confirmed_part2_kdf_failure_count", "session_confirmed_unexpected_block_count",
        "session_confirmed_unexpected_router_info_size_count",
        "session_confirmed_router_info_verification_failure_count",
        "session_confirmed_router_info_too_old_count", "session_confirmed_router_info_from_future_count", "session_confirmed_router_version_too_old_count", "session_confirmed_router_info_accepted_count", "ntcp2_session_terminated_count",
        "session_confirmed_router_info_update_failure_count", "session_confirmed_address_not_found_count",
        "session_confirmed_host_mismatch_count", "session_confirmed_wrong_static_key_count",
        "reference_stages", "launcher_stages",
    }
    result: dict[str, object] = {}
    for key in allowed:
        value = attempt.get(key)
        if key in {"reference_stages", "launcher_stages"}:
            if isinstance(value, list):
                result[key] = [
                    {field: safe_reason(row.get(field)) for field in ("event_kind", "phase", "result", "reason_code")
                     if safe_reason(row.get(field)) is not None}
                    for row in value[:128] if isinstance(row, dict)
                ]
        elif key in {"helper_return_code", "launcher_return_code"}:
            if isinstance(value, int) and -255 <= value <= 255:
                result[key] = value
        elif key in {
            "decoded_delivery_status_count", "decrypted_frame_count", "i2np_block_count",
            "session_request_received_count", "session_created_received_count",
            "session_confirmed_received_count", "session_confirmed_sent_count",
            "session_request_aead_failure_count", "session_created_aead_failure_count",
            "session_confirmed_aead_failure_count",
            "session_confirmed_part2_kdf_failure_count", "session_confirmed_unexpected_block_count",
            "session_confirmed_unexpected_router_info_size_count",
            "session_confirmed_router_info_verification_failure_count",
            "session_confirmed_router_info_too_old_count", "session_confirmed_router_info_from_future_count", "session_confirmed_router_version_too_old_count", "session_confirmed_router_info_accepted_count", "ntcp2_session_terminated_count",
            "session_confirmed_router_info_update_failure_count", "session_confirmed_address_not_found_count",
            "session_confirmed_host_mismatch_count", "session_confirmed_wrong_static_key_count",
            "attempt_number",
        }:
            if isinstance(value, int) and 0 <= value <= 0xFFFF:
                result[key] = value
        elif key in {"attempted"}:
            if isinstance(value, bool):
                result[key] = value
        elif key in {"direction"}:
            if value in {"forward", "reverse"}:
                result[key] = value
        else:
            safe = safe_reason(value)
            if safe is not None:
                result[key] = safe
    return result


def persist_and_cleanup(evidence: Path, attempts: list[dict[str, object]], root: Path,
                        record: dict[str, object], remove_tree=shutil.rmtree) -> None:
    try:
        write_evidence(evidence, attempts, "in-progress" if record.get("result") == "passed" else "rejected")
    except OSError:
        record["result"] = "rejected"
        record["reason_code"] = "evidence-write-failed"
    try:
        remove_tree(root)
        record["cleanup_result"] = "passed"
    except OSError:
        record["cleanup_result"] = "failed"
        record["result"] = "rejected"
        record["reason_code"] = "cleanup-failed"
    try:
        write_evidence(evidence, attempts, "in-progress" if record.get("result") == "passed" else "rejected")
    except OSError:
        record["result"] = "rejected"
        record["reason_code"] = "evidence-write-failed"


def classify_attempt(record: dict[str, object], helper_rc: int | None, launcher_rc: int | None,
                     observer_record: dict[str, object] | None,
                     helper_ok: bool, launcher_ok: bool) -> None:
    if record.get("attempted") is not True:
        return
    if helper_rc != 0:
        record["reason_code"] = "reference-process-rejected"
    elif launcher_rc != 0:
        record["reason_code"] = "launcher-process-rejected"
    elif observer_record is None or observer_record.get("result") != "observed":
        record["reason_code"] = "log-observation-rejected"
    elif not helper_ok:
        record["reason_code"] = "reference-stage-correlation-failed"
    elif not launcher_ok:
        record["reason_code"] = "launcher-stage-correlation-failed"
    else:
        record["result"] = "passed"
        record["reason_code"] = "observed-one-delivery-status"


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def free_loopback_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as handle:
        handle.bind(("127.0.0.1", 0))
        return int(handle.getsockname()[1])


def json_lines(path: Path) -> list[dict[str, object]]:
    try:
        return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines()]
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise RunError("event-stream-invalid") from exc


def wait_for_event(path: Path, key: str, value: str, process: subprocess.Popen[str], timeout: float) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RunError("process-exited-before-ready")
        try:
            for record in json_lines(path):
                if record.get(key) == value:
                    return
        except RunError:
            pass
        time.sleep(0.05)
    raise RunError("readiness-timeout")


def stop(process: subprocess.Popen[str] | None) -> tuple[int | None, str, str]:
    if process is None:
        return (None, "", "")
    try:
        stdout, stderr = process.communicate(timeout=TIMEOUT_MS / 1000 + 5)
    except subprocess.TimeoutExpired:
        process.terminate()
        try:
            stdout, stderr = process.communicate(timeout=2)
        except subprocess.TimeoutExpired:
            process.kill()
            stdout, stderr = process.communicate(timeout=2)
        return (process.returncode, stdout, stderr)
    return (process.returncode, stdout, stderr)


def run_checked(args: list[str], timeout: float = 30) -> str:
    try:
        result = subprocess.run(args, capture_output=True, text=True, timeout=timeout, check=False)
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise RunError("command-failed") from exc
    if result.returncode != 0:
        operation = args[1] if len(args) > 1 and args[1] in {
            "ntcp2", "--config"
        } else "command"
        raise RunError(f"{operation}-rejected")
    return result.stdout


def parse_single_json(stdout: str) -> dict[str, object]:
    try:
        record = json.loads(stdout)
    except json.JSONDecodeError as exc:
        raise RunError("command-output-invalid") from exc
    if not isinstance(record, dict) or record.get("result") not in {"prepared", "validated"}:
        raise RunError("command-result-invalid")
    return record


def write_driver_config(path: Path, values: dict[str, object]) -> None:
    path.write_text(json.dumps(values, sort_keys=True) + "\n", encoding="utf-8")


def driver_values(
    root: Path,
    driver: Path,
    manifest: Path,
    direction: str,
    mode: str,
    local_port: int,
    peer_port: int,
    peer_info: Path,
    peer_hash: str,
    message_id: int,
    run_id: str,
) -> dict[str, object]:
    manifest_raw = manifest.read_bytes()
    entries = {}
    for line in manifest_raw.decode("utf-8").splitlines():
        key, sep, value = line.partition("=")
        if sep:
            entries[key] = value
    source = Path(__file__).resolve().parent / "src" / "i2pd_current_ntcp2_driver.cpp"
    if entries.get("reference_revision") != PIN or entries.get("reference_tree_sha256") != TREE_SHA256:
        raise RunError("reference-pin-mismatch")
    return {
        "schema": "i2pr-i2pd-current-pin-driver-config-v1",
        "schema_version": 1,
        "run_id": run_id,
        "scenario_id": direction,
        "invocation_id": uuid.uuid4().hex,
        "direction": direction,
        "mode": mode,
        "data_dir": str(root / "i2pd-data"),
        "output_dir": str(root / ("i2pd-" + mode)),
        "local_address": "127.0.0.1",
        "local_port": local_port,
        "network_id": 2,
        "peer_router_info_path": str(peer_info),
        "expected_peer_router_hash_sha256": peer_hash,
        "expected_peer_address": "127.0.0.1",
        "expected_peer_port": peer_port,
        "delivery_status_message_id": message_id,
        "startup_timeout_ms": TIMEOUT_MS,
        "handshake_timeout_ms": TIMEOUT_MS,
        "data_phase_timeout_ms": TIMEOUT_MS,
        "shutdown_timeout_ms": 10_000,
        "reference_revision": PIN,
        "reference_tree_sha256": entries["reference_tree_sha256"],
        "driver_source_sha256": sha256(source.read_bytes()),
        "driver_binary_sha256": sha256(driver.read_bytes()),
        "build_manifest_sha256": sha256(manifest_raw),
        "run_identity_sha256": sha256(run_id.encode()),
        "topology_kind": TOPOLOGY,
    }


def launcher_router_hash(launcher: Path, router_info: Path) -> str:
    raw = run_checked([str(launcher), "ntcp2", "router-hash", "--router-info", str(router_info)])
    value = parse_single_json(raw).get("router_hash_sha256")
    if not isinstance(value, str) or len(value) != 64 or any(c not in "0123456789abcdef" for c in value):
        raise RunError("router-hash-invalid")
    return value


def prepare_i2pr(launcher: Path, root: Path, port: int) -> str:
    state = root / "i2pr-state"
    raw = run_checked([
        str(launcher), "ntcp2", "prepare", "--state-dir", str(state),
        "--local-address", "127.0.0.1", "--local-port", str(port),
        "--network-id", "2", "--topology-kind", TOPOLOGY,
    ])
    record = parse_single_json(raw)
    value = record.get("router_hash_sha256")
    if not isinstance(value, str) or len(value) != 64:
        raise RunError("i2pr-router-hash-invalid")
    return value


def scenario_text(
    scenario_id: str,
    run_id: str,
    role: str,
    local_port: int,
    peer_port: int,
    local_hash: str,
    peer_hash: str,
    message_id: int,
    peer_info: bool,
) -> str:
    fields = [
        '[scenario]',
        'schema = "i2pr-launcher-scenario-v2"',
        'schema_version = 2',
        f'scenario_id = "{scenario_id}"',
        f'run_id = "{run_id}"',
        f'role = "{role}"',
        'address_family = "ipv4"',
        'local_address = "127.0.0.1"',
        f'local_port = {local_port}',
        'network_id = 2',
        'state_dir = "i2pr-state"',
        'handshake_deadline_ms = 30000',
        'read_deadline_ms = 30000',
        'write_deadline_ms = 30000',
        'queue_deadline_ms = 30000',
        'drain_deadline_ms = 30000',
        'padding_profile = "representative"',
        'smoke_message_profile = "delivery-status"',
        'expected_result_class = "authenticated-handshake-and-bounded-i2np-exchange"',
        'status_path = "launcher-status.jsonl"',
        'data_phase_mode = "round-trip-delivery-status"',
        'data_phase_required_peer_action = "observe-receive"',
        'data_phase_timeout_ms = 30000',
        'expected_observation = "i2pr-sent-and-acknowledged"',
        f'delivery_status_message_id = {message_id}',
        f'expected_sender_router_hash_sha256 = "{local_hash}"',
        f'expected_receiver_router_hash_sha256 = "{peer_hash}"',
        'reference_driver_mode = "i2pd-direct-driver"',
        f'run_identity_sha256 = "{sha256(run_id.encode())}"',
        f'topology_kind = "{TOPOLOGY}"',
    ]
    if role == "initiator":
        fields.extend([
            'peer_address = "127.0.0.1"',
            f'peer_port = {peer_port}',
            'peer_router_info = "exchange/i2pd.info"',
        ])
    return "\n".join(fields) + "\n"


def status_passed(path: Path, message_id: int, expected_peer_hash: str) -> bool:
    records = json_lines(path)
    terminal = [record for record in records if record.get("phase") == "terminal"]
    return (
        len(terminal) == 1
        and terminal[0].get("result") == "passed"
        and terminal[0].get("counters", {}).get("delivery_status_message_id") == message_id
        and terminal[0].get("counters", {}).get("expected_peer_router_hash_sha256") == expected_peer_hash
    )


def helper_passed(path: Path, message_id: int) -> bool:
    records = json_lines(path)
    kinds = [record.get("event_kind") for record in records]
    matching = [
        record for record in records
        if record.get("event_kind") == "i2np_message_decoded"
        and record.get("i2np_type") == 10
        and record.get("delivery_status_message_id") == message_id
    ]
    return all(kind in kinds for kind in (
        "listener_ready", "ntcp2_authenticated", "frame_emitted",
        "i2np_message_decoded", "terminal_clean",
    )) and len(matching) == 1


def run_direction(
    launcher: Path, driver: Path, manifest: Path, observer: Path,
    evidence: Path, work_parent: Path, attempts: list[dict[str, object]], direction: str,
) -> dict[str, object]:
    root = Path(tempfile.mkdtemp(prefix="plan417-", dir=work_parent))
    helper: subprocess.Popen[str] | None = None
    launcher_process: subprocess.Popen[str] | None = None
    helper_rc: int | None = None
    launcher_rc: int | None = None
    message_id = 0
    peer_hash = ""
    log_baseline = 0
    log_path = root / "i2pd-data" / "i2pd.log"
    helper_events = root / ("i2pd-listen" if direction == "forward" else "i2pd-dial") / "events.ndjson"
    status_path = root / "launcher-status.jsonl"
    record: dict[str, object] = {
        "direction": direction,
        "attempt_number": 1,
        "attempted": False,
        "result": "rejected",
        "reason_code": "preflight-failed",
        "cleanup_result": "pending",
    }
    observer_record: dict[str, object] | None = None
    try:
        if direction not in {"forward", "reverse"} or MAX_ATTEMPTS_PER_DIRECTION != 1:
            raise RunError("direction-or-attempt-budget-invalid")
        ref_port = free_loopback_port()
        i2pr_port = free_loopback_port()
        if i2pr_port == ref_port:
            raise RunError("loopback-port-collision")
        run_id = uuid.uuid4().hex
        message_id = secrets.randbelow(0xFFFFFFFF) + 1
        i2pr_hash = prepare_i2pr(launcher, root, i2pr_port)
        i2pr_info = root / "i2pr-state" / "router.info"
        inspect_values = driver_values(
            root, driver, manifest,
            "i2pr-to-i2pd-ipv4" if direction == "forward" else "i2pd-to-i2pr-ipv4",
            "inspect", ref_port, i2pr_port, i2pr_info, i2pr_hash, message_id, run_id,
        )
        inspect_config = root / "inspect-config.json"
        write_driver_config(inspect_config, inspect_values)
        run_checked([str(driver), "--config", str(inspect_config)], timeout=30)
        i2pd_info = root / "i2pd-inspect" / "router.info"
        i2pd_hash = launcher_router_hash(launcher, i2pd_info)
        exchange = root / "exchange"
        exchange.mkdir()
        shutil.copyfile(i2pd_info, exchange / "i2pd.info")
        scenario_id = "i2pr-to-i2pd-ipv4" if direction == "forward" else "i2pd-to-i2pr-ipv4"
        role = "initiator" if direction == "forward" else "responder"
        local_hash, peer_hash = scenario_identity_values(
            direction, i2pr_hash, i2pd_hash
        )
        scenario = root / "scenario.toml"
        scenario.write_text(
            scenario_text(scenario_id, run_id, role, i2pr_port, ref_port,
                          local_hash, peer_hash, message_id, role == "initiator"),
            encoding="utf-8",
        )
        checked = run_checked([str(launcher), "ntcp2", "validate-scenario", "--scenario-config", str(scenario)])
        if "validated" not in checked:
            raise RunError("scenario-rejected")
        mode = "listen" if direction == "forward" else "dial"
        active = driver_values(
            root, driver, manifest,
            "i2pr-to-i2pd-ipv4" if direction == "forward" else "i2pd-to-i2pr-ipv4",
            mode, ref_port, i2pr_port, i2pr_info, i2pr_hash, message_id, run_id,
        )
        active_config = root / "active-config.json"
        write_driver_config(active_config, active)
        if direction == "forward":
            helper = subprocess.Popen([str(driver), "--config", str(active_config)],
                                      stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            wait_for_event(helper_events, "event_kind", "listener_ready", helper, 15)
            log_baseline = log_path.stat().st_size
            launcher_process = subprocess.Popen(
                [str(launcher), "ntcp2", "dial", "--scenario-config", str(scenario)],
                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
            )
        else:
            launcher_process = subprocess.Popen(
                [str(launcher), "ntcp2", "listen", "--scenario-config", str(scenario)],
                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
            )
            wait_for_event(status_path, "phase", "listener_ready", launcher_process, 15)
            log_baseline = log_path.stat().st_size if log_path.exists() else 0
            helper = subprocess.Popen([str(driver), "--config", str(active_config)],
                                      stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        record["attempted"] = True
        helper_rc, _, _ = stop(helper)
        helper = None
        launcher_rc, _, _ = stop(launcher_process)
        launcher_process = None
        record["helper_return_code"] = helper_rc
        record["launcher_return_code"] = launcher_rc
    except RunError as exc:
        record["reason_code"] = safe_reason(exc.reason) or "runner-failed"
    except (OSError, subprocess.SubprocessError, json.JSONDecodeError):
        record["reason_code"] = "runner-failed"
    finally:
        if helper is not None:
            helper_rc, _, _ = stop(helper)
            record["helper_return_code"] = helper_rc
        if launcher_process is not None:
            launcher_rc, _, _ = stop(launcher_process)
            record["launcher_return_code"] = launcher_rc
        record["reference_stages"] = project_stages(helper_events, True)
        record["launcher_stages"] = project_stages(status_path, False)
        if log_path.exists():
            summary = root / "sanitized-observation.json"
            try:
                observed = subprocess.run(
                    [sys.executable, str(observer), "--consume-log", str(log_path),
                     "--owned-root", str(log_path.parent), "--output", str(summary),
                     "--direction", "i2pr-to-i2pd-ipv4" if direction == "forward" else "i2pd-to-i2pr-ipv4",
                     "--baseline-offset", str(log_baseline)],
                    capture_output=True, text=True, timeout=10, check=False,
                )
                if observed.returncode == 0:
                    try:
                        observer_record = json.loads(summary.read_text(encoding="utf-8"))
                    except (OSError, UnicodeError, json.JSONDecodeError):
                        observer_record = None
                else:
                    try:
                        observer_record = json.loads(observed.stdout)
                    except json.JSONDecodeError:
                        observer_record = None
            except (OSError, subprocess.SubprocessError):
                observer_record = None
        if observer_record is not None:
            record["observer_result"] = safe_reason(observer_record.get("result")) or "rejected"
            observer_reason = safe_reason(observer_record.get("reason_code"))
            if observer_reason is not None:
                record["observer_reason_code"] = observer_reason
            for field in (
                "decoded_delivery_status_count", "decrypted_frame_count", "i2np_block_count",
                "session_request_received_count", "session_created_received_count",
                "session_confirmed_received_count", "session_confirmed_sent_count",
                "session_request_aead_failure_count", "session_created_aead_failure_count",
                "session_confirmed_aead_failure_count",
                "session_confirmed_part2_kdf_failure_count", "session_confirmed_unexpected_block_count",
                "session_confirmed_unexpected_router_info_size_count",
                "session_confirmed_router_info_verification_failure_count",
                "session_confirmed_router_info_too_old_count", "session_confirmed_router_info_from_future_count", "session_confirmed_router_version_too_old_count", "session_confirmed_router_info_accepted_count", "ntcp2_session_terminated_count",
                "session_confirmed_router_info_update_failure_count", "session_confirmed_address_not_found_count",
                "session_confirmed_host_mismatch_count", "session_confirmed_wrong_static_key_count",
            ):
                value = observer_record.get(field)
                if isinstance(value, int) and 0 <= value <= 0xFFFF:
                    record[field] = value
        try:
            helper_ok = False
            launcher_ok = False
            if (helper_rc == 0 and launcher_rc == 0 and observer_record is not None
                    and observer_record.get("result") == "observed"):
                helper_ok = helper_passed(helper_events, message_id)
                launcher_ok = status_passed(status_path, message_id, peer_hash)
            classify_attempt(record, helper_rc, launcher_rc, observer_record, helper_ok, launcher_ok)
        except RunError:
            record["reason_code"] = "stage-record-invalid"
        attempts.append(record)
        persist_and_cleanup(evidence, attempts, root, record)
    return record

def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--launcher", type=Path)
    parser.add_argument("--driver", type=Path)
    parser.add_argument("--build-manifest", type=Path)
    parser.add_argument("--observer", type=Path)
    parser.add_argument("--evidence", type=Path)
    parser.add_argument("--work-parent", type=Path)
    parser.add_argument("--direction", choices=("forward", "reverse", "both"), default="both")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        with tempfile.TemporaryDirectory(prefix="plan417-self-test-") as temporary:
            root = Path(temporary)
            if (directions_for("forward") != ("forward",)
                    or directions_for("reverse") != ("reverse",)
                    or directions_for("both") != ("forward", "reverse")):
                raise RunError("self-test-direction-selection-failed")
            i2pr_identity = "1" * 64
            i2pd_identity = "2" * 64
            for direction in ("forward", "reverse"):
                sender, receiver = scenario_identity_values(
                    direction, i2pr_identity, i2pd_identity
                )
                validate_scenario_identity_values(
                    direction, sender, receiver, i2pr_identity, i2pd_identity
                )
                for invalid_pair in (
                    (i2pd_identity, i2pr_identity),
                    (i2pr_identity, i2pr_identity),
                    (i2pd_identity, i2pd_identity),
                ):
                    try:
                        validate_scenario_identity_values(
                            direction, *invalid_pair, i2pr_identity, i2pd_identity
                        )
                    except RunError as exc:
                        if exc.reason != "scenario-router-identity-role-mismatch":
                            raise
                    else:
                        raise RunError("self-test-swapped-or-duplicate-identities-accepted")
            try:
                directions_for("unknown")
            except RunError as exc:
                if exc.reason != "direction-selection-invalid":
                    raise
            else:
                raise RunError("self-test-invalid-direction-accepted")
            staged = root / "events.jsonl"
            staged.write_text(json.dumps({
                "phase": "terminal", "result": "failed", "reason_code": "wire-rejected",
                "delivery_status_message_id": 778899, "expected_peer_router_hash_sha256": "a" * 64,
                "raw_stdout": "PRIVATE_SENTINEL", "path": "/private/path",
            }) + "\n", encoding="utf-8")
            projected = project_stages(staged, False)
            if projected != [{"phase": "terminal", "result": "failed", "reason_code": "wire-rejected"}]:
                raise RunError("self-test-stage-projection-failed")
            evidence = root / "evidence.json"
            write_evidence(evidence, [{"direction": "forward", "launcher_stages": projected}], "rejected")
            output = evidence.read_text(encoding="utf-8")
            if any(value in output for value in ("PRIVATE_SENTINEL", "/private/path", "778899", "a" * 64)):
                raise RunError("self-test-evidence-redaction-failed")
            malformed = root / "malformed.jsonl"
            malformed.write_text("not json\n", encoding="utf-8")
            if project_stages(malformed, False) != []:
                raise RunError("self-test-malformed-stage-failed")
            private = {
                "direction": "forward", "attempt_number": 1, "attempted": True,
                "result": "rejected", "reason_code": "readiness-timeout",
                "cleanup_result": "pending", "helper_return_code": 7,
                "launcher_return_code": -15, "observer_result": "rejected",
                "observer_reason_code": "log-ambiguous", "decoded_delivery_status_count": 2,
                "reference_stages": [{"event_kind": "listener_ready", "message_id": 778899,
                                       "router_hash": "a" * 64, "raw_stderr": "PRIVATE_SENTINEL"}],
                "session_request_received_count": 1,
                "session_confirmed_received_count": 1,
                "session_confirmed_unexpected_block_count": 1,
                "session_confirmed_wrong_static_key_count": 1,
                "session_confirmed_router_info_accepted_count": 1,
                "ntcp2_session_terminated_count": 1,
                "log_sha256": "b" * 64,
                "private_path": "/private/path", "raw_stdout": "PRIVATE_SENTINEL",
            }
            sanitized = sanitize_attempt(private)
            if any(value in json.dumps(sanitized) for value in
                   ("778899", "a" * 64, "PRIVATE_SENTINEL", "/private/path")):
                raise RunError("self-test-attempt-redaction-failed")
            stage_evidence = root / "stage-evidence.json"
            write_evidence(stage_evidence, [private], "rejected")
            stage_json = stage_evidence.read_text(encoding="utf-8")
            if ('"session_request_received_count":1' not in stage_json
                    or '"session_confirmed_unexpected_block_count":1' not in stage_json
                    or '"session_confirmed_wrong_static_key_count":1' not in stage_json
                    or '"session_confirmed_router_info_accepted_count":1' not in stage_json
                    or '"ntcp2_session_terminated_count":1' not in stage_json or any(
                    value in stage_json for value in ("log_sha256", "b" * 64, "778899"))):
                raise RunError("self-test-stage-count-evidence-failed")
            cleanup_root = root / "owned"
            cleanup_root.mkdir()
            (cleanup_root / "private.log").write_text("PRIVATE_SENTINEL", encoding="utf-8")
            persist_and_cleanup(root / "missing-parent" / "evidence.json", [private], cleanup_root, private)
            if cleanup_root.exists() or private.get("cleanup_result") != "passed":
                raise RunError("self-test-cleanup-after-write-failure-failed")
            if private.get("reason_code") != "evidence-write-failed":
                raise RunError("self-test-write-failure-outcome-failed")
            cases = [
                ({"attempted": False, "reason_code": "readiness-timeout"}, None, None, None,
                 "readiness-timeout"),
                ({"attempted": True, "result": "rejected"}, 3, 0, None,
                 "reference-process-rejected"),
                ({"attempted": True, "result": "rejected"}, 0, 4, None,
                 "launcher-process-rejected"),
                ({"attempted": True, "result": "rejected"}, 0, 0, None,
                 "log-observation-rejected"),
                ({"attempted": True, "result": "rejected"}, 0, 0, {"result": "observed"},
                 "reference-stage-correlation-failed"),
            ]
            for sample, helper_code, launcher_code, observer, expected in cases:
                classify_attempt(sample, helper_code, launcher_code, observer, False, False)
                if sample.get("reason_code") != expected:
                    raise RunError("self-test-failure-classification-failed")
            cleanup_failure_root = root / "cleanup-failure"
            cleanup_failure_root.mkdir()
            failed_cleanup = {"direction": "reverse", "attempted": True, "result": "rejected",
                              "reason_code": "log-observation-rejected"}
            def reject_cleanup(_path: Path) -> None:
                raise OSError("PRIVATE_SENTINEL")
            persist_and_cleanup(evidence, [failed_cleanup], cleanup_failure_root, failed_cleanup, reject_cleanup)
            if failed_cleanup.get("reason_code") != "cleanup-failed" or failed_cleanup.get("cleanup_result") != "failed":
                raise RunError("self-test-cleanup-failure-outcome-failed")
            shutil.rmtree(cleanup_failure_root)
            if MAX_ATTEMPTS_PER_DIRECTION != 1 or PIN != "635b013a612ff47278ef02acf8580a28e10e26c5":
                raise RunError("self-test-contract-failed")
        print("plan417 runner self-test passed")
        return 0
    evidence: Path | None = None
    attempts: list[dict[str, object]] = []
    try:
        required = (args.launcher, args.driver, args.build_manifest, args.observer, args.evidence, args.work_parent)
        if any(value is None for value in required):
            raise RunError("arguments-required")
        launcher = args.launcher.resolve(strict=True)
        driver = args.driver.resolve(strict=True)
        manifest = args.build_manifest.resolve(strict=True)
        observer = args.observer.resolve(strict=True)
        parent = args.work_parent.resolve(strict=True)
        evidence = args.evidence.resolve(strict=False)
        if evidence.is_relative_to(parent):
            raise RunError("evidence-must-be-outside-work-parent")
        evidence.parent.mkdir(parents=True, exist_ok=True)
        if evidence.exists():
            raise RunError("evidence-already-exists")
        selected = directions_for(args.direction)
        terminal: dict[str, object] | None = None
        for direction in selected:
            terminal = run_direction(
                launcher, driver, manifest, observer, evidence, parent, attempts, direction
            )
            if terminal.get("result") != "passed":
                break
        result = "passed" if terminal is not None and terminal.get("result") == "passed" else "rejected"
        write_evidence(evidence, attempts, result)
        print(json.dumps({"schema": "i2pr-plan417-evidence-v1", "result": result,
                          **({} if result == "passed" else {
                              "reason_code": safe_reason((terminal or {}).get("reason_code"))
                              or "attempt-rejected"})},
                         separators=(",", ":")))
        return 0 if result == "passed" else 2
    except (OSError, RunError, subprocess.SubprocessError, json.JSONDecodeError) as exc:
        reason = exc.reason if isinstance(exc, RunError) else "runner-failed"
        if evidence is not None and not evidence.exists():
            try:
                write_evidence(evidence, attempts, "rejected")
            except OSError:
                pass
        print(json.dumps({"schema": "i2pr-plan417-evidence-v1", "result": "rejected", "reason_code": reason}, separators=(",", ":")))
        return 2


if __name__ == "__main__":
    sys.exit(main())

