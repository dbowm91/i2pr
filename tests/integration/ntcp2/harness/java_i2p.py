"""Pinned Java I2P process adapter for a disposable namespace run."""

from __future__ import annotations

import hashlib
import os
import platform
import shutil
import time
import zipfile
from pathlib import Path

try:
    from .metadata import CacheMetadata, MetadataError, parse_metadata
    from .config_contract import ConfigurationContractError, assert_java_private_configuration
    from .process import BoundedProcess, ProcessError
    from .router_info import RouterInfoPathError, netdb_filename
    from .topology import EndpointDescription
    from .interop_topology import ProcessPlacement, TopologyContractError
except ImportError:  # unittest discovery loads this directory as a flat path.
    from metadata import CacheMetadata, MetadataError, parse_metadata  # type: ignore
    from config_contract import ConfigurationContractError, assert_java_private_configuration  # type: ignore
    from process import BoundedProcess, ProcessError  # type: ignore
    from router_info import RouterInfoPathError, netdb_filename  # type: ignore
    from topology import EndpointDescription  # type: ignore
    from interop_topology import ProcessPlacement, TopologyContractError  # type: ignore


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

    def __init__(
        self,
        cache: Path,
        run_root: Path,
        endpoint: EndpointDescription,
        repo_root: Path,
        *,
        shared_data_dir: Path | None = None,
        placement: ProcessPlacement | None = None,
    ):
        self.cache = cache.resolve()
        self.run_root = run_root.resolve()
        self.endpoint = endpoint
        self.repo_root = repo_root.resolve()
        self.runtime_dir = self.run_root / "reference-runtime"
        # Plan 045 D1: a Plan 044 mixed-runner created a fresh
        # ``reference-data`` directory for the live phase, losing the
        # identity and RouterInfo produced by the generation pass. The
        # default keeps the historical isolation; an explicit
        # ``shared_data_dir`` lets a pair of adapters share one disposable
        # identity root across the ``-gen`` and live phases.
        if shared_data_dir is None:
            self.data_dir = self.run_root / "reference-data"
        else:
            self.data_dir = shared_data_dir.resolve()
            # Plan 045 D1: the ``-gen`` and live phases must share a single
            # data directory inside the dispatcher's run_root. Accept any
            # path that is, or is inside, the live run_root, and accept any
            # sibling path that lives under the same top-level run_root.
            live_under_run_root = (self.data_dir == self.run_root or self.run_root in self.data_dir.parents)
            sibling_under_run_root = bool(self.run_root.parents) and (
                self.data_dir in self.run_root.parents
                or any(self.data_dir == (p / self.data_dir.name) for p in self.run_root.parents)
            )
            if not (live_under_run_root or sibling_under_run_root):
                raise JavaI2pError("shared-data-dir-outside-run-root")
        # Java I2P only honours ``i2p.dir.config`` and ``i2p.dir.base``; it
        # always writes its key store, ``router.info``, eventlog, and logger
        # under the working directory computed from ``i2p.dir.config``. We
        # therefore point ``i2p.dir.config`` at ``data_dir`` so the Plan 045
        # D1 shared directory produces both the live config and the
        # RouterInfo export.
        self.config_dir = self.data_dir
        self.placement: ProcessPlacement | None = placement
        self.process: BoundedProcess | None = None
        self.metadata: CacheMetadata | None = None
        self.configuration_sha256 = ""

    def _prefix(self) -> list[str]:
        """Backwards-compatible legacy prefix construction.

        Used only when an explicit ``placement`` was not supplied to the
        adapter. New callers must pass ``placement`` through ``select_topology``.
        """

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
        self._rewrite_host_paths()
        # The cached tree is read-only because cp preserves the build tree
        # modes. The router writes its eventlog, logs, and key store under
        # the staged runtime dir, so widen the modes inside the namespace.
        # Additionally, the JVM extracts bundled native libraries
        # (libjbigi.so, libjcpuid.so) at first start with default umask
        # modes that strip execute permission, so always re-walk regardless
        # of whether the staged tree existed before this call.
        for root, dirs, files in os.walk(self.runtime_dir):
            for entry in dirs:
                (Path(root) / entry).chmod(0o700)
            for entry in files:
                entry_path = Path(root) / entry
                if entry_path.suffix in {".jar", ".war"}:
                    entry_path.chmod(0o700)
                else:
                    entry_path.chmod(0o755)
        # The cached "tmp" directory is read-only because cp preserves the
        # build tree modes. The launcher writes the router pid file there,
        # so ensure it is writable inside the namespace.
        for tmp_like in (self.runtime_dir / "tmp",):
            if tmp_like.exists():
                tmp_like.chmod(0o700)
        launcher = self.runtime_dir / self.metadata.launcher
        if not self._inside_run_root(launcher) or not launcher.is_file() or not os.access(launcher, os.X_OK):
            raise JavaI2pError("invalid-staged-launcher")
        (self.runtime_dir / "tmp").mkdir(mode=0o700, parents=True, exist_ok=True)
        os.chmod(self.runtime_dir / "tmp", 0o700)
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
        # The JVM extracts bundled native libraries (libjbigi.so,
        # libjcpuid.so) from jar resources at first start with default umask
        # modes that strip the executable bit, breaking later dlopen calls.
        # Pre-extract the bundled shared objects into the staged runtime
        # tree with executable permissions so dlopen picks them up rather
        # than re-extracting them at startup.
        self._extract_native_libraries()
        # The Java router uses ``router.ping`` to detect another running
        # instance (it aborts startup when a fresh ping is observed within
        # 60 seconds). Plan 045 D1 shares the data directory between the
        # ref-gen and live phases, so the prior run's ping must be removed
        # before launching a new JVM or the new process will refuse to
        # bind NTCP2.
        stale_ping = self.data_dir / "router.ping"
        try:
            stale_ping.unlink()
        except FileNotFoundError:
            pass
        # The Java router appends to ``eventlog.txt`` indefinitely. Plan 045
        # D1 reuses the same data directory between the ref-gen and live
        # phases, so the prior run's ``crashed`` line would fail the live
        # phase's readiness check. Truncate the eventlog here so each phase
        # starts with an empty log.
        eventlog = self.data_dir / "eventlog.txt"
        try:
            eventlog.unlink()
        except FileNotFoundError:
            pass
        if self.placement is None:
            command = self._prefix() + ["ip", "netns", "exec", self.endpoint.namespace, str(launcher)]
        else:
            try:
                command = self.placement.command([str(launcher)])
            except TopologyContractError as exc:
                raise JavaI2pError(exc.code) from exc
        environment = os.environ.copy()
        environment["I2P"] = str(self.runtime_dir)
        environment["JAVA_TOOL_OPTIONS"] = f"-Di2p.dir.base={self.runtime_dir} -Di2p.dir.config={self.config_dir} -Di2p.dir.router={self.data_dir} -Xmx512m"
        self.process = BoundedProcess(command, self.run_root / "raw" / "java-i2p.log", environment=environment)
        try:
            self.process.start()
        except OSError as exc:
            raise JavaI2pError("process-start-failed") from exc

    def wait_ready(self, timeout_seconds: float = 240.0) -> None:
        if self.process is None:
            raise JavaI2pError("not-started")
        try:
            self.process.wait_ready(("Starting I2P ",), timeout_seconds)
        except ProcessError:
            pass
        try:
            self._wait_for_eventlog_started(timeout_seconds)
        except ProcessError as exc:
            raise JavaI2pError(exc.code) from exc
        # The JVM extracts bundled native libraries (libjbigi.so,
        # libjcpuid.so) into the staged runtime tree with default umask
        # modes that strip the executable bit. Re-walk the tree after the
        # router has reached the started event so newly extracted shared
        # objects become executable for any subsequent dlopen.
        self._widen_native_lib_permissions()

    def _widen_native_lib_permissions(self) -> None:
        for root, dirs, files in os.walk(self.runtime_dir):
            for entry in dirs:
                (Path(root) / entry).chmod(0o700)
            for entry in files:
                entry_path = Path(root) / entry
                if entry_path.suffix in {".jar", ".war"}:
                    entry_path.chmod(0o700)
                else:
                    entry_path.chmod(0o755)

    # The JVM extracts bundled native libraries (libjbigi.so,
    # libjcpuid.so) from ``jbigi.jar``/``jcpuid.jar`` at first start with
    # default umask modes that strip the executable bit, breaking later
    # dlopen calls. Pre-extract the bundled shared objects into the staged
    # runtime tree with executable permissions so dlopen picks them up
    # rather than re-extracting them at startup.
    _JBIGI_CANDIDATES_X86_64 = (
        "libjbigi-linux-zen2_64.so",
        "libjbigi-linux-skylake_64.so",
        "libjbigi-linux-coreibwl_64.so",
        "libjbigi-linux-coreihwl_64.so",
        "libjbigi-linux-coreisbr_64.so",
        "libjbigi-linux-corei_64.so",
        "libjbigi-linux-core2_64.so",
    )
    _JBIGI_CANDIDATES_X86 = (
        "libjbigi-linux-coreisbr.so",
        "libjbigi-linux-corei.so",
        "libjbigi-linux-core2.so",
        "libjbigi-linux-athlon64.so",
        "libjbigi-linux-athlon.so",
    )
    _JCPUID_CANDIDATES_X86_64 = ("libjcpuid-x86_64-linux.so",)
    _JCPUID_CANDIDATES_X86 = ("libjcpuid-x86-linux.so",)

    def _select_native_library(self, jar_entries: set[str], candidates: tuple[str, ...]) -> str | None:
        for candidate in candidates:
            if candidate in jar_entries:
                return candidate
        return None

    def _rewrite_host_paths(self) -> None:
        cache_root = self.cache.resolve()
        host_marker = str(cache_root)
        runtime_root = self.runtime_dir.resolve()
        replacement = str(runtime_root)
        targets = []
        wrapper_config = self.runtime_dir / "wrapper.config"
        if wrapper_config.is_file():
            targets.append(wrapper_config)
        i2prouter_script = self.runtime_dir / self.metadata.launcher
        if i2prouter_script.is_file():
            targets.append(i2prouter_script)
        for path in targets:
            try:
                text = path.read_text(encoding="utf-8", errors="replace")
            except OSError:
                continue
            if host_marker in text:
                path.write_text(text.replace(host_marker, replacement), encoding="utf-8")

    def _extract_native_libraries(self) -> None:
        machine = platform.machine().lower()
        jbigi_candidates = (
            self._JBIGI_CANDIDATES_X86_64 if machine in {"x86_64", "amd64"} else self._JBIGI_CANDIDATES_X86
        )
        jcpuid_candidates = (
            self._JCPUID_CANDIDATES_X86_64 if machine in {"x86_64", "amd64"} else self._JCPUID_CANDIDATES_X86
        )
        jars = {
            "jbigi": self.runtime_dir / "lib" / "jbigi.jar",
            "jcpuid": self.runtime_dir / "lib" / "jcpuid.jar",
        }
        targets = {
            "jbigi": self.runtime_dir / "libjbigi.so",
            "jcpuid": self.runtime_dir / "libjcpuid.so",
        }
        candidate_lists = {
            "jbigi": jbigi_candidates,
            "jcpuid": jcpuid_candidates,
        }
        for name, jar_path in jars.items():
            if not jar_path.is_file():
                continue
            try:
                with zipfile.ZipFile(jar_path) as zf:
                    entries = set(zf.namelist())
            except (OSError, zipfile.BadZipFile):
                continue
            chosen = self._select_native_library(entries, candidate_lists[name])
            if chosen is None:
                continue
            target = targets[name]
            if target.exists():
                target.unlink()
            with zipfile.ZipFile(jar_path) as zf:
                with zf.open(chosen) as src, target.open("wb") as dst:
                    shutil.copyfileobj(src, dst)
            target.chmod(0o755)

    def _wait_for_eventlog_started(self, timeout_seconds: float) -> None:
        deadline = time.monotonic() + timeout_seconds
        eventlog = self.data_dir / "eventlog.txt"
        keys_file = self.data_dir / "router.keys.dat"
        info_file = self.data_dir / "router.info"
        while time.monotonic() < deadline:
            try:
                if self.process is not None and self.process.process is not None:
                    if self.process.process.poll() is not None:
                        raise ProcessError("java-eventlog-started-timeout")
                if eventlog.is_file() and eventlog.stat().st_size > 0:
                    try:
                        lines = eventlog.read_text(encoding="utf-8", errors="replace").splitlines()
                    except OSError:
                        lines = []
                    started = [line for line in lines if line.endswith(" started 2.12.0-0")]
                    crashed = [line for line in lines if " crashed " in line]
                    if (crashed
                            and (not started or len(crashed) >= len(started))):
                        raise ProcessError("java-eventlog-started-timeout")
                info_complete = False
                if info_file.is_file() and info_file.stat().st_size > 600:
                    try:
                        data = info_file.read_bytes()
                        info_complete = (
                            b"NTCP" in data
                            and b"192.0.2.2" in data
                            and b"45678" in data
                            and b"s=" in data
                        )
                    except OSError:
                        info_complete = False
                if eventlog.is_file() and eventlog.stat().st_size > 0:
                    try:
                        lines = eventlog.read_text(encoding="utf-8", errors="replace").splitlines()
                    except OSError:
                        lines = []
                    started = [line for line in lines if line.endswith(" started 2.12.0-0")]
                    if (started and keys_file.is_file() and info_file.is_file()
                            and info_complete):
                        return
            except OSError:
                pass
            time.sleep(0.25)
        raise ProcessError("java-eventlog-started-timeout")

    def export_router_info(self) -> Path:
        allowed_names = ("router.info", "router.info.su3")
        for name in allowed_names:
            candidate = self.data_dir / name
            live_under_run_root = (self.run_root == candidate or self.run_root in candidate.parents)
            sibling_under_run_root = bool(self.run_root.parents) and (
                self.data_dir == self.run_root.parent
                or self.data_dir in self.run_root.parents
                or any(self.data_dir == (p / self.data_dir.name) for p in self.run_root.parents)
            )
            if not (live_under_run_root or sibling_under_run_root):
                raise JavaI2pError("router-info-outside-run-root")
            if candidate.is_symlink():
                raise JavaI2pError("router-info-symlink-rejected")
            if not candidate.is_file():
                continue
            stat = candidate.stat()
            if stat.st_size == 0:
                raise JavaI2pError("router-info-empty")
            if stat.st_size > 1_048_576:
                raise JavaI2pError("router-info-oversized")
            target = self.run_root / "exchange" / "java-router.info"
            target.parent.mkdir(mode=0o700, exist_ok=True)
            shutil.copyfile(candidate, target)
            if not self._inside_run_root(target):
                raise JavaI2pError("exported-router-info-outside-run-root")
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
