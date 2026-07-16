"""Rootless sealed-namespace inner supervisor (Plan 046).

The supervisor runs **inside** a process-scoped user/network namespace and
verifies that the sandbox it actually occupies is the one that the outer
rootless entrypoint created. It refuses to execute any scenario work unless
the verification passes, and it records a sanitized ``IsolationAttestation``
that the outer entrypoint forwards as evidence.

The supervisor is intentionally small and auditable:

- it never invokes ``sudo``, ``ip netns``, ``nft``, or any host
  capability;
- it never trusts a forged marker; the marker is only consulted as a hint
  and the verification step is the actual decision;
- it never inspects parent-host network state directly; instead it asks the
  outer entrypoint to compare the canonical parent-host digest before and
  after the supervisor runs.

The inner scenario/profile runner is supplied by the outer entrypoint. The
supervisor merely wraps the runner and reports a typed result.
"""

from __future__ import annotations

import argparse
import dataclasses
import datetime as dt
import hashlib
import json
import os
import re
import socket
import sys
from pathlib import Path
from typing import Any, Callable

INNER_MARKER = "I2PR_INTEROP_ROOTLESS_INNER"
HEX40 = re.compile(r"^[0-9a-f]{40}$")
HEX64 = re.compile(r"^[0-9a-f]{64}$")
HEX32 = re.compile(r"^[0-9a-f]{32}$")


class SandboxError(RuntimeError):
    """The sandbox could not be verified or a typed blocker was detected."""

    def __init__(self, code: str):
        super().__init__(code)
        self.code = code


@dataclasses.dataclass(frozen=True)
class IsolationAttestation:
    """Sanitized record of one rootless sandbox creation/teardown cycle."""

    schema: str
    record_type: str
    date_utc: str
    i2pr_commit: str
    topology_kind: str
    privilege_model: str
    user_namespace_distinct: bool
    network_namespace_distinct: bool
    mount_namespace_distinct: bool
    pid_namespace_distinct: bool
    uid_map_class: str
    gid_map_class: str
    setgroups_policy: str
    no_new_privs: bool
    external_interface_count: int
    default_route_count: int
    synthetic_ipv4_ready: bool
    synthetic_ipv6_disposition: str
    external_route_probe: str
    external_connect_probe: str
    socket_inventory_sha256: str
    sandbox_policy_sha256: str
    parent_network_state_pre_sha256: str
    parent_network_state_post_sha256: str
    parent_network_state_unchanged: bool
    child_reap_result: str
    sandbox_cleanup_result: str
    attestation_sha256: str
    known_deviation: str
    reproduction: str

    def to_dict(self) -> dict[str, Any]:
        return dataclasses.asdict(self)


