"""Non-production i2pr interoperability launcher adapter."""

from __future__ import annotations

import os
from pathlib import Path

from .process import BoundedProcess, ProcessError


class I2prAdapter:
    """Invoke the dedicated launcher, never the normal daemon."""

    def __init__(self, repo_root: Path, run_root: Path, namespace: str):
        self.repo_root = repo_root
        self.run_root = run_root
        self.namespace = namespace
        self.process: BoundedProcess | None = None

    def start(self, mode: str) -> None:
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
        self.process.start()

    def wait_ready(self, timeout_seconds: float = 30.0) -> None:
        if self.process is None:
            raise RuntimeError("i2pr-not-started")
        try:
            self.process.wait_ready(("blocked_missing_driver", "ready"), timeout_seconds)
        except ProcessError as exc:
            raise RuntimeError(exc.code) from exc

    def stop(self, timeout_seconds: float = 5.0) -> str:
        if self.process is None:
            return "not-started"
        return self.process.stop(timeout_seconds)
