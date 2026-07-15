"""Pinned Java I2P process adapter for a disposable namespace run."""

from __future__ import annotations

import hashlib
import os
import shutil
from pathlib import Path

try:
    from .metadata import CacheMetadata, MetadataError, parse_metadata
    from .config_contract import ConfigurationContractError, assert_java_private_configuration
    from .process import BoundedProcess, ProcessError
    from .router_info import RouterInfoPathError, netdb_filename
    from .topology import EndpointDescription
except ImportError:  # unittest discovery loads this directory as a flat path.
    from metadata import CacheMetadata, MetadataError, parse_metadata  # type: ignore
    from config_contract import ConfigurationContractError, assert_java_private_configuration  # type: ignore
    from process import BoundedProcess, ProcessError  # type: ignore
    from router_info import RouterInfoPathError, netdb_filename  # type: ignore
    from topology import EndpointDescription  # type: ignore


class JavaI2pError(RuntimeError):
    """A Java I2P adapter precondition or lifecycle operation failed."""

    def __init__(self, code: str):
        super().__init__(code)
        self.code = code


class JavaI2pAdapter:
    """Manage one disposable Java I2P instance inside a reference namespace."""

    version = "2.12.0"
    revision = "2800040deee9bb376567b671ef2e9c34cf3e30b6"
    authenticated_phrases = ("NTCP2 connection established", "Established NTCP2 connection")

    def __init__(self, cache: Path, run_root: Path, endpoint: EndpointDescription, repo_root: Path):
        self.cache = cache.resolve()
        self.run_root = run_root.resolve()
        self.endpoint = endpoint
        self.repo_root = repo_root.resolve()
        self.runtime_dir = self.run_root / "reference-runtime"
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
        metadata_path = self.cache / "build-metadata.txt"
        try:
            self.metadata = parse_metadata(metadata_path, selected_reference="java_i2p", cache_root=self.cache)
        except MetadataError as exc:
            raise JavaI2pError("invalid-reference-metadata") from exc
        if self.metadata.source_revision != self.revision:
            raise JavaI2pError("reference-revision-mismatch")
        launcher = self.runtime_dir / self.metadata.launcher
        if not self.runtime_dir.exists():
            shutil.copytree(self.cache, self.runtime_dir, ignore=shutil.ignore_patterns("build-metadata.txt"))
        launcher = self.runtime_dir / self.metadata.launcher
        if not self._inside_run_root(launcher) or not launcher.is_file() or not os.access(launcher, os.X_OK):
            raise JavaI2pError("invalid-staged-launcher")
        self.data_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
        self.config_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
        template = self.repo_root / "tests/integration/ntcp2/config/java-i2p/router.config.template"
        rendered = template.read_text(encoding="utf-8")
        replacements = {
            "@DATA_DIR@": str(self.data_dir),
            "@CONFIG_DIR@": str(self.config_dir),
            "@NTCP2_ADDRESS@": self.endpoint.local_address,
            "@NTCP2_PORT@": str(self.endpoint.local_port),
            "@ADDRESS_FAMILY@": self.endpoint.address_family,
            "@ADDRESS_FAMILY_IPV6@": "true" if self.endpoint.address_family == "ipv6" else "false",
        }
        for key, value in replacements.items():
            rendered = rendered.replace(key, value)
        if "@" in rendered or self.endpoint.local_address not in rendered:
            raise JavaI2pError("unrendered-or-wrong-reference-address")
        try:
            assert_java_private_configuration(
                rendered,
                address=self.endpoint.local_address,
                port=self.endpoint.local_port,
                network_id=99,
                ipv6=self.endpoint.address_family == "ipv6",
            )
        except ConfigurationContractError as exc:
            raise JavaI2pError("safety-configuration-assertion-failed") from exc
        (self.config_dir / "router.config").write_text(rendered, encoding="utf-8")
        clients = (self.repo_root / "tests/integration/ntcp2/config/java-i2p/clients.config.template").read_text(encoding="utf-8").replace("@DATA_DIR@", str(self.data_dir))
        (self.config_dir / "clients.config").write_text(clients, encoding="utf-8")
        self.configuration_sha256 = hashlib.sha256(rendered.encode()).hexdigest()
        (self.run_root / "config.sha256").write_text(self.configuration_sha256 + "\n", encoding="ascii")
        return launcher

    def start(self) -> None:
        launcher = self.prepare()
        command = self._prefix() + ["ip", "netns", "exec", self.endpoint.namespace, str(launcher)]
        environment = os.environ.copy()
        environment["JAVA_TOOL_OPTIONS"] = f"-Di2p.dir.base={self.runtime_dir} -Di2p.dir.config={self.config_dir} -Di2p.dir.router={self.data_dir} -Xmx256m"
        self.process = BoundedProcess(command, self.run_root / "raw" / "java-i2p.log", environment=environment)
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
        candidates = (self.runtime_dir / "router.info", self.runtime_dir / "router.info.su3")
        for candidate in candidates:
            if candidate.is_file():
                target = self.run_root / "exchange" / "java-router.info"
                target.parent.mkdir(mode=0o700, exist_ok=True)
                shutil.copyfile(candidate, target)
                return target
        raise JavaI2pError("router-info-not-produced")

    def import_peer_router_info(self, source: Path) -> None:
        source = source.resolve()
        if not self._inside_run_root(source):
            raise JavaI2pError("peer-router-info-outside-run-root")
        try:
            target = self.data_dir / "netDb" / netdb_filename(source)
        except RouterInfoPathError as exc:
            raise JavaI2pError("invalid-peer-router-info") from exc
        if not self._inside_run_root(target):
            raise JavaI2pError("peer-netdb-path-outside-run-root")
        target.parent.mkdir(mode=0o700, exist_ok=True)
        shutil.copyfile(source, target)

    def query_typed_state(self) -> dict[str, int | str]:
        if self.process is None:
            return {"state": "not-started"}
        return {"state": "running" if self.process.snapshot()["running"] else "stopped"}

    def authenticated_observation(self) -> str:
        if self.process is None:
            return "not-started"
        return "authenticated" if self.process.observed_phrase(self.authenticated_phrases) else "not-observed"

    def counters(self) -> dict[str, int]:
        snapshot = self.process.snapshot() if self.process is not None else {"running": 0, "exit_code": -1, "forced": 0}
        return {"started": int(self.process is not None), "exited": int(snapshot["running"] == 0), "forced": int(snapshot.get("forced", 0))}

    def stop(self, timeout_seconds: float = 5.0) -> str:
        if self.process is None:
            return "not-started"
        return self.process.stop(timeout_seconds)
