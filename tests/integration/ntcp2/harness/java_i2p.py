"""Pinned Java I2P process adapter.

The adapter only invokes a staged artifact from ``target/interop/cache`` and
never a system service. Its configuration names are recorded in the adjacent
README and are asserted before launch.
"""

from __future__ import annotations

import hashlib
import os
import shutil
from pathlib import Path

from .process import BoundedProcess, ProcessError


class JavaI2pError(RuntimeError):
    """A Java I2P adapter precondition or lifecycle operation failed."""

    def __init__(self, code: str):
        super().__init__(code)
        self.code = code


class JavaI2pAdapter:
    """Manage one disposable Java I2P instance inside a namespace."""

    version = "2.12.0"
    revision = "2800040"

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
        metadata = self.cache / "build-metadata.txt"
        if not metadata.is_file():
            raise JavaI2pError("missing-reference-cache")
        values = {}
        for line in metadata.read_text(encoding="utf-8").splitlines():
            if "=" in line:
                key, value = line.split("=", 1)
                values[key] = value
        launcher_name = values.get("launcher")
        if not launcher_name or "/" in launcher_name or launcher_name.startswith("."):
            raise JavaI2pError("invalid-staged-launcher")
        launcher = self.cache / launcher_name
        if not launcher.exists():
            raise JavaI2pError("missing-staged-launcher")
        self.data_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
        self.config_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
        template = self.repo_root / "tests/integration/ntcp2/config/java-i2p/router.config.template"
        rendered = template.read_text(encoding="utf-8")
        replacements = {
            "@DATA_DIR@": str(self.data_dir),
            "@CONFIG_DIR@": str(self.config_dir),
            "@NTCP2_ADDRESS@": "192.0.2.1",
            "@NTCP2_PORT@": "45678",
        }
        for key, value in replacements.items():
            rendered = rendered.replace(key, value)
        required = (
            "i2np.allowLocal=true",
            "router.reseedDisable=true",
            "router.updateDisabled=true",
            "i2np.ntcp.port=45678",
            "i2np.ntcp.hostname=192.0.2.1",
            "i2np.ntcp.autoip=false",
            "i2np.ntcp.autoport=false",
            "i2np.upnp.enable=false",
            "i2np.udp.enable=false",
            "router.floodfillParticipant=false",
            "router.maxParticipatingTunnels=0",
        )
        if any(line not in rendered for line in required):
            raise JavaI2pError("safety-configuration-assertion-failed")
        (self.config_dir / "router.config").write_text(rendered, encoding="utf-8")
        (self.config_dir / "clients.config").write_text(
            (self.repo_root / "tests/integration/ntcp2/config/java-i2p/clients.config.template")
            .read_text(encoding="utf-8")
            .replace("@DATA_DIR@", str(self.data_dir)),
            encoding="utf-8",
        )
        (self.run_root / "config.sha256").write_text(
            hashlib.sha256(rendered.encode()).hexdigest() + "\n", encoding="ascii"
        )
        return launcher

    def start(self) -> None:
        launcher = self.prepare()
        command = self._prefix() + ["ip", "netns", "exec", self.namespace, str(launcher)]
        environment = os.environ.copy()
        environment["JAVA_TOOL_OPTIONS"] = (
            f"-Di2p.dir.base={self.cache} -Di2p.dir.config={self.config_dir} -Xmx256m"
        )
        self.process = BoundedProcess(
            command,
            self.run_root / "raw" / "java-i2p.log",
            environment=environment,
        )
        try:
            self.process.start()
        except OSError as exc:
            raise JavaI2pError("process-start-failed") from exc

    def wait_ready(self, timeout_seconds: float = 30.0) -> None:
        if self.process is None:
            raise JavaI2pError("not-started")
        try:
            self.process.wait_ready(("Router is ready", "I2P Router ready"), timeout_seconds)
        except ProcessError as exc:
            raise JavaI2pError(exc.code) from exc

    def export_router_info(self) -> Path:
        candidates = (self.data_dir / "router.info", self.data_dir / "router.info.su3")
        for candidate in candidates:
            if candidate.is_file():
                target = self.run_root / "exchange" / "java-router.info"
                target.parent.mkdir(mode=0o700, exist_ok=True)
                shutil.copyfile(candidate, target)
                return target
        raise JavaI2pError("router-info-not-produced")

    def import_peer_router_info(self, source: Path) -> None:
        if self.run_root not in source.resolve().parents:
            raise JavaI2pError("peer-router-info-outside-run-root")
        target = self.data_dir / "netDb" / source.name
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