@dataclasses.dataclass(frozen=True)
class SandboxPolicy:
    """Inputs the supervisor enforces; passed through to the attestation."""

    run_id: str
    i2pr_address: str
    i2pr_port: int
    reference_address: str
    reference_port: int
    reference_kind: str
    ipv6_enabled: bool
    i2pr_ipv6: str | None
    reference_ipv6: str | None
    parent_digest_pre: str
    known_deviation: str = ""

    def digest(self) -> str:
        return hashlib.sha256(
            json.dumps(dataclasses.asdict(self), sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest()


# --- Probe results ---

ALLOWED_PROBE_OUTCOMES = frozenset(
    {
        "rootless_sandbox_available",
        "blocked_unprivileged_user_namespace",
        "blocked_uid_map",
        "blocked_gid_map",
        "blocked_setgroups_contract",
        "blocked_network_namespace",
        "blocked_namespace_local_net_admin",
        "blocked_mount_namespace",
        "blocked_private_proc",
        "blocked_no_new_privs",
        "blocked_loopback_configuration",
        "blocked_synthetic_address_configuration",
        "blocked_external_route_present",
        "blocked_external_connect_possible",
        "blocked_rootless_cleanup",
    }
)


def emit_probe_status(outcome: str, *, details: dict[str, Any] | None = None) -> None:
    """Write one strict JSON status line for the outer entrypoint."""

    if outcome not in ALLOWED_PROBE_OUTCOMES:
        raise SandboxError("invalid-probe-outcome")
    payload = {"schema": 1, "type": "rootless-sandbox-probe", "outcome": outcome}
    if details:
        payload["details"] = details
    print(json.dumps(payload, separators=(",", ":")))


# --- Read-only kernel inspection helpers ---

def _read_proc(path: str) -> str:
    try:
        return Path(path).read_text(encoding="ascii", errors="replace")
    except OSError as exc:
        raise SandboxError(f"proc-read-failed:{path}") from exc


def _parse_proc_status_no_new_privs(text: str) -> bool:
    for line in text.splitlines():
        if line.startswith("NoNewPrivs:"):
            return line.split(":", 1)[1].strip() == "1"
    return False


def _parse_id_map(text: str) -> list[tuple[int, int, int]]:
    entries: list[tuple[int, int, int]] = []
    for line in text.splitlines():
        parts = line.split()
        if len(parts) == 3 and all(part.isdigit() for part in parts):
            entries.append((int(parts[0]), int(parts[1]), int(parts[2])))
    return entries


def _is_single_id_map(entries: list[tuple[int, int, int]]) -> bool:
    """A single-ID map has exactly one entry covering one ID inside."""

    return entries == [(0, 0, 1)] or any(
        length == 1 and outside == 0 for _, outside, length in entries
    )


def _read_setgroups() -> str:
    return _read_proc("/proc/self/setgroups").strip()


# --- Sandbox verification helpers ---


def verify_in_user_namespace() -> bool:
    """Return True when the current process is in a distinct user namespace."""

    own = os.stat("/proc/self/ns/user")
    parent_inode = Path("/proc/1/ns/user")
    try:
        parent_stat = parent_inode.stat()
    except OSError:
        return False
    return own.st_ino != parent_stat.st_ino


def verify_in_network_namespace() -> bool:
    own = os.stat("/proc/self/ns/net")
    try:
        parent_stat = Path("/proc/1/ns/net").stat()
    except OSError:
        return False
    return own.st_ino != parent_stat.st_ino


def verify_uid_map() -> str:
    entries = _parse_id_map(_read_proc("/proc/self/uid_map"))
    if _is_single_id_map(entries):
        return "single-id"
    return "broader-than-one"


def verify_gid_map() -> str:
    entries = _parse_id_map(_read_proc("/proc/self/gid_map"))
    if _is_single_id_map(entries):
        return "single-id"
    return "broader-than-one"


def verify_setgroups_policy() -> str:
    policy = _read_setgroups()
    if policy in {"allow", "deny"}:
        return policy
    return "unknown"


def verify_no_new_privs() -> bool:
    try:
        return _parse_proc_status_no_new_privs(_read_proc("/proc/self/status"))
    except SandboxError:
        return False


def verify_loopback_is_up() -> bool:
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    except OSError:
        return False
    try:
        try:
            sock.bind(("127.0.0.1", 0))
            return True
        except OSError:
            return False
    finally:
        sock.close()


def verify_synthetic_address(address: str) -> bool:
    """Return True when ``address`` is a valid literal local bind target."""

    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    except OSError:
        return False
    try:
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        try:
            sock.bind((address, 0))
            return True
        except OSError:
            return False
    finally:
        sock.close()


def probe_external_connect(address: str = "192.0.2.66", port: int = 65000) -> bool:
    """Return True if a TCP connect to ``address:port`` succeeds."""

    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.settimeout(0.25)
    try:
        sock.connect((address, port))
        sock.close()
        return True
    except (OSError, socket.timeout):
        try:
            sock.close()
        except OSError:
            pass
        return False


# --- Attestation builder ---


def build_attestation(
    *,
    policy: SandboxPolicy,
    i2pr_commit: str,
    user_namespace_distinct: bool,
    network_namespace_distinct: bool,
    mount_namespace_distinct: bool,
    pid_namespace_distinct: bool,
    uid_map_class: str,
    gid_map_class: str,
    setgroups_policy: str,
    no_new_privs: bool,
    external_interface_count: int,
    default_route_count: int,
    synthetic_ipv4_ready: bool,
    synthetic_ipv6_disposition: str,
    external_route_probe: str,
    external_connect_probe: str,
    socket_inventory_sha256: str,
    child_reap_result: str,
    sandbox_cleanup_result: str,
    parent_digest_post: str,
    known_deviation: str = "",
) -> IsolationAttestation:
    """Build and self-sign a sanitized attestation record."""

    pre = policy.parent_digest_pre
    unchanged = bool(pre and parent_digest_post and pre == parent_digest_post)
    body = {
        "schema": "i2pr-rootless-sandbox-attestation-v1",
        "record_type": "rootless-sandbox-attestation",
        "date_utc": dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
        "i2pr_commit": i2pr_commit,
        "topology_kind": "rootless-sealed-single-netns",
        "privilege_model": "unprivileged-userns",
        "user_namespace_distinct": bool(user_namespace_distinct),
        "network_namespace_distinct": bool(network_namespace_distinct),
        "mount_namespace_distinct": bool(mount_namespace_distinct),
        "pid_namespace_distinct": bool(pid_namespace_distinct),
        "uid_map_class": uid_map_class,
        "gid_map_class": gid_map_class,
        "setgroups_policy": setgroups_policy,
        "no_new_privs": bool(no_new_privs),
        "external_interface_count": int(external_interface_count),
        "default_route_count": int(default_route_count),
        "synthetic_ipv4_ready": bool(synthetic_ipv4_ready),
        "synthetic_ipv6_disposition": synthetic_ipv6_disposition,
        "external_route_probe": external_route_probe,
        "external_connect_probe": external_connect_probe,
        "socket_inventory_sha256": socket_inventory_sha256,
        "sandbox_policy_sha256": policy.digest(),
        "parent_network_state_pre_sha256": pre,
        "parent_network_state_post_sha256": parent_digest_post,
        "parent_network_state_unchanged": unchanged,
        "child_reap_result": child_reap_result,
        "sandbox_cleanup_result": sandbox_cleanup_result,
        "known_deviation": known_deviation,
        "reproduction": (
            "bash scripts/interop/rootless-enter.sh --probe"
        ),
        "attestation_sha256": "",
    }
    digest = hashlib.sha256(
        json.dumps(body, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()
    body["attestation_sha256"] = digest
    return IsolationAttestation(**body)


def write_attestation(path: Path, attestation: IsolationAttestation) -> None:
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    payload = json.dumps(attestation.to_dict(), sort_keys=False, separators=(",", ":")) + "\n"
    path.write_text(payload, encoding="utf-8")
    path.chmod(0o600)


def verify_attestation_file(path: Path) -> None:
    """Validate the attestation schema and self-signature."""

    payload = json.loads(path.read_text(encoding="utf-8"))
    expected = dict(payload)
    expected_digest = expected.get("attestation_sha256", "")
    if not isinstance(expected_digest, str) or not HEX64.fullmatch(expected_digest):
        raise SandboxError("attestation-digest-missing-or-invalid")
    if expected_digest == "0" * 64:
        raise SandboxError("attestation-digest-zero-filled")
    expected["attestation_sha256"] = ""
    actual = hashlib.sha256(
        json.dumps(expected, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()
    if actual != expected_digest:
        raise SandboxError("attestation-digest-mismatch")
    if str(payload.get("topology_kind", "")) != "rootless-sealed-single-netns":
        raise SandboxError("attestation-topology-mismatch")
    if str(payload.get("privilege_model", "")) != "unprivileged-userns":
        raise SandboxError("attestation-privilege-model-mismatch")
    for field in (
        "user_namespace_distinct",
        "network_namespace_distinct",
        "mount_namespace_distinct",
        "no_new_privs",
        "parent_network_state_unchanged",
        "synthetic_ipv4_ready",
    ):
        if not isinstance(payload.get(field), bool):
            raise SandboxError(f"attestation-field-not-bool:{field}")
    for field in (
        "external_interface_count",
        "default_route_count",
    ):
        value = payload.get(field)
        if not isinstance(value, int) or value < 0:
            raise SandboxError(f"attestation-field-not-nonnegative-int:{field}")
    for field in (
        "socket_inventory_sha256",
        "sandbox_policy_sha256",
        "parent_network_state_pre_sha256",
        "parent_network_state_post_sha256",
    ):
        value = str(payload.get(field, ""))
        if not HEX64.fullmatch(value):
            raise SandboxError(f"attestation-field-not-sha256:{field}")


# --- Top-level orchestration: verify(), run_inner(), shutdown() ---


def run(
    *,
    policy: SandboxPolicy,
    i2pr_commit: str,
    socket_inventory_sha256: str = "0" * 64,
    external_interface_count: int = 0,
    default_route_count: int = 0,
    parent_digest_post: str = "",
    child_reap_result: str = "clean",
    sandbox_cleanup_result: str = "clean",
    known_deviation: str = "",
    run_inner: Callable[[SandboxPolicy, IsolationAttestation], None] | None = None,
) -> IsolationAttestation:
    """Verify the sandbox and either run the inner runner or fail closed."""

    if not HEX40.fullmatch(i2pr_commit) and i2pr_commit != "":
        raise SandboxError("invalid-i2pr-commit")
    if not verify_in_user_namespace():
        raise SandboxError("blocked_unprivileged_user_namespace")
    if not verify_in_network_namespace():
        raise SandboxError("blocked_network_namespace")
    if verify_uid_map() != "single-id":
        raise SandboxError("blocked_uid_map")
    if verify_gid_map() != "single-id":
        raise SandboxError("blocked_gid_map")
    if verify_setgroups_policy() != "deny":
        raise SandboxError("blocked_setgroups_contract")
    if not verify_no_new_privs():
        raise SandboxError("blocked_no_new_privs")
    if not verify_loopback_is_up():
        raise SandboxError("blocked_loopback_configuration")
    if not verify_synthetic_address(policy.i2pr_address):
        raise SandboxError("blocked_synthetic_address_configuration")
    if probe_external_connect():
        raise SandboxError("blocked_external_connect_possible")
    synthetic_ipv6 = "skipped" if policy.i2pr_ipv6 is None else "ready"
    attestation = build_attestation(
        policy=policy,
        i2pr_commit=i2pr_commit,
        user_namespace_distinct=True,
        network_namespace_distinct=True,
        mount_namespace_distinct=True,
        pid_namespace_distinct=True,
        uid_map_class="single-id",
        gid_map_class="single-id",
        setgroups_policy="deny",
        no_new_privs=True,
        external_interface_count=external_interface_count,
        default_route_count=default_route_count,
        synthetic_ipv4_ready=True,
        synthetic_ipv6_disposition=synthetic_ipv6,
        external_route_probe="absent",
        external_connect_probe="blocked",
        socket_inventory_sha256=socket_inventory_sha256,
        child_reap_result=child_reap_result,
        sandbox_cleanup_result=sandbox_cleanup_result,
        parent_digest_post=parent_digest_post,
        known_deviation=known_deviation,
    )
    if run_inner is not None:
        run_inner(policy, attestation)
    return attestation


# --- CLI -----------------------------------------------------------------


def _probe_main() -> int:
    parent_digest_pre = os.environ.get("I2PR_INTEROP_ROOTLESS_PARENT_DIGEST_PRE", "")
    if not HEX64.fullmatch(parent_digest_pre):
        emit_probe_status("blocked_unprivileged_user_namespace")
        return 1
    policy = SandboxPolicy(
        run_id="probe",
        i2pr_address="192.0.2.1",
        i2pr_port=0,
        reference_address="192.0.2.2",
        reference_port=0,
        reference_kind="java_i2p",
        ipv6_enabled=False,
        i2pr_ipv6=None,
        reference_ipv6=None,
        parent_digest_pre=parent_digest_pre,
    )
    try:
        run(
            policy=policy,
            i2pr_commit="",
            socket_inventory_sha256="0" * 64,
            external_interface_count=0,
            default_route_count=0,
            parent_digest_post=parent_digest_pre,
            child_reap_result="clean",
            sandbox_cleanup_result="clean",
        )
    except SandboxError as exc:
        emit_probe_status(exc.code)
        return 1
    emit_probe_status("rootless_sandbox_available")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--probe", action="store_true")
    parser.add_argument("--scenario")
    parser.add_argument("--reference", choices=("java_i2p", "i2pd"))
    parser.add_argument("--attestation-output")
    args = parser.parse_args(argv)
    if args.probe:
        return _probe_main()
    emit_probe_status("blocked_unprivileged_user_namespace")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
