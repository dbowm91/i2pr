#!/usr/bin/env python3
"""One-shot stock-to-stock control for Plan 441.

Runs two pristine i2pd 2.61.0 helper processes with independent data roots and
identities over loopback. Durable output contains only the pinned revision,
fixed role outcomes, bounded stage counts, and process return categories.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import secrets
import shutil
import socket
import subprocess
import tempfile
import time

PIN = "635b013a612ff47278ef02acf8580a28e10e26c5"
TOPOLOGY = "current-network-loopback"
TIMEOUT_SECONDS = 75
MESSAGE_ID = 0x44100001


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 0)
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


def manifest_values(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        key, separator, value = line.partition("=")
        if separator:
            values[key] = value
    if values.get("reference_revision") != PIN:
        raise ValueError("reference-pin-mismatch")
    return values


def config_values(*, root: Path, driver: Path, manifest: Path,
                  mode: str, local_port: int, peer_port: int,
                  peer_info: Path, peer_hash: str, run_id: str) -> dict[str, object]:
    manifest_raw = manifest.read_bytes()
    entries = manifest_values(manifest)
    source = Path(__file__).resolve().parent / "src/i2pd_current_ntcp2_driver.cpp"
    return {
        "schema": "i2pr-i2pd-current-pin-driver-config-v1",
        "schema_version": 1,
        "run_id": run_id,
        "scenario_id": "plan441-stock-control",
        "invocation_id": secrets.token_hex(16),
        "direction": "i2pr-to-i2pd-ipv4",
        "mode": mode,
        "data_dir": str(root / "data"),
        "output_dir": str(root / ("output-" + mode)),
        "local_address": "127.0.0.1",
        "local_port": local_port,
        "network_id": 2,
        "peer_router_info_path": str(peer_info),
        "expected_peer_router_hash_sha256": peer_hash,
        "expected_peer_address": "127.0.0.1",
        "expected_peer_port": peer_port,
        "delivery_status_message_id": MESSAGE_ID,
        "startup_timeout_ms": 15_000,
        "handshake_timeout_ms": 30_000,
        "data_phase_timeout_ms": 30_000,
        "shutdown_timeout_ms": 10_000,
        "reference_revision": PIN,
        "reference_tree_sha256": entries["reference_tree_sha256"],
        "driver_source_sha256": digest(source.read_bytes()),
        "driver_binary_sha256": digest(driver.read_bytes()),
        "build_manifest_sha256": digest(manifest_raw),
        "run_identity_sha256": digest(run_id.encode()),
        "topology_kind": TOPOLOGY,
    }


def write_config(path: Path, values: dict[str, object]) -> None:
    path.write_text(json.dumps(values, sort_keys=True) + "\n", encoding="utf-8")
    path.chmod(0o600)


def router_hash(launcher: Path, router_info: Path) -> str:
    completed = subprocess.run(
        [str(launcher), "ntcp2", "router-hash", "--router-info", str(router_info)],
        check=True, capture_output=True, text=True, timeout=15,
    )
    record = json.loads(completed.stdout)
    value = record.get("router_hash_sha256") if isinstance(record, dict) else None
    if not isinstance(value, str) or len(value) != 64 or any(c not in "0123456789abcdef" for c in value):
        raise ValueError("router-info-identity-unavailable")
    return value


def read_events(path: Path) -> list[dict[str, object]]:
    records: list[dict[str, object]] = []
    if not path.is_file():
        return records
    for line in path.read_text(encoding="utf-8").splitlines()[:128]:
        try:
            record = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(record, dict):
            records.append(record)
    return records


def wait_event(path: Path, kind: str, process: subprocess.Popen[str], deadline: float) -> bool:
    while time.monotonic() < deadline:
        if any(record.get("event_kind") == kind for record in read_events(path)):
            return True
        if process.poll() is not None:
            return False
        time.sleep(0.025)
    return False


def stop(process: subprocess.Popen[str], deadline: float) -> int:
    remaining = max(0.0, deadline - time.monotonic())
    try:
        return process.wait(timeout=remaining)
    except subprocess.TimeoutExpired:
        process.terminate()
        try:
            return process.wait(timeout=2)
        except subprocess.TimeoutExpired:
            process.kill()
            return process.wait(timeout=2)


def accepted(records: list[dict[str, object]], *, listener: bool) -> bool:
    kinds = [row.get("event_kind") for row in records]
    required = ["ntcp2_authenticated", "i2np_message_decoded", "frame_emitted", "terminal_clean"]
    if listener:
        required.append("listener_ready")
    else:
        required.append("frame_emitted")
    matching = [row for row in records if row.get("event_kind") == "i2np_message_decoded"
                and row.get("i2np_type") == 10
                and row.get("delivery_status_message_id") == MESSAGE_ID]
    return all(kind in kinds for kind in required) and len(matching) == 1


def terminal_reason(records: list[dict[str, object]]) -> str | None:
    for row in records:
        reason = row.get("reason_code")
        if row.get("event_kind") == "terminal_rejected" and isinstance(reason, str):
            # The helper's event writer strips detail after the first colon.
            # Keep only a bounded category; never persist process output or detail.
            if re.fullmatch(r"[a-z][a-z0-9-]{0,63}", reason):
                return reason
            return "unclassified-helper-rejection"
    return None


def run_control(driver: Path, manifest: Path, launcher: Path,
                work_parent: Path) -> dict[str, object]:
    root = Path(tempfile.mkdtemp(prefix="i2pr-plan441-", dir=work_parent))
    root.chmod(0o700)
    processes: list[subprocess.Popen[str]] = []
    result: dict[str, object] = {
        "schema": "i2pr-plan441-stock-control-v1",
        "reference_revision": PIN,
        "runner_sha256": digest(Path(__file__).read_bytes()),
        "topology": "loopback-controlled",
        "attempt_budget": 1,
        "result": "environment-blocked",
        "listener": "not-run",
        "dialer": "not-run",
        "cleanup": "pending",
    }
    try:
        manifest_values(manifest)
        ports = (free_port(), free_port())
        if ports[0] == ports[1]:
            raise ValueError("port-collision")
        run_id = secrets.token_hex(16)
        roots = (root / "router-a", root / "router-b")
        for router_root in roots:
            router_root.mkdir(mode=0o700)
        placeholder = root / "unused-peer.info"
        for index, (router_root, local_port) in enumerate(zip(roots, ports)):
            values = config_values(
                root=router_root, driver=driver, manifest=manifest,
                mode="inspect", local_port=local_port, peer_port=ports[1-index],
                peer_info=placeholder, peer_hash="1" * 64, run_id=run_id,
            )
            config = root / f"inspect-{index}.json"
            write_config(config, values)
            subprocess.run([str(driver), "--config", str(config)], check=True,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                           timeout=30)
        infos = tuple(router_root / "output-inspect/router.info" for router_root in roots)
        peer_hashes = tuple(router_hash(launcher, info) for info in infos)

        def active(index: int, mode: str) -> Path:
            values = config_values(
                root=roots[index], driver=driver, manifest=manifest, mode=mode,
                local_port=ports[index], peer_port=ports[1-index],
                peer_info=infos[1-index], peer_hash=str(peer_hashes[1-index]),
                run_id=run_id,
            )
            path = root / f"active-{index}.json"
            write_config(path, values)
            return path

        listener_config = active(0, "listen")
        dialer_config = active(1, "dial")
        listener = subprocess.Popen([str(driver), "--config", str(listener_config)],
                                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                    text=True)
        processes.append(listener)
        listener_events = roots[0] / "output-listen/events.ndjson"
        deadline = time.monotonic() + TIMEOUT_SECONDS
        if not wait_event(listener_events, "listener_ready", listener, deadline):
            stop(listener, deadline)
            raise ValueError("listener-not-ready")
        dialer = subprocess.Popen([str(driver), "--config", str(dialer_config)],
                                  stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                  text=True)
        processes.append(dialer)
        dialer_rc = stop(dialer, deadline)
        listener_rc = stop(listener, deadline)
        dialer_records = read_events(roots[1] / "output-dial/events.ndjson")
        listener_records = read_events(listener_events)
        result["dialer"] = "passed" if dialer_rc == 0 and accepted(dialer_records, listener=False) else "rejected"
        result["listener"] = "passed" if listener_rc == 0 and accepted(listener_records, listener=True) else "rejected"
        result["result"] = "stock-to-stock-passed" if result["dialer"] == result["listener"] == "passed" else "environment-blocked"
        result["dialer_reason_code"] = terminal_reason(dialer_records) or "no-terminal-rejection"
        result["listener_reason_code"] = terminal_reason(listener_records) or "no-terminal-rejection"
        result["dialer_exit"] = max(-255, min(255, dialer_rc))
        result["listener_exit"] = max(-255, min(255, listener_rc))
        result["dialer_stage_count"] = min(32, len(dialer_records))
        result["listener_stage_count"] = min(32, len(listener_records))
    except (OSError, ValueError, subprocess.SubprocessError, KeyError, StopIteration):
        result["result"] = "environment-blocked"
    finally:
        for process in processes:
            if process.poll() is None:
                stop(process, time.monotonic() + 2)
        shutil.rmtree(root, ignore_errors=True)
        result["cleanup"] = "removed"
    return result


def self_test() -> bool:
    sample = [{"event_kind": kind} for kind in (
        "listener_ready", "ntcp2_authenticated", "frame_emitted",
        "i2np_message_decoded", "terminal_clean",
    )]
    sample[-2].update({"i2np_type": 10, "delivery_status_message_id": MESSAGE_ID})
    if not accepted(sample, listener=True) or not accepted(sample, listener=False):
        return False
    mutations = [
        [row for row in sample if row.get("event_kind") != "ntcp2_authenticated"],
        [row for row in sample if row.get("event_kind") != "i2np_message_decoded"],
        [dict(row, delivery_status_message_id=0) if row.get("event_kind") == "i2np_message_decoded" else row for row in sample],
        sample + [dict(sample[-2])],
    ]
    return (all(not accepted(rows, listener=True) and not accepted(rows, listener=False)
                for rows in mutations)
            and terminal_reason([{"event_kind": "terminal_rejected",
                                  "reason_code": "listening-handshake-timeout"}])
            == "listening-handshake-timeout"
            and terminal_reason([{"event_kind": "terminal_rejected",
                                  "reason_code": "/tmp/private-router/path"}])
            == "unclassified-helper-rejection")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--driver", type=Path)
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--launcher", type=Path)
    parser.add_argument("--work-dir", type=Path)
    parser.add_argument("--evidence", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        print("Plan 441 stock control self-test passed" if self_test()
              else "Plan 441 stock control self-test failed")
        return 0 if self_test() else 1
    if not args.driver or not args.manifest or not args.launcher or not args.work_dir or not args.evidence:
        parser.error("--driver, --manifest, --launcher, --work-dir, and --evidence are required")
    result = run_control(args.driver.resolve(), args.manifest.resolve(),
                         args.launcher.resolve(), args.work_dir.resolve())
    args.evidence.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    args.evidence.write_text(json.dumps(result, sort_keys=True) + "\n", encoding="utf-8")
    args.evidence.chmod(0o600)
    print(json.dumps(result, sort_keys=True))
    return 0 if result["result"] == "stock-to-stock-passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
