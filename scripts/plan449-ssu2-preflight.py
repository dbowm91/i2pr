#!/usr/bin/env python3
"""Plan 449 offline preflight for an explicitly authorized private SSU2 pair.

The ordinary mode validates a transient owner-supplied inventory, local host
readiness and the exact stock i2pd source/executable. It never starts i2pd,
sends a packet, or prints host addresses, interface names, data paths, or router
identities. --self-test uses only deterministic in-memory fixtures.
"""

from __future__ import annotations

import argparse
import hashlib
import ipaddress
import json
import os
from pathlib import Path
import re
import socket
import stat
import subprocess
import sys

PIN = "635b013a612ff47278ef02acf8580a28e10e26c5"
SCHEMA = "i2pr-plan449-host-inventory-v1"
ROLES = {"i2pr", "i2pd"}
MAX_TIMEOUT_SECONDS = 300


class PreflightError(Exception):
    def __init__(self, code: str) -> None:
        super().__init__(code)
        self.code = code


def validate_inventory(value: object) -> tuple[dict[str, object], dict[str, object]]:
    if not isinstance(value, dict) or set(value) != {
        "schema", "reference_revision", "consent", "attempt_budget",
        "timeout_seconds", "hosts",
    }:
        raise PreflightError("inventory-schema-invalid")
    if value["schema"] != SCHEMA:
        raise PreflightError("inventory-schema-invalid")
    if value["reference_revision"] != PIN:
        raise PreflightError("reference-pin-mismatch")
    consent = value["consent"]
    if not isinstance(consent, dict) or set(consent) != {"owner_authorized", "scope"}:
        raise PreflightError("authorization-missing")
    if consent["owner_authorized"] is not True or consent["scope"] != "isolated-private-subnet-only":
        raise PreflightError("authorization-missing")
    if value["attempt_budget"] != 1:
        raise PreflightError("attempt-budget-invalid")
    timeout = value["timeout_seconds"]
    if not isinstance(timeout, int) or isinstance(timeout, bool) or not 1 <= timeout <= MAX_TIMEOUT_SECONDS:
        raise PreflightError("timeout-budget-invalid")
    hosts = value["hosts"]
    if not isinstance(hosts, list) or len(hosts) != 2:
        raise PreflightError("host-pair-invalid")

    by_role: dict[str, dict[str, object]] = {}
    for host in hosts:
        if not isinstance(host, dict) or set(host) != {
            "role", "host_id", "interface", "address", "prefix", "udp_port", "data_dir",
        }:
            raise PreflightError("host-inventory-invalid")
        role = host["role"]
        host_id = host["host_id"]
        if role not in ROLES or role in by_role:
            raise PreflightError("host-pair-invalid")
        if not isinstance(host_id, str) or not re.fullmatch(r"[0-9a-f]{32}", host_id):
            raise PreflightError("host-inventory-invalid")
        interface = host["interface"]
        data_dir = host["data_dir"]
        if not isinstance(interface, str) or not interface or len(interface) > 64:
            raise PreflightError("host-inventory-invalid")
        if not isinstance(data_dir, str) or not data_dir or not Path(data_dir).is_absolute():
            raise PreflightError("host-inventory-invalid")
        try:
            address = ipaddress.ip_address(host["address"])
        except (ValueError, TypeError):
            raise PreflightError("address-invalid") from None
        prefix = host["prefix"]
        port = host["udp_port"]
        rfc1918 = (
            ipaddress.ip_network("10.0.0.0/8"),
            ipaddress.ip_network("172.16.0.0/12"),
            ipaddress.ip_network("192.168.0.0/16"),
        )
        if not isinstance(address, ipaddress.IPv4Address) or not any(address in network for network in rfc1918):
            raise PreflightError("address-not-private-ipv4")
        if not isinstance(prefix, int) or isinstance(prefix, bool) or not 8 <= prefix <= 30:
            raise PreflightError("prefix-invalid")
        if not isinstance(port, int) or isinstance(port, bool) or not 1024 <= port <= 65535:
            raise PreflightError("port-invalid")
        row = dict(host)
        row["_ip"] = address
        row["_network"] = ipaddress.ip_network(f"{address}/{prefix}", strict=False)
        by_role[role] = row

    if set(by_role) != ROLES:
        raise PreflightError("host-pair-invalid")
    a, b = by_role["i2pr"], by_role["i2pd"]
    if a["_ip"] == b["_ip"] or a["udp_port"] == b["udp_port"]:
        raise PreflightError("host-endpoints-collide")
    if a["_network"] != b["_network"]:
        raise PreflightError("hosts-not-on-same-subnet")
    if a["host_id"] == b["host_id"]:
        raise PreflightError("host-identities-not-distinct")
    return by_role["i2pr"], by_role["i2pd"]


