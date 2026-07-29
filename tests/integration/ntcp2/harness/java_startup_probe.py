"""Plan 052 standalone Java startup probe.

The probe exists to isolate the Plan 052 Java startup diagnosis from the
rest of the harness. It runs only the pinned Java reference against one
specified data-state class, namespace placement, and launcher. It does NOT
run i2pr, does NOT attempt any NTCP2 peer connection, and does NOT classify
a transport handshake.

It emits a sanitized per-attempt record under the output directory:

```text
<output>/java-startup-probe.json
```

The record carries:

- attempt ID;
- namespace placement;
- data-state class;
- launcher identity (runplain vs wrapper);
- entropy device accessibility (typed);
- file inventory (allowlisted names only);
- process tree before / at-readiness-or-failure / after cleanup;
- per-attempt readiness outcome;
- per-attempt cleanup outcome.

The probe is intentionally narrow: if Java is intermittent on this host,
the probe isolates whether the failure is Java, the wrapper, the seed, or
the namespace. It does not declare a plan-052 evidence outcome.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import secrets
import shutil
import subprocess
import sys
import tempfile
import time
import uuid
from pathlib import Path
from typing import Any

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[3]
sys.path.insert(0, str(HERE))

from process import BoundedProcess, ProcessError  # noqa: E402


PROBE_SCHEMA = "i2pr-java-startup-probe-v1"
PROBE_SCHEMA_VERSION = 1

ALLOWED_NAMESPACE = {"outer", "rootless"}
ALLOWED_LAUNCHER = {"runplain", "wrapper"}
ALLOWED_DATA_STATE = {"empty", "config-only", "fresh-unique-seed", "initialized-snapshot", "seeded-clone"}
ALLOWED_SEQUENCE = {"single", "generate-live"}
ALLOWED_ENTROPY = {"ok", "degraded", "unavailable", "not-tested"}
FAILURE_STAGES = (
    "java-process-spawn-failed",
    "java-wrapper-bootstrap-failed",
    "java-router-start-marker-missing",
    "java-random-source-shutdown",
    "java-key-generation-failed",
    "java-ntcp2-configuration-failed",
    "java-listener-readiness-timeout",
    "java-premature-process-exit",
    "java-graceful-shutdown-timeout",
    "java-residual-process",
    "java-state-permission-invalid",
    "java-state-lock-invalid",
)


class ProbeError(RuntimeError):
    def __init__(self, code: str):
        super().__init__(code)
        self.code = code


def _now() -> str:
    return dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def _attempt_id() -> str:
    return f"attempt-{dt.datetime.now(dt.UTC).strftime('%Y%m%dT%H%M%SZ')}-{uuid.uuid4().hex[:8]}"


def _entropy_class() -> str:
    """Classify ``/dev/urandom`` and ``/dev/random`` accessibility."""

    return _entropy_probe()["class"]


def _latency_bucket(milliseconds: float) -> str:
    if milliseconds < 10:
        return "0-10"
    if milliseconds < 100:
        return "10-100"
    if milliseconds < 1000:
        return "100-1000"
    return "1000+"


def _entropy_probe(seed_file: Path | None = None) -> dict[str, Any]:
    result: dict[str, Any] = {
        "class": "unavailable",
        "getrandom_result": "failure",
        "latency_bucket_ms": "unknown",
        "urandom_readable": False,
        "random_readable": False,
        "seed_file_state": "absent",
        "seed_file_sha256": "",
    }
    try:
        with open("/dev/urandom", "rb") as handle:
            result["urandom_readable"] = len(handle.read(32)) == 32
    except OSError:
        pass
    try:
        with open("/dev/random", "rb") as handle:
            result["random_readable"] = len(handle.read(1)) == 1
    except OSError:
        pass
    started = time.monotonic()
    try:
        secrets.token_bytes(1)
        result["getrandom_result"] = "success"
        result["latency_bucket_ms"] = _latency_bucket((time.monotonic() - started) * 1000)
    except OSError:
        result["latency_bucket_ms"] = _latency_bucket((time.monotonic() - started) * 1000)
    if seed_file and seed_file.is_file():
        try:
            size = seed_file.stat().st_size
            result["seed_file_state"] = "present-empty" if size == 0 else "present-nonempty"
            result["seed_file_sha256"] = hashlib.sha256(seed_file.read_bytes()).hexdigest()
        except OSError:
            result["seed_file_state"] = "unreadable"
    result["class"] = "ok" if result["getrandom_result"] == "success" and result["urandom_readable"] else "degraded"
    return result


def _classify_failure(code: str, *, launcher: str = "runplain", exit_code: int | None = None) -> str:
    if code in FAILURE_STAGES:
        return code
    if code in {"process-start-failed", "launcher-missing"}:
        return "java-process-spawn-failed"
    if launcher == "wrapper" and code in {"java-eventlog-started-timeout", "premature-exit"}:
        return "java-wrapper-bootstrap-failed"
    if code in {"java-eventlog-started-timeout", "readiness-timeout"}:
        return "java-listener-readiness-timeout"
    if code in {"java-premature-exit", "premature-exit"} or exit_code not in (None, 0):
        return "java-premature-process-exit"
    if code in {"java-state-file-mode-not-private", "java-state-permission-invalid"}:
        return "java-state-permission-invalid"
    if code in {"java-state-lock-invalid", "router-ping-stale"}:
        return "java-state-lock-invalid"
    return "java-router-start-marker-missing"


def _inventory_allowlisted(data_dir: Path) -> list[dict[str, str]]:
    """Return mode+size+sha256 for allowlisted Java state files."""

    allowlist = ("router.config", "clients.config", "router.info",
                 "prngseed.rnd", "eventlog.txt", "wrapper.config")
    inventory: list[dict[str, str]] = []
    for name in allowlist:
        path = data_dir / name
        if not path.exists():
            continue
        try:
            stat = path.stat()
        except OSError:
            continue
        if stat.st_mode & 0o077:
            raise ProbeError("java-state-file-mode-not-private")
        import hashlib
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        inventory.append({
            "name": name,
            "size": str(stat.st_size),
            "mode": oct(stat.st_mode & 0o777),
            "sha256": digest,
        })
    return inventory


def _ensure_data_state(
    *,
    template: Path,
    data_dir: Path,
    data_state: str,
) -> None:
    if data_state == "empty":
        data_dir.mkdir(parents=True, exist_ok=True)
        return
    if data_state == "config-only":
        if not template.exists():
            raise ProbeError("template-missing")
        data_dir.mkdir(parents=True, exist_ok=True)
        for name in ("router.config", "clients.config"):
            source = template / name
            if source.exists():
                shutil.copy2(source, data_dir / name)
        return
    if data_state == "fresh-unique-seed":
        data_dir.mkdir(parents=True, exist_ok=True)
        # Generate a unique 64-byte prngseed from /dev/urandom. Plan 052 E5.
        seed = data_dir / "prngseed.rnd"
        with open("/dev/urandom", "rb") as source, open(seed, "wb") as target:
            target.write(source.read(64))
        seed.chmod(0o600)
        return
    if data_state == "initialized-snapshot":
        if not template.exists():
            raise ProbeError("template-missing")
        shutil.copytree(template, data_dir)
        return
    if data_state == "seeded-clone":
        if not template.is_dir():
            raise ProbeError("template-missing")
        if data_dir.resolve() == template.resolve():
            raise ProbeError("template-launch-forbidden")
        shutil.copytree(template, data_dir)
        return
    raise ProbeError("unknown-data-state")


def _run_attempt(
    *,
    reference_install: Path,
    data_dir: Path,
    launcher: str,
    namespace_placement: str,
    sequence: str,
    entropy: dict[str, Any],
    inventory: list[dict[str, str]],
    timeout_seconds: float,
) -> dict[str, Any]:
    if launcher not in ALLOWED_LAUNCHER:
        raise ProbeError("unknown-launcher")
    if namespace_placement not in ALLOWED_NAMESPACE:
        raise ProbeError("unknown-namespace-placement")
    if sequence not in ALLOWED_SEQUENCE:
        raise ProbeError("unknown-sequence")
    launcher_path = reference_install / ("runplain.sh" if launcher == "runplain" else "i2prouter")
    if not launcher_path.exists():
        raise ProbeError("launcher-missing")
    raw_log = data_dir / "java-i2p-probe.log"
    env = os.environ.copy()
    env["I2PHOME"] = str(data_dir)
    started_at = time.monotonic()
    failure_stage: str | None = None
    process = BoundedProcess(
        [str(launcher_path)],
        raw_log,
        environment=env,
    )
    try:
        process.start()
    except OSError as exc:
        raise ProbeError("process-start-failed") from exc
    readiness_marker = "Starting I2P"
    readiness_at = None
    try:
        try:
            process.wait_ready((readiness_marker,), timeout_seconds)
            readiness_at = time.monotonic() - started_at
        except ProcessError as exc:
            failure_stage = _classify_failure(exc.code, launcher=launcher)
            readiness_at = None
    finally:
        if process.process is not None and process.process.poll() is not None and failure_stage is None:
            failure_stage = _classify_failure("java-premature-exit", launcher=launcher, exit_code=process.process.returncode)
        cleanup = process.stop(timeout_seconds=5.0)
        if cleanup == "failed":
            failure_stage = failure_stage or "java-graceful-shutdown-timeout"
    return {
        "process_started": True,
        "readiness_marker": readiness_marker,
        "readiness_observed": readiness_at is not None,
        "readiness_monotonic_seconds": readiness_at,
        "cleanup_result": cleanup,
        "failure_stage": failure_stage or "",
        "log_bytes": process.snapshot().get("log_bytes", 0),
        "entropy": entropy,
        "data_state_inventory": inventory,
        "namespace_placement": namespace_placement,
        "launcher": launcher,
        "sequence": sequence,
    }


def _cell_id(launcher: str, data_state: str, namespace_placement: str, sequence: str) -> str:
    return f"{namespace_placement}__{data_state}__{launcher}__{sequence}"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--reference-install", type=Path, required=True)
    parser.add_argument("--data-dir", type=Path, required=True)
    parser.add_argument("--data-state", choices=sorted(ALLOWED_DATA_STATE), required=True)
    parser.add_argument("--launcher", choices=sorted(ALLOWED_LAUNCHER), required=True)
    parser.add_argument("--namespace", choices=sorted(ALLOWED_NAMESPACE), required=True)
    parser.add_argument("--sequence", choices=sorted(ALLOWED_SEQUENCE), default="single")
    parser.add_argument("--attempts", type=int, default=1)
    parser.add_argument("--timeout-seconds", type=float, default=60.0)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--state-template", type=Path, default=Path(""))
    parser.add_argument("--matrix-run-id", default="")
    parser.add_argument("--qualification-mode", action="store_true")
    args = parser.parse_args()
    if args.attempts < 1:
        raise SystemExit(2)
    seed_file = args.state_template / "prngseed.rnd" if args.state_template.is_dir() else None
    entropy = _entropy_probe(seed_file=seed_file)
    template = args.state_template if args.state_template else Path("")
    attempts: list[dict[str, Any]] = []
    failures: list[str] = []
    for index in range(args.attempts):
        attempt_id = _attempt_id()
        cell_id = _cell_id(args.launcher, args.data_state, args.namespace, args.sequence)
        with tempfile.TemporaryDirectory(prefix=f"i2pr-probe-{attempt_id}-") as directory:
            data_dir = Path(directory) / "data"
            data_dir.mkdir(parents=True, exist_ok=True)
            try:
                _ensure_data_state(
                    template=template,
                    data_dir=data_dir,
                    data_state=args.data_state,
                )
            except ProbeError as exc:
                failures.append(f"{attempt_id}: {exc.code}")
                continue
            inventory = _inventory_allowlisted(data_dir)
            try:
                attempt = _run_attempt(
                    reference_install=args.reference_install,
                    data_dir=data_dir,
                    launcher=args.launcher,
                    namespace_placement=args.namespace,
                    sequence=args.sequence,
                    entropy=entropy,
                    inventory=inventory,
                    timeout_seconds=args.timeout_seconds,
                )
                attempt["attempt_id"] = attempt_id
                attempt["attempt_index"] = index + 1
                attempt["cell_id"] = cell_id
                attempt["qualification_mode"] = bool(args.qualification_mode)
                attempts.append(attempt)
            except ProbeError as exc:
                failures.append(f"{attempt_id}: {_classify_failure(exc.code, launcher=args.launcher)}")
    ready_count = sum(1 for attempt in attempts if attempt.get("readiness_observed"))
    record = {
        "schema": PROBE_SCHEMA,
        "schema_version": PROBE_SCHEMA_VERSION,
        "type": "java-startup-probe",
        "created_at_utc": _now(),
        "matrix_run_id": args.matrix_run_id or None,
        "qualification_mode": bool(args.qualification_mode),
        "cell_id": _cell_id(args.launcher, args.data_state, args.namespace, args.sequence),
        "data_state": args.data_state,
        "launcher": args.launcher,
        "namespace_placement": args.namespace,
        "sequence": args.sequence,
        "attempts_total": args.attempts,
        "attempts_recorded": len(attempts),
        "attempts_ready": ready_count,
        "failures": failures,
        "entropy": entropy,
        "attempts": attempts,
    }
    args.output.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=f".{args.output.name}.", dir=args.output.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            handle.write(json.dumps(record, sort_keys=False, separators=(",", ":")) + "\n")
            handle.flush()
            os.fsync(handle.fileno())
        os.chmod(temporary, 0o600)
        os.replace(temporary, args.output)
    finally:
        if Path(temporary).exists():
            Path(temporary).unlink()
    if failures and not attempts:
        # No attempt could even start; the probe is unusable in this
        # configuration. Exit non-zero so callers do not silently record
        # an empty success.
        return 1
    if ready_count < len(attempts):
        # At least one attempt did not reach readiness. Exit non-zero but
        # still emit the per-attempt record so the failure is auditable.
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())