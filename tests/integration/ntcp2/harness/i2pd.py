"""Pinned i2pd process adapter for isolated Plan 038 runs."""

from __future__ import annotations

import hashlib
import os
import shutil
from pathlib import Path

from .process import BoundedProcess, ProcessError


class I2pdError(RuntimeError):
    """An i2pd adapter precondition or lifecycle operation failed."""

    def __init__(self, code: str):
        super().__init__(code)
        self.code = code


class I2pdAdapter:
    """Manage a foreground, explicitly staged i2pd process."""

    version = "2.60.0"
    revision = "f618e41"

    def __init__(self, cache: Path, run_root: Path, namespace: str, repo_root: Path):
        self.cache = cache
        self.run_root = run_root
        self.namespace = namespace
        self.repo_root = repo_root
        self.data_dir = run_root / "reference-data"
        self.config_dir = run_root / "config"
        self.process: BoundedProcess | None = None

    def _prefix(self) -> list[str]:
        return [] if os.geteuid() == 0 else ["sudo", "-n"]

    def prepare(self) -> Path:
        binary = self.cache / "bin" / "i2pd"
        if not binary.is_file() or not os.access(binary, os.X_OK):
            raise I2pdError("missing-reference-cache")
        self.data_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
        self.config_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
        template = self.repo_root / "tests/integration/ntcp2/config/i2pd/i2pd.conf.template"
        rendered = template.read_text(encoding="utf-8")
        rendered = rendered.replace("@DATA_DIR@", str(self.data_dir))
        required = (
            "daemon = false",
            "address4 = 192.0.2.2",
            "notransit = true",
            "floodfill = false",
            "[ntcp2]",
            "enabled = true",
            "[ssu2]",
            "enabled = false",
            "[upnp]",
            "[reseed]",
            "threshold = 0",
            "port = 45679",
        )
        if any(line not in rendered for line in required):
            raise I2pdError("safety-configuration-assertion-failed")
        config = self.config_dir / "i2pd.conf"
        config.write_text(rendered, encoding="utf-8")
        (self.config_dir / "tunnels.conf").write_text(
            (self.repo_root / "tests/integration/ntcp2/config/i2pd/tunnels.conf.template")
            .read_text(encoding="utf-8"),
            encoding="utf-8",
        )
        (self.run_root / "config.sha256").write_text(
            hashlib.sha256(rendered.encode()).hexdigest() + "\n", encoding="ascii"
        )
        return binary

    def start(self) -> None:
        binary = self.prepare()
        command = self._prefix() + [
            "ip",
            "netns",
            "exec",
            self.namespace,
            str(binary),
            "--datadir",
            str(self.data_dir),
            "--conf",
            str(self.config_dir / "i2pd.conf"),
        ]
        self.process = BoundedProcess(command, self.run_root / "raw" / "i2pd.log")
        try:
            self.process.start()
        except OSError as exc:
            raise I2pdError("process-start-failed") from exc

    def wait_ready(self, timeout_seconds: float = 30.0) -> None:
        if self.process is None:
            raise I2pdError("not-started")
        try:
            self.process.wait_ready(("i2pd", "NTCP2"), timeout_seconds)
        except ProcessError as exc:
            raise I2pdError(exc.code) from exc

    def export_router_info(self) -> Path:
        candidates = (self.data_dir / "router.info", self.data_dir / "router.info.su3")
        for candidate in candidates:
            if candidate.is_file():
                target = self.run_root / "exchange" / "i2pd-router.info"
                target.parent.mkdir(mode=0o700, exist_ok=True)
                shutil.copyfile(candidate, target)
                return target
        raise I2pdError("router-info-not-produced")

    def import_peer_router_info(self, source: Path) -> None:
        if self.run_root not in source.resolve().parents:
            raise I2pdError("peer-router-info-outside-run-root")
        target = self.data_dir / "netdb" / source.name
        target.parent.mkdir(mode=0o700, exist_ok=True)
        shutil.copyfile(source, target)

    def query_typed_state(self) -> dict[str, int | str]:
        if self.process is None:
            return {"state": "not-started"}
        return {"state": "running" if self.process.snapshot()["running"] else "stopped"}

    def stop(self, timeout_seconds: float = 5.0) -> str:
        if self.process is None:
            return "not-started"
        return self.process.stop(timeout_seconds)
