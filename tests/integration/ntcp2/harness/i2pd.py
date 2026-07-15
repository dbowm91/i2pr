"""Pinned i2pd process adapter for isolated Plan 040 runs."""

from __future__ import annotations

import hashlib
import os
import shutil
from pathlib import Path

from .metadata import CacheMetadata, MetadataError, parse_metadata
from .process import BoundedProcess, ProcessError
from .router_info import RouterInfoPathError, netdb_filename
from .topology import EndpointDescription


class I2pdError(RuntimeError):
    """An i2pd adapter precondition or lifecycle operation failed."""

    def __init__(self, code: str):
        super().__init__(code)
        self.code = code


class I2pdAdapter:
    """Manage a foreground, explicitly staged i2pd process."""

    version = "2.60.0"
    revision = "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e"

    def __init__(self, cache: Path, run_root: Path, endpoint: EndpointDescription, repo_root: Path):
        self.cache = cache.resolve()
        self.run_root = run_root.resolve()
        self.endpoint = endpoint
        self.repo_root = repo_root.resolve()
        self.data_dir = self.run_root / "reference-data"
        self.config_dir = self.run_root / "config"
        self.process: BoundedProcess | None = None
        self.metadata: CacheMetadata | None = None
        self.configuration_sha256 = ""

    def _prefix(self) -> list[str]:
        return [] if os.geteuid() == 0 else ["sudo", "-n"]

    def _inside_run_root(self, path: Path) -> bool:
        resolved = path.resolve()
        return resolved == self.run_root or self.run_root in resolved.parents

    def prepare(self) -> Path:
        try:
            self.metadata = parse_metadata(self.cache / "build-metadata.txt", selected_reference="i2pd", cache_root=self.cache)
        except MetadataError as exc:
            raise I2pdError("invalid-reference-metadata") from exc
        if self.metadata.source_revision != self.revision:
            raise I2pdError("reference-revision-mismatch")
        binary = self.cache / self.metadata.launcher
        if binary != (self.cache / "bin/i2pd").resolve() or not binary.is_file() or not os.access(binary, os.X_OK):
            raise I2pdError("invalid-staged-launcher")
        self.data_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
        self.config_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
        template = self.repo_root / "tests/integration/ntcp2/config/i2pd/i2pd.conf.template"
        rendered = template.read_text(encoding="utf-8")
        replacements = {
            "@DATA_DIR@": str(self.data_dir),
            "@ADDRESS4@": self.endpoint.local_address if self.endpoint.address_family == "ipv4" else "",
            "@ADDRESS6@": self.endpoint.local_address if self.endpoint.address_family == "ipv6" else "",
            "@LOCAL_PORT@": str(self.endpoint.local_port),
            "@ADDRESS_FAMILY@": self.endpoint.address_family,
            "@IPV4_ENABLED@": "true" if self.endpoint.address_family == "ipv4" else "false",
            "@IPV6_ENABLED@": "true" if self.endpoint.address_family == "ipv6" else "false",
        }
        for key, value in replacements.items():
            rendered = rendered.replace(key, value)
        if "@" in rendered or self.endpoint.local_address not in rendered:
            raise I2pdError("unrendered-or-wrong-reference-address")
        required = (
            "daemon = false", "notransit = true", "floodfill = false", "netid = 99",
            "[ntcp2]", "enabled = true", "[ssu2]", "enabled = false", "[upnp]",
            "[reseed]", "threshold = 0", f"port = {self.endpoint.local_port}",
        )
        if any(line not in rendered for line in required):
            raise I2pdError("safety-configuration-assertion-failed")
        config = self.config_dir / "i2pd.conf"
        config.write_text(rendered, encoding="utf-8")
        (self.config_dir / "tunnels.conf").write_text(
            (self.repo_root / "tests/integration/ntcp2/config/i2pd/tunnels.conf.template").read_text(encoding="utf-8"),
            encoding="utf-8",
        )
        self.configuration_sha256 = hashlib.sha256(rendered.encode()).hexdigest()
        (self.run_root / "config.sha256").write_text(self.configuration_sha256 + "\n", encoding="ascii")
        return binary

    def start(self) -> None:
        binary = self.prepare()
        command = self._prefix() + ["ip", "netns", "exec", self.endpoint.namespace, str(binary), "--datadir", str(self.data_dir), "--conf", str(self.config_dir / "i2pd.conf")]
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
        source = source.resolve()
        if not self._inside_run_root(source):
            raise I2pdError("peer-router-info-outside-run-root")
        try:
            target = self.data_dir / "netDb" / netdb_filename(source)
        except RouterInfoPathError as exc:
            raise I2pdError("invalid-peer-router-info") from exc
        if not self._inside_run_root(target):
            raise I2pdError("peer-netdb-path-outside-run-root")
        target.parent.mkdir(mode=0o700, exist_ok=True)
        shutil.copyfile(source, target)

    def query_typed_state(self) -> dict[str, int | str]:
        if self.process is None:
            return {"state": "not-started"}
        return {"state": "running" if self.process.snapshot()["running"] else "stopped"}

    def counters(self) -> dict[str, int]:
        snapshot = self.process.snapshot() if self.process is not None else {"running": 0, "exit_code": -1, "forced": 0}
        return {"started": int(self.process is not None), "exited": int(snapshot["running"] == 0), "forced": int(snapshot.get("forced", 0))}

    def stop(self, timeout_seconds: float = 5.0) -> str:
        if self.process is None:
            return "not-started"
        return self.process.stop(timeout_seconds)
