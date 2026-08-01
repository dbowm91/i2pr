"""Non-production i2pr interoperability launcher adapter."""

from __future__ import annotations

import hashlib
import ipaddress
import json
import os
import re
import shutil
from dataclasses import dataclass
from pathlib import Path
from typing import Any

try:
    from .launcher_protocol import LauncherStatusError, parse_status_line
    from .process import BoundedProcess, ProcessError
except ImportError:  # pragma: no cover - direct harness-module execution
    from launcher_protocol import LauncherStatusError, parse_status_line  # type: ignore
    from process import BoundedProcess, ProcessError  # type: ignore

try:
    from .interop_topology import ProcessPlacement, TopologyContractError
except ImportError:  # pragma: no cover - direct harness-module execution
    from interop_topology import ProcessPlacement, TopologyContractError  # type: ignore


def _legacy_privileged_prefix() -> list[str]:
    """Build the legacy privileged ``ip netns exec <namespace>`` prefix.

    Used only when a caller passes a ``namespace`` string instead of a
    ``ProcessPlacement``. New code must pass ``ProcessPlacement`` directly.
    """

    return [] if os.geteuid() == 0 else ["sudo", "-n"]


class I2prAdapter:
    """Invoke the dedicated launcher, never the normal daemon.

    The adapter executes inside the topology's sealed execution context
    (``ProcessPlacement``). For backwards compatibility, callers may pass a
    ``namespace`` string, which the adapter interprets as the legacy
    privileged ``ip netns exec <namespace>`` placement. New code should
    pass ``placement=`` directly.
    """

    def __init__(
        self,
        repo_root: Path,
        run_root: Path,
        namespace: str | None = None,
        *,
        placement: ProcessPlacement | None = None,
    ):
        if placement is None:
            if namespace is None:
                raise RuntimeError("i2pr-adapter-needs-placement-or-namespace")
            placement = ProcessPlacement(
                topology_kind="privileged-dual-netns-veth",
                actor="i2pr",
                command_prefix=tuple(_legacy_privileged_prefix() + ["ip", "netns", "exec", namespace]),
            )
        elif namespace is not None:
            raise RuntimeError("i2pr-adapter-placement-and-namespace-mutually-exclusive")
        if placement.actor != "i2pr":
            raise RuntimeError("i2pr-adplacement-must-be-i2pr")
        self.repo_root = repo_root
        self.run_root = run_root
        self.placement = placement
        self.process: BoundedProcess | None = None
        self.preparation_process: BoundedProcess | None = None
        self.mode: str | None = None
        self.last_status: dict[str, object] | None = None

    @dataclass(frozen=True)
    class PreparedI2prState:
        router_info_path: Path
        router_info_sha256: str
        router_hash_sha256: str
        ntcp2_address_count: int

    def prepare_state(
        self,
        *,
        local_address: str,
        local_port: int,
        network_id: int = 99,
        deterministic_seed: int | None = None,
        state_dir: str = "state",
    ) -> PreparedI2prState:
        """Prepare and validate i2pr state without opening a socket."""

        address = ipaddress.ip_address(local_address)
        if address.version == 4:
            synthetic = ipaddress.ip_network("192.0.2.0/24")
        else:
            synthetic = ipaddress.ip_network("2001:db8:36::/64")
        if address not in synthetic or not 1 <= local_port <= 65535 or network_id != 99:
            raise RuntimeError("prepare-input-invalid")
        state_path = (self.run_root / state_dir).resolve()
        if not self._inside_run_root(state_path):
            raise RuntimeError("prepare-state-path-invalid")
        command_args = [
            str(self.repo_root / "target" / "debug" / "i2pr-interop"),
            "ntcp2",
            "prepare",
            "--state-dir",
            str(state_path),
            "--local-address",
            str(address),
            "--local-port",
            str(local_port),
            "--network-id",
            str(network_id),
        ]
        if deterministic_seed is not None:
            command_args.extend(["--deterministic-seed", str(deterministic_seed)])
        try:
            command = self.placement.command(command_args)
            process = BoundedProcess(command, self.run_root / "raw" / "i2pr-prepare.log")
            process.start()
        except (OSError, ProcessError, TopologyContractError) as exc:
            raise RuntimeError("i2pr-preparation-start-failed") from exc
        self.preparation_process = process
        try:
            record = process.wait_for_record(_parse_preparation, 30.0)
            if process.wait_for_exit(30.0) != 0:
                raise RuntimeError("i2pr-state-preparation-failed")
        except ProcessError as exc:
            raise RuntimeError("i2pr-state-preparation-failed") from exc
        if record["result"] != "prepared":
            raise RuntimeError(str(record.get("reason_code", "i2pr-state-preparation-failed")))
        router_info_path = state_path / "router.info"
        if not router_info_path.is_file():
            raise RuntimeError("i2pr-router-info-missing")
        try:
            from .router_info import strict_validate_router_info
        except ImportError:  # pragma: no cover
            from router_info import strict_validate_router_info  # type: ignore
        try:
            strict_validate_router_info(
                router_info_path,
                expected_address=str(address),
                expected_port=local_port,
                repo_root=self.repo_root,
            )
        except (OSError, ValueError) as exc:
            raise RuntimeError("i2pr-router-info-validation-failed") from exc
        info_digest = hashlib.sha256(router_info_path.read_bytes()).hexdigest()
        router_hash = str(record["router_hash_sha256"])
        if not _HEX64.fullmatch(router_hash):
            raise RuntimeError("i2pr-router-hash-invalid")
        if info_digest != record["router_info_sha256"]:
            raise RuntimeError("i2pr-preparation-record-invalid")
        return self.PreparedI2prState(
            router_info_path=router_info_path,
            router_info_sha256=info_digest,
            router_hash_sha256=router_hash,
            ntcp2_address_count=int(record["ntcp2_address_count"]),
        )

    def start(self, mode: str) -> None:
        if mode not in {"listen", "dial"}:
            raise RuntimeError("invalid-i2pr-mode")
        binary = self.repo_root / "target" / "debug" / "i2pr-interop"
        if not binary.is_file():
            raise RuntimeError("missing-i2pr-interop-launcher")
        try:
            command = self.placement.command(
                [str(binary), "ntcp2", mode, "--scenario-config", str(self.run_root / "scenario.toml")]
            )
        except TopologyContractError as exc:
            raise RuntimeError(exc.code) from exc
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
        ``exchange/router.info`` is an export of the already-prepared state.
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
        if self.process is None and self.preparation_process is None:
            return "not-started"
        results = []
        if self.process is not None:
            results.append(self.process.stop(timeout_seconds))
        if self.preparation_process is not None and self.preparation_process.process is not None:
            results.append(self.preparation_process.stop(timeout_seconds))
        return "forced" if "forced" in results else "clean"

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


_HEX64 = re.compile(r"^[0-9a-f]{64}$")


def _parse_preparation(line: str) -> dict[str, object] | None:
    try:
        value = json.loads(line)
    except (json.JSONDecodeError, UnicodeError):
        return None
    if not isinstance(value, dict) or value.get("schema") != "i2pr-interop-state-prepared-v1":
        return None
    if value.get("result") == "prepared":
        required = {"schema", "result", "router_hash_sha256", "router_info_sha256", "ntcp2_address_count"}
        if set(value) != required:
            return None
        if not _HEX64.fullmatch(str(value["router_hash_sha256"])):
            return None
        if not _HEX64.fullmatch(str(value["router_info_sha256"])):
            return None
        if value["ntcp2_address_count"] != 1:
            return None
    elif value.get("result") != "rejected":
        return None
    return value