def repository_root() -> Path:
    result = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"], capture_output=True,
        text=True, check=False,
    )
    if result.returncode != 0:
        raise PreflightError("repository-root-unavailable")
    return Path(result.stdout.strip()).resolve()


def load_inventory(path: Path, repo: Path) -> tuple[dict[str, object], dict[str, object]]:
    try:
        resolved = path.resolve(strict=True)
        resolved.relative_to(repo)
        raise PreflightError("inventory-must-stay-outside-repository")
    except ValueError:
        pass
    except OSError:
        raise PreflightError("inventory-unavailable") from None
    if os.name == "posix" and stat.S_IMODE(resolved.stat().st_mode) & 0o077:
        raise PreflightError("inventory-permissions-too-open")
    try:
        value = json.loads(resolved.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError):
        raise PreflightError("inventory-unavailable") from None
    return validate_inventory(value)


def interface_ipv4_addresses(interface: str) -> set[str]:
    """Return assigned IPv4 values using the host's ordinary read-only tools."""
    ip = shutil_which("ip")
    if ip:
        result = subprocess.run(
            [ip, "-j", "-4", "addr", "show", "dev", interface],
            capture_output=True, text=True, check=False, timeout=5,
        )
        if result.returncode != 0:
            raise PreflightError("interface-unavailable")
        try:
            records = json.loads(result.stdout)
            return {
                row["local"] for record in records
                for row in record.get("addr_info", [])
                if row.get("family") == "inet" and isinstance(row.get("local"), str)
            }
        except (json.JSONDecodeError, AttributeError, KeyError, TypeError):
            raise PreflightError("interface-inventory-invalid") from None
    ifconfig = shutil_which("ifconfig")
    if ifconfig:
        result = subprocess.run(
            [ifconfig, interface], capture_output=True, text=True,
            check=False, timeout=5,
        )
        if result.returncode != 0:
            raise PreflightError("interface-unavailable")
        return set(re.findall(r"\binet\s+(?:addr:)?(\d{1,3}(?:\.\d{1,3}){3})\b", result.stdout))
    raise PreflightError("interface-probe-unavailable")


def shutil_which(name: str) -> str | None:
    # Keep the script standard-library-only.
    import shutil
    return shutil.which(name)


def check_local_host(host: dict[str, object], peer: dict[str, object], repo: Path) -> None:
    interface = str(host["interface"])
    address = str(host["_ip"])
    try:
        socket.if_nametoindex(interface)
    except (OSError, AttributeError):
        raise PreflightError("interface-unavailable") from None
    if address not in interface_ipv4_addresses(interface):
        raise PreflightError("local-address-not-assigned")

    local = (address, int(host["udp_port"]))
    probe = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    try:
        probe.bind(local)
        # UDP connect selects a route and source address in the local kernel;
        # it does not send a datagram or test remote reachability.
        probe.connect((str(peer["_ip"]), int(peer["udp_port"])))
        if probe.getsockname()[0] != address:
            raise PreflightError("route-source-mismatch")
    except PreflightError:
        raise
    except OSError:
        raise PreflightError("local-bind-or-route-unavailable") from None
    finally:
        probe.close()

    data_dir = Path(str(host["data_dir"])).resolve()
    try:
        data_dir.relative_to(repo)
    except ValueError:
        pass
    else:
        raise PreflightError("data-directory-must-stay-outside-repository")
    if data_dir.exists():
        if not data_dir.is_dir():
            raise PreflightError("data-directory-invalid")
        if os.name == "posix" and stat.S_IMODE(data_dir.stat().st_mode) & 0o077:
            raise PreflightError("data-directory-permissions-too-open")


