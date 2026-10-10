#!/usr/bin/env python3
"""Run at most one forward and one reverse Plan 414 loopback attempt."""

from __future__ import annotations

import argparse
import hashlib
import json
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
    evidence_dir: Path, direction: str,
) -> dict[str, object]:
    if direction not in {"forward", "reverse"} or MAX_ATTEMPTS_PER_DIRECTION != 1:
        raise RunError("direction-or-attempt-budget-invalid")
    root = Path(tempfile.mkdtemp(prefix="plan414-", dir=evidence_dir))
    helper: subprocess.Popen[str] | None = None
    launcher_process: subprocess.Popen[str] | None = None
    try:
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
        local_hash, peer_hash = (i2pr_hash, i2pd_hash) if direction == "forward" else (i2pr_hash, i2pd_hash)
        if role == "responder":
            local_hash, peer_hash = i2pd_hash, i2pr_hash
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
        helper_events = root / ("i2pd-" + mode) / "events.ndjson"
        status_path = root / "launcher-status.jsonl"
        if direction == "forward":
            helper = subprocess.Popen(
                [str(driver), "--config", str(active_config)],
                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
            )
            wait_for_event(helper_events, "event_kind", "listener_ready", helper, 15)
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
            helper = subprocess.Popen(
                [str(driver), "--config", str(active_config)],
                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
            )

        helper_rc, _, _ = stop(helper)
        helper = None
        launcher_rc, _, _ = stop(launcher_process)
        launcher_process = None
        if helper_rc != 0 or launcher_rc != 0:
            raise RunError("wire-process-rejected")
        if not helper_passed(helper_events, message_id):
            raise RunError("i2pd-stage-correlation-failed")
        if not status_passed(status_path, message_id, peer_hash):
            raise RunError("i2pr-stage-correlation-failed")

        summary = root / "sanitized-observation.json"
        observed = subprocess.run(
            [sys.executable, str(observer), "--consume-log",
             str(root / "i2pd-data" / "i2pd.log"), "--owned-root",
             str(root / "i2pd-data"), "--output", str(summary),
             "--direction", "i2pr-to-i2pd-ipv4" if direction == "forward" else "i2pd-to-i2pr-ipv4"],
            capture_output=True, text=True, timeout=10, check=False,
        )
        if observed.returncode != 0:
            raise RunError("log-observation-rejected")
        log_record = json.loads(summary.read_text(encoding="utf-8"))
        return {
            "direction": direction,
            "result": "passed",
            "reference_revision": PIN,
            "decoded_delivery_status_count": log_record["decoded_delivery_status_count"],
            "decrypted_frame_count": log_record["decrypted_frame_count"],
            "i2np_block_count": log_record["i2np_block_count"],
            "log_sha256": log_record["log_sha256"],
            "launcher_status_sha256": sha256(status_path.read_bytes()),
            "reference_events_sha256": sha256(helper_events.read_bytes()),
        }
    finally:
        stop(helper)
        stop(launcher_process)
        shutil.rmtree(root, ignore_errors=True)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--launcher", type=Path)
    parser.add_argument("--driver", type=Path)
    parser.add_argument("--build-manifest", type=Path)
    parser.add_argument("--observer", type=Path)
    parser.add_argument("--evidence", type=Path)
    parser.add_argument("--work-parent", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        assert MAX_ATTEMPTS_PER_DIRECTION == 1
        assert PIN == "635b013a612ff47278ef02acf8580a28e10e26c5"
        print("plan414 runner self-test passed")
        return 0
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
        first = run_direction(launcher, driver, manifest, observer, parent, "forward")
        second = run_direction(launcher, driver, manifest, observer, parent, "reverse")
        record = {
            "schema": "i2pr-plan414-evidence-v1",
            "reference_revision": PIN,
            "max_attempts_per_direction": MAX_ATTEMPTS_PER_DIRECTION,
            "directions": [first, second],
            "result": "passed",
        }
        temporary = evidence.with_name(evidence.name + ".tmp")
        temporary.write_text(json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
        temporary.replace(evidence)
        return 0
    except (OSError, RunError, subprocess.SubprocessError, json.JSONDecodeError) as exc:
        reason = exc.reason if isinstance(exc, RunError) else "runner-failed"
        print(json.dumps({"schema": "i2pr-plan414-evidence-v1", "result": "rejected", "reason_code": reason}, separators=(",", ":")))
        return 2


if __name__ == "__main__":
    sys.exit(main())

