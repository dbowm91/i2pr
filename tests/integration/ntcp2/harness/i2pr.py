"""Non-production i2pr interoperability launcher adapter."""

from __future__ import annotations

import hashlib
import os
import shutil
from pathlib import Path

try:
    from .launcher_protocol import LauncherStatusError, parse_status_line
    from .process import BoundedProcess, ProcessError
except ImportError:  # pragma: no cover - direct harness-module execution
    from launcher_protocol import LauncherStatusError, parse_status_line  # type: ignore
    from process import BoundedProcess, ProcessError  # type: ignore


class I2prAdapter:
    """Invoke the dedicated launcher, never the normal daemon."""

    def __init__(self, repo_root: Path, run_root: Path, namespace: str):
        self.repo_root = repo_root
        self.run_root = run_root
        self.namespace = namespace
        self.process: BoundedProcess | None = None
        self.mode: str | None = None
        self.last_status: dict[str, object] | None = None

    def start(self, mode: str) -> None:
        if mode not in {"listen", "dial"}:
            raise RuntimeError("invalid-i2pr-mode")
        binary = self.repo_root / "target" / "debug" / "i2pr-interop"
        if not binary.is_file():
            raise RuntimeError("missing-i2pr-interop-launcher")
        command = ([] if os.geteuid() == 0 else ["sudo", "-n"]) + [
            "ip",
            "netns",
            "exec",
            self.namespace,
            str(binary),
            "ntcp2",
            mode,
            "--scenario-config",
            str(self.run_root / "scenario.toml"),
        ]
        self.process = BoundedProcess(command, self.run_root / "raw" / "i2pr.log")
        self.mode = mode
        self.process.start()

    def wait_ready(self, timeout_seconds: float = 30.0) -> None:
        if self.process is None:
            raise RuntimeError("i2pr-not-started")
        if self.mode != "listen":
            raise RuntimeError("readiness-not-available-for-dial")
        try:
            status = self.process.wait_for_record(_parse_status, timeout_seconds)
        except ProcessError as exc:
            raise RuntimeError(exc.code) from exc
        self.last_status = status
        if status["phase"] != "listener_ready" or status["result"] != "ready":
            raise RuntimeError("terminal-status-before-readiness")

    def wait_terminal(self, timeout_seconds: float = 30.0) -> dict[str, object]:
        if self.process is None:
            raise RuntimeError("i2pr-not-started")
        try:
            status = self.process.wait_for_record(_parse_terminal_status, timeout_seconds)
        except ProcessError as exc:
            raise RuntimeError(exc.code) from exc
        self.last_status = status
        return status

    def export_router_info(self, *, state_dir: str = "state") -> Path:
        """Copy the i2pr launcher's persisted RouterInfo into the run-root exchange dir.

        The Rust launcher writes ``state_dir/router.info``; the previous
        ``exchange/router.info`` path did not exist after a generation pass
        and was the source of Plan 045 D2.
        """
        source = (self.run_root / state_dir / "router.info").resolve()
        if not self._inside_run_root(source):
            raise RuntimeError("router-info-outside-run-root")
        if not source.is_file():
            raise RuntimeError("router-info-not-produced")
        target = (self.run_root / "exchange" / "i2pr-router.info").resolve()
        if not self._inside_run_root(target):
            raise RuntimeError("exported-router-info-outside-run-root")
        target.parent.mkdir(mode=0o700, exist_ok=True)
        shutil.copyfile(source, target)
        return target

    def public_digest(self, *, state_dir: str = "state") -> str:
        """Return a SHA-256 of the persisted RouterInfo bytes for evidence."""

        path = self.run_root / state_dir / "router.info"
        try:
            return hashlib.sha256(path.read_bytes()).hexdigest()
        except OSError:
            return ""

    def stop(self, timeout_seconds: float = 5.0) -> str:
        if self.process is None:
            return "not-started"
        return self.process.stop(timeout_seconds)

    def _inside_run_root(self, path: Path) -> bool:
        resolved = path.resolve()
        return resolved == self.run_root or self.run_root in resolved.parents


def _parse_status(line: str) -> dict[str, object] | None:
    try:
        return parse_status_line(line)
    except LauncherStatusError:
        return None


def _parse_terminal_status(line: str) -> dict[str, object] | None:
    status = _parse_status(line)
    if status is not None and status["phase"] == "terminal":
        return status
    return None
