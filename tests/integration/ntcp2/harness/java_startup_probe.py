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
import json
import os
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
ALLOWED_DATA_STATE = {"empty", "config-only", "fresh-unique-seed", "initialized-snapshot"}
ALLOWED_SEQUENCE = {"single", "generate-live"}
ALLOWED_ENTROPY = {"ok", "degraded", "unavailable", "not-tested"}


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

    try:
        with open("/dev/urandom", "rb") as handle:
            data = handle.read(32)
        if len(data) < 32:
            return "degraded"
    except OSError:
        return "unavailable"
    return "ok"


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
    raise ProbeError("unknown-data-state")


def _run_attempt(
    *,
    reference_install: Path,
    data_dir: Path,
    launcher: str,
    namespace_placement: str,
    sequence: str,
    entropy: str,
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
    process = BoundedProcess(
        [str(launcher_path)],
        raw_log,
        environment=env,
    )
    process.start()
    readiness_marker = "Starting I2P"
    readiness_at = None
    try:
        try:
            process.wait_ready((readiness_marker,), timeout_seconds)
            readiness_at = time.monotonic() - started_at
        except ProcessError:
            readiness_at = None
    finally:
        cleanup = process.stop(timeout_seconds=5.0)
    return {
        "process_started": True,
        "readiness_marker": readiness_marker,
        "readiness_observed": readiness_at is not None,
        "readiness_monotonic_seconds": readiness_at,
        "cleanup_result": cleanup,
        "log_bytes": process.snapshot().get("log_bytes", 0),
        "entropy": entropy,
        "data_state_inventory": inventory,
        "namespace_placement": namespace_placement,
        "launcher": launcher,
        "sequence": sequence,
    }


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
    args = parser.parse_args()
    if args.attempts < 1:
        raise SystemExit(2)
    entropy = _entropy_class()
    template = args.state_template if args.state_template else Path("")
    attempts: list[dict[str, Any]] = []
    failures: list[str] = []
    for index in range(args.attempts):
        attempt_id = _attempt_id()
        with tempfile.TemporaryDirectory(prefix=f"i2pr-probe-{attempt_id}-") as directory:
            data_dir = Path(directory)
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
                attempts.append(attempt)
            except ProbeError as exc:
                failures.append(f"{attempt_id}: {exc.code}")
    ready_count = sum(1 for attempt in attempts if attempt.get("readiness_observed"))
    record = {
        "schema": PROBE_SCHEMA,
        "schema_version": PROBE_SCHEMA_VERSION,
        "type": "java-startup-probe",
        "created_at_utc": _now(),
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