def verify_reference(source: Path, binary: Path) -> str:
    try:
        source = source.resolve(strict=True)
        binary = binary.resolve(strict=True)
    except OSError:
        raise PreflightError("reference-artifact-unavailable") from None
    head = subprocess.run(
        ["git", "-C", str(source), "rev-parse", "HEAD"], capture_output=True,
        text=True, check=False,
    )
    dirty = subprocess.run(
        ["git", "-C", str(source), "status", "--porcelain", "--untracked-files=no"],
        capture_output=True, text=True, check=False,
    )
    if head.returncode != 0 or head.stdout.strip() != PIN:
        raise PreflightError("reference-pin-mismatch")
    if dirty.returncode != 0 or dirty.stdout.strip():
        raise PreflightError("reference-source-modified")
    version = subprocess.run(
        [str(binary), "--version"], capture_output=True, text=True,
        check=False, timeout=10,
    )
    if version.returncode != 0 or "i2pd version 2.61.0" not in version.stdout:
        raise PreflightError("reference-version-mismatch")
    try:
        return hashlib.sha256(binary.read_bytes()).hexdigest()
    except OSError:
        raise PreflightError("reference-artifact-unavailable") from None


def run_self_test() -> None:
    def row(role: str, address: str, data_dir: str, port: int) -> dict[str, object]:
        return {
            "role": role, "host_id": hashlib.sha256(role.encode()).hexdigest()[:32],
            "interface": "fixture0", "address": address,
            "prefix": 24, "udp_port": port, "data_dir": data_dir,
        }

    base: dict[str, object] = {
        "schema": SCHEMA,
        "reference_revision": PIN,
        "consent": {"owner_authorized": True, "scope": "isolated-private-subnet-only"},
        "attempt_budget": 1,
        "timeout_seconds": 120,
        "hosts": [row("i2pr", "192.168.44.10", "/tmp/i2pr-plan449-a", 19001),
                  row("i2pd", "192.168.44.11", "/tmp/i2pr-plan449-b", 19002)],
    }
    a, b = validate_inventory(base)
    assert a["_ip"] != b["_ip"]
    assert a["_network"] == b["_network"]

    def rejected(change: object, code: str) -> None:
        try:
            validate_inventory(change)
        except PreflightError as error:
            assert error.code == code, (error.code, code)
        else:
            raise AssertionError(f"expected {code}")

    bad = json.loads(json.dumps(base)); bad["reference_revision"] = "0" * 40
    rejected(bad, "reference-pin-mismatch")
    bad = json.loads(json.dumps(base)); bad["consent"]["owner_authorized"] = False
    rejected(bad, "authorization-missing")
    bad = json.loads(json.dumps(base)); bad["hosts"][1]["address"] = "192.168.44.10"
    rejected(bad, "host-endpoints-collide")
    bad = json.loads(json.dumps(base)); bad["hosts"][1]["address"] = "192.168.45.11"
    rejected(bad, "hosts-not-on-same-subnet")
    bad = json.loads(json.dumps(base)); bad["hosts"][1]["host_id"] = bad["hosts"][0]["host_id"]
    rejected(bad, "host-identities-not-distinct")
    bad = json.loads(json.dumps(base)); bad["attempt_budget"] = 2
    rejected(bad, "attempt-budget-invalid")
    bad = json.loads(json.dumps(base)); bad["hosts"][0]["address"] = "8.8.8.8"
    rejected(bad, "address-not-private-ipv4")
    print("plan449-preflight-self-test: passed (8 positive/negative cases)")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--role", choices=sorted(ROLES))
    parser.add_argument("--reference-source", type=Path)
    parser.add_argument("--i2pd-binary", type=Path)
    args = parser.parse_args()
    if args.self_test:
        run_self_test()
        return 0
    if not all((args.manifest, args.role, args.reference_source, args.i2pd_binary)):
        parser.error("real preflight requires --manifest, --role, --reference-source, and --i2pd-binary")
    try:
        repo = repository_root()
        i2pr_host, i2pd_host = load_inventory(args.manifest, repo)
        local, peer = (i2pr_host, i2pd_host) if args.role == "i2pr" else (i2pd_host, i2pr_host)
        check_local_host(local, peer, repo)
        binary_sha256 = verify_reference(args.reference_source, args.i2pd_binary)
    except PreflightError as error:
        print(json.dumps({"schema": "i2pr-plan449-preflight-result-v1",
                          "result": "environment-unavailable", "reason": error.code,
                          "packet_attempt": "none", "live_qualification": "not-run"},
                         sort_keys=True))
        return 1
    print(json.dumps({"schema": "i2pr-plan449-preflight-result-v1",
                      "result": "preflight-passed", "role": args.role,
                      "reference_revision": PIN, "reference_binary_sha256": binary_sha256,
                      "packet_attempt": "none", "live_qualification": "not-run"},
                     sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
