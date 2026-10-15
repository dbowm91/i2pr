#!/usr/bin/env python3
"""Controlled Java floodfill comparator (Plan 479, profiles A/B).

Drives unmodified Java I2P 2.13.0 through its normal Router(Properties)
lifecycle on loopback and observes the pinned stock selector's treatment
of honestly tiered controlled candidates. Only sanitized stage counters
are printed: booleans, counts, and tier letters observed by stock Java.
No hash, address, key, or log material ever leaves the scratch directory.

Profile B (low-tier comparator): Java's NetDB holds an eligible `fN`
i2pr candidate and a truthful `L` i2pr candidate (reachable, biddable,
explicitly non-floodfill). The pass gate is end-to-end preference: the
eligible candidate is admitted, selected, and solely receives Java's
normal Store with an accepted daemon effect, while the low-tier
candidate is present but never selected and never receives a store.

Profile A (healthy-stock population): Java's NetDB holds the eligible
`fN` i2pr candidate and a healthy stock i2pd `fO` floodfill. The gate
is operation among genuinely multiple healthy candidates: both are
admitted and selector-included, Java's normal Store succeeds, and the
recipient is attributed (i2pr effect or stock recipient) without
overclaiming preference between two healthy peers.

Frozen per-profile attempt budget: 1. A failure stops the profile with
the first missing stage; the other profile still runs, because the two
profiles are independent evidence. Missing exact pins fail hard
(exit 2), never silently pass. Reference processes are never patched.
"""

from __future__ import annotations

import argparse
import base64
import http.client
import json
import os
from pathlib import Path
import re
import signal
import socket
import subprocess
import sys
import tempfile
import time


JAVA_PIN = "9134f808337b401e8e53c73734c81fab04280c9d"
JAVA_VERSION = "2.13.0"
I2PD_PIN = "635b013a612ff47278ef02acf8580a28e10e26c5"
I2PD_VERSION = "2.61.0"
CONTROL_PASSWORD = "plan479-loopback-only"
START_TIMEOUT = 50
RUN_TIMEOUT = 45
I2PD_START_TIMEOUT = 120
MAX_ROUTER_INFO = 786_432

# Shared 12/48/64/128/256/2000 KiB/s Java-compatible boundaries. The
# eligible pair shares 128 KiB/s (N); the low-tier pair shares
# 32 KiB/s (L). Rates are bytes/sec, never bits/sec.
ELIGIBLE_RATES = (262_144, 262_144, 50)
LOWTIER_RATES = (131_072, 131_072, 25)


def reserve(kind: int) -> int:
    with socket.socket(socket.AF_INET, kind) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def router_info_hash(
    interop: Path,
    path: Path,
    cwd: Path,
    expected_loopback_port: int | None = None,
) -> tuple[bytes, dict[str, object]]:
    raw = path.read_bytes()
    if not raw or len(raw) > MAX_ROUTER_INFO:
        raise RuntimeError("router-info-size-out-of-range")
    command = [str(interop), "ntcp2", "router-info-summary", "--router-info", str(path)]
    if expected_loopback_port is not None:
        command.extend(("--expected-loopback-port", str(expected_loopback_port)))
    summary = json.loads(subprocess.check_output(command, cwd=cwd, text=True))
    identity = json.loads(subprocess.check_output(
        [str(interop), "ntcp2", "router-hash", "--router-info", str(path)],
        cwd=cwd, text=True,
    ))
    digest = bytes.fromhex(identity["router_hash_sha256"])
    if len(digest) != 32:
        raise RuntimeError("router-hash-length-invalid")
    return digest, summary


def i2p_base64(raw: bytes) -> str:
    return base64.b64encode(raw).decode("ascii").replace("+", "-").replace("/", "~").rstrip("=")


def java_store_path(root: Path, digest: bytes) -> Path:
    ident = i2p_base64(digest)
    return root / "netDb" / f"r{ident[0]}" / f"routerInfo-{ident}.dat"


def control(port: int, request: dict[str, object]) -> dict[str, object]:
    conn = http.client.HTTPSConnection(
        "127.0.0.1", port, timeout=2,
        context=__import__("ssl")._create_unverified_context(),
    )
    conn.request("POST", "/", json.dumps(request).encode(), {"Content-Type": "application/json"})
    response = conn.getresponse()
    body = response.read(1_100_000)
    conn.close()
    if response.status != 200:
        raise RuntimeError("i2pcontrol-http-rejected")
    decoded = json.loads(body)
    if "error" in decoded:
        raise RuntimeError("i2pcontrol-request-rejected")
    return decoded["result"]


def fetch_daemon_router_info(port: int, output: Path, deadline: float) -> None:
    token: str | None = None
    last_error = "unavailable"
    while time.monotonic() < deadline:
        try:
            if token is None:
                token = str(control(port, {
                    "jsonrpc": "2.0", "id": 1, "method": "Authenticate",
                    "params": {"API": 1, "Password": CONTROL_PASSWORD},
                })["Token"])
            result = control(port, {
                "jsonrpc": "2.0", "id": 2, "method": "RouterInfo",
                "params": {"Token": token, "i2p.router.info": None},
            })
            encoded = result.get("i2p.router.info")
            if isinstance(encoded, str) and encoded:
                text = encoded.translate(str.maketrans("-~", "+/"))
                raw = base64.b64decode(text + "=" * (-len(text) % 4), validate=True)
                if not raw or len(raw) > MAX_ROUTER_INFO:
                    raise RuntimeError("daemon-router-info-size-invalid")
                output.write_bytes(raw)
                return
        except (OSError, KeyError, TypeError, ValueError, RuntimeError) as exc:
            last_error = type(exc).__name__
            token = None
        time.sleep(0.15)
    raise TimeoutError(f"daemon-router-info-unavailable:{last_error}")


def stop(child: subprocess.Popen[bytes] | None) -> None:
    if child is None or child.poll() is not None:
        return
    child.send_signal(signal.SIGTERM)
    try:
        child.wait(timeout=8)
    except subprocess.TimeoutExpired:
        child.kill()
        child.wait(timeout=4)


def java_runtime_log(java_data: Path, stdout_log: Path) -> str:
    # Router(Properties) writes protocol logs under i2p.dir.log, not
    # stdout. Read only the bounded scratch log set and never export it.
    chunks = [stdout_log.read_text(errors="replace")]
    total = len(chunks[0])
    for path in sorted((java_data / "logs").rglob("*")):
        if not path.is_file():
            continue
        if total >= 8_000_000:
            break
        text = path.read_text(errors="replace")[-min(2_000_000, 8_000_000 - total):]
        chunks.append(text)
        total += len(text)
    return "\n".join(chunks)


def java_seed(seed_root: Path, raw: bytes, digest: bytes) -> None:
    path = java_store_path(seed_root, digest)
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    path.write_bytes(raw)
    path.chmod(0o600)


def java_diagnostic(port: int, request: str, deadline: float) -> str:
    while time.monotonic() < deadline:
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=1) as conn:
                conn.settimeout(1)
                conn.sendall((request + "\n").encode("ascii"))
                response = conn.makefile("rb").readline(4096).decode("ascii", errors="strict")
            if response.startswith(("P220-EV ", "P470-EV ", "P470-STORE ", "P470-LISTENER ")):
                return response.strip()
        except (OSError, UnicodeError):
            time.sleep(0.1)
    return "unavailable"


def wait_java_listener(child: subprocess.Popen[bytes], port: int, deadline: float) -> None:
    while time.monotonic() < deadline:
        if child.poll() is not None:
            raise RuntimeError("java-router-exited-before-listener")
        if "P470-LISTENER listening=true" in java_diagnostic(
            port, "P470-LISTENER", min(deadline, time.monotonic() + 1)
        ):
            return
        time.sleep(0.1)
    raise TimeoutError("java-ntcp2-listener-timeout")


def java_send_counters(port: int, deadline: float) -> dict[str, int]:
    response = java_diagnostic(port, "P470-COUNTERS", deadline)
    if not response.startswith("P470-EV observable=true"):
        return {}
    allowed = {
        "transport_bidFailNoTransports", "transport_bidFailBanlisted",
        "transport_bidFailSelf", "transport_expiredOnQueueLifetime",
        "transport_sendMessageFailureLifetime", "ntcp_accept",
        "ntcp_registerConnect", "ntcp_connectSuccessful", "ntcp_inboundEstablished",
        "ntcp_outboundFailedIOEImmediate", "ntcp_outboundEstablishFailed",
        "ntcp_connectFailedIOE", "ntcp_connectFailedTimeout",
        "ntcp_connectFailedTimeoutIOE",
        "ntcp_zeroReadDrop", "ntcp_readError", "netDb_lookupsReceived",
        "netDb_lookupsDropped", "netDb_lookupsHandled", "netDb_lookupsMatched",
        "netDb_nonFFLookupsDropped", "netDb_DLMAllZeros",
    }
    counts: dict[str, int] = {}
    for name, value in re.findall(r"([a-zA-Z0-9_]+)=(-?\d+)", response):
        if name in allowed:
            counts[name] = int(value)
    return counts if set(counts) == allowed else {}


def java_caps(port: int, digest_hex: str, deadline: float) -> dict[str, object]:
    """Stock-computed capability view of one seeded candidate.

    Returns in-memory tier/capability facts only; the caller prints
    booleans derived from them, never hashes or capability strings.
    """
    response = java_diagnostic(port, "P220-CAPABILITIES " + digest_hex, deadline)
    facts: dict[str, object] = {"present": False}
    if not response.startswith("P220-EV kind=capabilities"):
        return facts
    facts["present"] = "stored=true" in response
    for token in response.split():
        if token.startswith("bandwidth_tier="):
            facts["bandwidth_tier"] = token.split("=", 1)[1]
        elif token.startswith("has_floodfill_capability="):
            facts["has_floodfill_capability"] = token.split("=", 1)[1] == "true"
    return facts


def java_peers_ff(port: int, deadline: float) -> set[str]:
    """Hashes (hex) of Java's live floodfill peer set, in memory only."""
    response = java_diagnostic(port, "P220-PEERS-FLOODFILL", deadline)
    if not response.startswith("P220-EV kind=peers-floodfill"):
        return set()
    return set(re.findall(r"peer_\d+_hex=([0-9a-f]{64})", response))


def java_transport(port: int, digest_hex: str, deadline: float) -> str:
    return java_diagnostic(port, "P470-TRANSPORT " + digest_hex, deadline)


def trigger_java_store_job(port: int, deadline: float) -> bool:
    while time.monotonic() < deadline:
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=1) as conn:
                conn.settimeout(1)
                conn.sendall(b"P470-STORE\n")
                response = conn.makefile("rb").readline(128).decode("ascii", errors="strict").strip()
            if response.startswith("P470-STORE submitted=1"):
                return True
            if response != "P470-NOT-READY":
                return False
        except (OSError, UnicodeError):
            time.sleep(0.2)
    return False


def assert_router(
    summary: dict[str, object], *, label: str, ntcp2_only: bool,
    expected_loopback_port: int | None = None, require_floodfill: bool = False,
) -> None:
    if summary.get("network_id") != "2":
        raise RuntimeError(f"{label}-network-id-invalid")
    version = tuple(int(item) for item in str(summary.get("version", "0")).split("."))
    if version < (0, 9, 62):
        raise RuntimeError(f"{label}-router-version-ineligible")
    if require_floodfill and summary.get("floodfill") is not True:
        raise RuntimeError(f"{label}-controlled-floodfill-not-active")
    addresses = summary.get("addresses")
    if not isinstance(addresses, list) or not addresses:
        raise RuntimeError(f"{label}-router-has-no-published-address")
    styles: set[str] = set()
    for address in addresses:
        if not isinstance(address, dict) or not address.get("loopback"):
            raise RuntimeError(f"{label}-router-address-not-loopback")
        style = str(address.get("style", ""))
        styles.add(style)
        if style not in ({"NTCP2"} if ntcp2_only else {"NTCP2", "SSU2"}):
            raise RuntimeError(f"{label}-router-address-style-unexpected")
        if not address.get("port_valid") or not address.get("static_key") or not address.get("i_key"):
            if style == "NTCP2":
                raise RuntimeError(f"{label}-ntcp2-static-key-or-version-invalid")
        if style == "NTCP2" and expected_loopback_port is not None:
            if address.get("listener_port_matches") is not True:
                raise RuntimeError(f"{label}-signed-ntcp2-listener-endpoint-mismatch")
    if "NTCP2" not in styles or (ntcp2_only and styles != {"NTCP2"}):
        raise RuntimeError(f"{label}-ntcp2-profile-invalid")


def daemon_config_text(
    data_dir: Path, ntcp_port: int, ssu_port: int, control_port: int,
    rates: tuple[int, int, int],
) -> str:
    inbound, outbound, share = rates
    return f'''schema_version = 1
[router]
data_dir = {json.dumps(str(data_dir))}
[transport.ntcp2]
enabled = true
qualification_profile = "controlled_loopback"
bind_address = "127.0.0.1"
listen_port = {ntcp_port}
[ssu2]
enabled = true
bind_ipv4 = "127.0.0.1"
bind_ipv6 = ""
port = {ssu_port}
[floodfill]
enabled = true
qualification_profile = "controlled_loopback"
send_reply_token_probe = true
send_initiator_lookup_probe = false
[bandwidth]
enabled = true
inbound_bytes_per_second = {inbound}
outbound_bytes_per_second = {outbound}
share_percent = {share}
control_reserve_percent = 10
# One max NTCP2 wire frame must fit the usable burst
# (65537 * 100 / 90 = 72819 minimum); the 64 KiB daemon default
# is below that floor, so the lane sets 128 KiB explicitly.
burst_bytes = 131072
max_pending_requests = 16
max_pending_per_peer = 4
[i2pcontrol]
enabled = true
bind_address = "127.0.0.1"
port = {control_port}
password = {json.dumps(CONTROL_PASSWORD)}
'''


def wait_daemon_router_info(
    interop: Path, root: Path, daemon: Path, config: Path, control_port: int,
    ntcp_port: int, work: Path, name: str, require_floodfill: bool,
    accept_log_token: str, daemon_log: Path,
    child_holder: list[subprocess.Popen[bytes] | None],
) -> tuple[bytes, dict[str, object]]:
    """Start one daemon and wait for its role-shaped RouterInfo.

    The floodfill flag must match the requested role, and the role's
    own composition log line must appear, so a classless or
    wrong-role record can never satisfy the wait.
    """
    with daemon_log.open("wb") as output:
        child = subprocess.Popen([str(daemon), "run", "--config", str(config)], cwd=root,
                                 stdout=output, stderr=subprocess.STDOUT)
        child_holder[0] = child
        try:
            raw_path = work / f"{name}.routerInfo"
            deadline = time.monotonic() + START_TIMEOUT
            while True:
                fetch_daemon_router_info(control_port, raw_path, deadline)
                daemon_hash, summary = router_info_hash(
                    interop, raw_path, root, ntcp_port
                )
                role_ok = (summary.get("floodfill") is True) == require_floodfill
                logged = accept_log_token in daemon_log.read_text(errors="replace")
                if role_ok and logged:
                    break
                if time.monotonic() >= deadline:
                    raise TimeoutError(f"{name}-role-shaped-router-info-timeout")
                time.sleep(0.2)
            assert_router(
                summary, label=name, ntcp2_only=False,
                expected_loopback_port=ntcp_port,
                require_floodfill=require_floodfill,
            )
            return daemon_hash, summary
        except BaseException:
            stop(child)
            child_holder[0] = None
            raise


def start_java(
    root: Path, java_cache: Path, work: Path, java_data: Path,
    ntcp_port: int, java_diag_port: int, java_publish_port: int,
    java_log: Path, child_holder: list[subprocess.Popen[bytes] | None],
) -> subprocess.Popen[bytes]:
    java_src = root / "tests/integration/m6-interop/java"
    java_build = work / "java-build"
    java_build.mkdir(mode=0o700)
    java_sources = [java_src / "ControlledRouter.java", java_src / "ReferenceRawDestination.java",
                    java_src / "ReferenceStreamingService.java"]
    java_sources.extend(sorted((java_src / "net/i2p/router/networkdb/kademlia").glob("*.java")))
    java_cp = ":".join([str(java_cache / "lib" / "*"), str(java_cache / "*")])
    subprocess.run(["javac", "-d", str(java_build), "-cp", java_cp, *map(str, java_sources)],
                   cwd=root, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    classpath = f"{java_build}:{java_cp}"
    with java_log.open("wb") as output:
        child = subprocess.Popen([
            "java", "-Xms128m", "-Xmx512m", "-Di2p.dir.base=" + str(java_cache),
            "-cp", classpath, "ControlledRouter", str(java_data), "127.0.0.1",
            str(ntcp_port), "0", "0", str(java_diag_port), "ntcp2", str(java_publish_port),
        ], cwd=root, stdout=output, stderr=subprocess.STDOUT)
    child_holder[0] = child
    wait_java_listener(child, java_diag_port, time.monotonic() + START_TIMEOUT)
    java_ri_path = java_data / "router" / "router.info"
    deadline = time.monotonic() + START_TIMEOUT
    while not java_ri_path.is_file() and time.monotonic() < deadline:
        if child.poll() is not None:
            raise RuntimeError("java-router-exited-before-routerinfo")
        time.sleep(0.1)
    return child


def check_environment(root: Path) -> tuple[Path, Path, Path, Path]:
    java_cache = Path(os.environ.get(
        "I2PR_PLAN479_JAVA_CACHE",
        root / "target/interop/cache/m6-java" / JAVA_PIN,
    ))
    java_source = Path(os.environ.get(
        "I2PR_PLAN479_JAVA_SOURCE",
        root / "target/interop/m6-java-sources" / f"i2p.i2p-{JAVA_PIN}",
    ))
    i2pd_cache = Path(os.environ.get(
        "I2PR_PLAN479_I2PD_CACHE",
        root / "target/interop/cache/ssu2/i2pd" / I2PD_PIN,
    ))
    if not (java_cache / "lib" / "router.jar").is_file():
        print("plan479: environment-unavailable: exact Java 2.13.0 cache absent", file=sys.stderr)
        raise SystemExit(2)
    try:
        revision = subprocess.check_output(
            ["git", "-C", str(java_source), "rev-parse", "HEAD"], text=True).strip()
    except (OSError, subprocess.CalledProcessError):
        print("plan479: environment-unavailable: exact Java source checkout absent", file=sys.stderr)
        raise SystemExit(2)
    if revision != JAVA_PIN:
        print("plan479: exact Java source pin mismatch", file=sys.stderr)
        raise SystemExit(2)
    if not (i2pd_cache / "bin" / "i2pd").is_file():
        print("plan479: environment-unavailable: exact i2pd 2.61.0 cache absent", file=sys.stderr)
        raise SystemExit(2)
    try:
        revision = (i2pd_cache / "source-revision.txt").read_text().strip()
    except OSError:
        print("plan479: environment-unavailable: i2pd source revision unreadable", file=sys.stderr)
        raise SystemExit(2)
    if revision != I2PD_PIN:
        print("plan479: exact i2pd source pin mismatch", file=sys.stderr)
        raise SystemExit(2)
    daemon = root / "target/debug/i2pr"
    interop = root / "target/debug/i2pr-interop"
    subprocess.run(["cargo", "build", "--locked", "-p", "i2pr-daemon", "-p", "i2pr-interop"],
                   cwd=root, check=True)
    if not daemon.is_file() or not interop.is_file():
        raise RuntimeError("workspace-binaries-missing")
    return java_cache, java_source, i2pd_cache, daemon, interop


def wait_live_router_info(
    interop: Path, root: Path, control_port: int, ntcp_port: int,
    raw_path: Path, name: str, require_floodfill: bool,
) -> bytes:
    """Wait until the live daemon serves its role-shaped record.

    The floodfill flag plus the address/endpoint shape must match;
    the stock-observed tier gate later backstops record freshness.
    """
    deadline = time.monotonic() + START_TIMEOUT
    while True:
        fetch_daemon_router_info(control_port, raw_path, deadline)
        daemon_hash, summary = router_info_hash(interop, raw_path, root, ntcp_port)
        if (summary.get("floodfill") is True) == require_floodfill:
            try:
                assert_router(
                    summary, label=name, ntcp2_only=False,
                    expected_loopback_port=ntcp_port,
                    require_floodfill=require_floodfill,
                )
            except RuntimeError:
                pass
            else:
                return daemon_hash
        if time.monotonic() >= deadline:
            raise TimeoutError(f"{name}-live-router-info-timeout")
        time.sleep(0.2)


def fetch_two(role_path: Path, control_port: int) -> bytes:
    """Refetch twice so the seeded bytes postdate the composition log line."""
    fetch_daemon_router_info(control_port, role_path, time.monotonic() + START_TIMEOUT)
    time.sleep(1.0)
    fetch_daemon_router_info(control_port, role_path, time.monotonic() + START_TIMEOUT)
    return role_path.read_bytes()


def wait_store_effect(
    java_diag_port: int, java_publish_port: int, deadline: float,
) -> tuple[bool, bool]:
    """Trigger Java's normal Store and wait for its success callback.

    Returns (submitted, succeeded). Daemon-side effects are counted
    separately by the caller from each daemon's own log.
    """
    submitted = trigger_java_store_job(java_publish_port, min(deadline, time.monotonic() + 15))
    if not submitted:
        return False, False
    while time.monotonic() < deadline:
        status = java_diagnostic(
            java_diag_port, "P470-STORE-STATUS", min(deadline, time.monotonic() + 1))
        if "P470-STORE submitted=1 succeeded=1 failed=0" in status:
            return True, True
        time.sleep(0.2)
    return True, False


def count_in(text: str, token: str) -> int:
    return text.count(token)


def emit_receipt(profile: str, stages: dict[str, int], extra: dict[str, object]) -> None:
    print(f"plan479_profile_{profile}_stages=" + json.dumps(
        stages, sort_keys=True, separators=(",", ":")))
    for key, value in extra.items():
        print(f"plan479_profile_{profile}_{key}=" + json.dumps(
            value, sort_keys=True, separators=(",", ":")))
    print("plan479_reference_pin=" + JAVA_VERSION + "@" + JAVA_PIN)


def fail(profile: str, stages: dict[str, int], reason: str) -> int:
    missing = next((k for k, v in stages.items() if not v), "none")
    print(f"plan479-{profile}: first-missing-stage={missing} reason={reason}", file=sys.stderr)
    return 1


def run_daemon_shape(
    interop: Path, root: Path, daemon: Path, work: Path, name: str,
    rates: tuple[int, int, int], require_floodfill: bool, log_token: str,
) -> tuple[Path, bytes, int, int]:
    """Shape phase: start, wait for the role-shaped record, stop.

    Returns the persisted role-shaped RI path, its hash, and the
    reserved NTCP2/control ports for the live restart.
    """
    data_dir = work / f"{name}-data"
    data_dir.mkdir(mode=0o700)
    ntcp_port = reserve(socket.SOCK_STREAM)
    ssu_port = reserve(socket.SOCK_DGRAM)
    control_port = reserve(socket.SOCK_STREAM)
    config = work / f"{name}.toml"
    config.write_text(daemon_config_text(
        data_dir, ntcp_port, ssu_port, control_port, rates), encoding="utf-8")
    config.chmod(0o600)
    subprocess.run([str(daemon), "identity", "generate", "--config", str(config)],
                   cwd=root, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    daemon_log = work / f"{name}.log"
    holder: list[subprocess.Popen[bytes] | None] = [None]
    try:
        with daemon_log.open("wb") as output:
            child = subprocess.Popen([str(daemon), "run", "--config", str(config)], cwd=root,
                                     stdout=output, stderr=subprocess.STDOUT)
            holder[0] = child
            raw_path = work / f"{name}.routerInfo"
            deadline = time.monotonic() + START_TIMEOUT
            daemon_hash = b""
            while True:
                fetch_daemon_router_info(control_port, raw_path, deadline)
                daemon_hash, summary = router_info_hash(interop, raw_path, root, ntcp_port)
                role_ok = (summary.get("floodfill") is True) == require_floodfill
                logged = log_token in daemon_log.read_text(errors="replace")
                if role_ok and logged:
                    break
                if time.monotonic() >= deadline:
                    raise TimeoutError(f"{name}-role-shaped-router-info-timeout")
                time.sleep(0.2)
            assert_router(
                summary, label=name, ntcp2_only=False,
                expected_loopback_port=ntcp_port, require_floodfill=require_floodfill,
            )
            _ = fetch_two(raw_path, control_port)
            digest, _ = router_info_hash(interop, raw_path, root, ntcp_port)
            assert digest == daemon_hash, "role-shaped record changed under refetch"
            return raw_path, daemon_hash, ntcp_port, control_port
    finally:
        stop(holder[0])


def restart_daemon(
    root: Path, daemon: Path, config: Path, control_port: int, ntcp_port: int,
    interop: Path, work: Path, name: str, require_floodfill: bool,
    daemon_log: Path, holder: list[subprocess.Popen[bytes] | None],
) -> bytes:
    """Live phase: restart the shaped daemon and wait for its live record."""
    with daemon_log.open("ab") as output:
        child = subprocess.Popen([str(daemon), "run", "--config", str(config)], cwd=root,
                                 stdout=output, stderr=subprocess.STDOUT)
        holder[0] = child
    raw_path = work / f"{name}-live.routerInfo"
    try:
        return wait_live_router_info(
            interop, root, control_port, ntcp_port, raw_path, name, require_floodfill)
    except BaseException:
        stop(child)
        holder[0] = None
        raise


def start_i2pd_stock(
    root: Path, interop: Path, i2pd_bin: Path, work: Path, name: str,
    holder: list[subprocess.Popen[bytes] | None],
) -> tuple[bytes, dict[str, object]]:
    """Start the exact-pinned stock i2pd floodfill on loopback NTCP2.

    i2pd 2.61.0 classical NTCP2 (`version = 2`) is pinned explicitly so
    Java 2.13.0 can bid and dial it; the RI is validated through the
    same interop summary as every controlled record.
    """
    ntcp_port = reserve(socket.SOCK_STREAM)
    ssu_port = reserve(socket.SOCK_DGRAM)
    data_dir = work / f"{name}data"
    data_dir.mkdir(mode=0o700)
    conf = work / f"{name}.conf"
    conf.write_text(f"""daemon = false
loglevel = debug
netid = 2
host = 127.0.0.1
port = {ssu_port}
ipv4 = true
ipv6 = false
nat = false
floodfill = true
reservedrange = false
bandwidth = O
[ssu2]
enabled = true
published = true
port = {ssu_port}
[ntcp2]
enabled = true
published = true
port = {ntcp_port}
version = 2
[http]
enabled = false
[httpproxy]
enabled = false
[socksproxy]
enabled = false
[sam]
enabled = false
[i2cp]
enabled = false
[i2pcontrol]
enabled = false
[upnp]
enabled = false
[reseed]
verify = false
urls =
threshold = 0
""", encoding="utf-8")
    log_path = work / f"{name}.log"
    with log_path.open("wb") as output:
        child = subprocess.Popen([str(i2pd_bin), f"--conf={conf}", f"--datadir={data_dir}",
                                  "--log=file", f"--logfile={log_path}"],
                                 cwd=root, stdout=output, stderr=subprocess.STDOUT)
        holder[0] = child
    try:
        ri_path = data_dir / "router.info"
        deadline = time.monotonic() + I2PD_START_TIMEOUT
        while not ri_path.is_file() and time.monotonic() < deadline:
            if child.poll() is not None:
                raise RuntimeError("stock-i2pd-exited-before-router-info")
            time.sleep(0.2)
        if not ri_path.is_file():
            raise TimeoutError("stock-i2pd-router-info-timeout")
        digest, summary = router_info_hash(interop, ri_path, root, ntcp_port)
        assert_router(
            summary, label="stock", ntcp2_only=False,
            expected_loopback_port=None, require_floodfill=True,
        )
        styles = {str(a.get("style", "")) for a in summary["addresses"]}
        assert "NTCP2" in styles, "stock-i2pd-ntcp2-address-unpublished"
        return digest, summary
    except BaseException:
        stop(child)
        holder[0] = None
        raise


def run_profile_b(
    root: Path, java_cache: Path, daemon: Path, interop: Path, work: Path,
) -> int:
    """Low-tier comparator: eligible `fN` beside truthful `L`."""
    profile = "b"
    stages = {"eligible_role_shaped": 0, "lowtier_role_shaped": 0,
              "both_present_in_java_netdb": 0,
              "eligible_java_tier_n": 0, "lowtier_java_tier_l": 0,
              "eligible_in_floodfill_peer_set": 0, "lowtier_absent_from_peer_set": 0,
              "eligible_selected_for_local_key": 0, "lowtier_not_selected": 0,
              "lowtier_ntcp_bid_available": 0,
              "java_store_job_succeeded": 0, "java_stock_transport_send": 0,
              "eligible_store_effect_accepted": 0, "lowtier_no_store_effect": 0}
    eligible_holder: list[subprocess.Popen[bytes] | None] = [None]
    lowtier_holder: list[subprocess.Popen[bytes] | None] = [None]
    java_holder: list[subprocess.Popen[bytes] | None] = [None]
    try:
        eligible_raw, eligible_hash, eligible_ntcp, eligible_control = run_daemon_shape(
            interop, root, daemon, work, "eligible", ELIGIBLE_RATES, True,
            "floodfill qualification activated",
        )
        stages["eligible_role_shaped"] = 1
        lowtier_raw, lowtier_hash, lowtier_ntcp, lowtier_control = run_daemon_shape(
            interop, root, daemon, work, "lowtier", LOWTIER_RATES, False,
            "controlled tiered addressed RouterInfo published without floodfill role",
        )
        stages["lowtier_role_shaped"] = 1

        # Live phase: both daemons serve their shaped records while
        # Java runs. The low-tier daemon is fully live (listeners up)
        # so a Java send to it would land; the gate is that Java's
        # own selector never yields it.
        eligible_config = work / "eligible.toml"
        lowtier_config = work / "lowtier.toml"
        restart_daemon(root, daemon, eligible_config, eligible_control, eligible_ntcp,
                       interop, work, "eligible", True,
                       work / "eligible.log", eligible_holder)
        restart_daemon(root, daemon, lowtier_config, lowtier_control, lowtier_ntcp,
                       interop, work, "lowtier", False,
                       work / "lowtier.log", lowtier_holder)

        java_data = work / "java"
        java_data.mkdir(mode=0o700)
        java_seed(java_data, eligible_raw.read_bytes(), eligible_hash)
        java_seed(java_data, lowtier_raw.read_bytes(), lowtier_hash)
        java_ntcp = reserve(socket.SOCK_STREAM)
        java_diag = reserve(socket.SOCK_STREAM)
        java_publish = reserve(socket.SOCK_STREAM)
        java_log = work / "java.log"
        start_java(root, java_cache, work, java_data, java_ntcp,
                   java_diag, java_publish, java_log, java_holder)
        java_ri_path = java_data / "router" / "router.info"
        java_hash, java_summary = router_info_hash(interop, java_ri_path, root, java_ntcp)
        assert_router(java_summary, label="java", ntcp2_only=True,
                      expected_loopback_port=java_ntcp)
        _ = java_hash
        deadline = time.monotonic() + 15
        eligible_hex = eligible_hash.hex()
        lowtier_hex = lowtier_hash.hex()
        while time.monotonic() < deadline:
            eligible_caps = java_caps(java_diag, eligible_hex, deadline)
            lowtier_caps = java_caps(java_diag, lowtier_hex, deadline)
            if eligible_caps.get("present") and lowtier_caps.get("present"):
                break
            time.sleep(0.25)
        else:
            return fail(profile, stages, "seeded-candidates-not-visible-in-java-netdb")
        stages["both_present_in_java_netdb"] = 1
        if eligible_caps.get("bandwidth_tier") != "N":
            return fail(profile, stages, "eligible-tier-not-n-as-observed-by-stock-java")
        stages["eligible_java_tier_n"] = 1
        if lowtier_caps.get("bandwidth_tier") != "L":
            return fail(profile, stages, "lowtier-tier-not-l-as-observed-by-stock-java")
        stages["lowtier_java_tier_l"] = 1
        if eligible_caps.get("has_floodfill_capability") is not True:
            return fail(profile, stages, "eligible-without-floodfill-capability")
        if lowtier_caps.get("has_floodfill_capability") is not False:
            return fail(profile, stages, "lowtier-carries-floodfill-capability")
        peers = java_peers_ff(java_diag, time.monotonic() + 5)
        if eligible_hex not in peers:
            return fail(profile, stages, "eligible-absent-from-java-floodfill-peer-set")
        stages["eligible_in_floodfill_peer_set"] = 1
        if lowtier_hex in peers:
            return fail(profile, stages, "lowtier-present-in-java-floodfill-peer-set")
        stages["lowtier_absent_from_peer_set"] = 1
        eligible_preflight = java_transport(java_diag, eligible_hex, time.monotonic() + 5)
        if "selected_for_local_router_info=true" not in eligible_preflight:
            return fail(profile, stages, "eligible-not-selected-for-local-key")
        stages["eligible_selected_for_local_key"] = 1
        lowtier_preflight = java_transport(java_diag, lowtier_hex, time.monotonic() + 5)
        if "selected_for_local_router_info=true" in lowtier_preflight:
            return fail(profile, stages, "lowtier-selected-for-local-key")
        stages["lowtier_not_selected"] = 1
        if "ntcp2_bid_available=true" not in lowtier_preflight:
            return fail(profile, stages, "lowtier-not-biddable-selection-confounded")
        stages["lowtier_ntcp_bid_available"] = 1

        deadline = time.monotonic() + RUN_TIMEOUT
        _, succeeded = wait_store_effect(java_diag, java_publish, deadline)
        if not succeeded:
            return fail(profile, stages, "java-stock-store-job-not-successful")
        stages["java_store_job_succeeded"] = 1
        # Stop Java before reading its logs: the pinned stock log
        # writer buffers behind the success callback, so log text is
        # only complete after close-flush. Daemon-side effects were
        # already observed live; they are re-counted below from files.
        stop(java_holder[0])
        java_holder[0] = None
        java_text = java_runtime_log(java_data, java_log)
        # The triggered job travels the StoreJob/OutNetMessage path
        # (never the facade flood path), so the stock-transport send
        # line is the observable send. Single-recipient follows from
        # the pinned redundancy (1) plus selection membership
        # (eligible only), not from this count.
        if count_in(java_text, "Attempting to send on transport NTCP") < 1:
            return fail(profile, stages, "java-no-stock-transport-send")
        stages["java_stock_transport_send"] = 1
        eligible_text = (work / "eligible.log").read_text(errors="replace")
        if count_in(eligible_text,
                    "authenticated floodfill DatabaseStore accepted with response owed") < 1:
            return fail(profile, stages, "eligible-store-effect-absent")
        stages["eligible_store_effect_accepted"] = 1
        lowtier_text = (work / "lowtier.log").read_text(errors="replace")
        if count_in(lowtier_text,
                    "authenticated floodfill DatabaseStore accepted with response owed") != 0:
            return fail(profile, stages, "lowtier-received-store-effect")
        stages["lowtier_no_store_effect"] = 1
        emit_receipt(profile, stages, {
            "recipient": "eligible-i2pr-candidate",
            "lowtier_fairness_bid": "available",
        })
        return 0
    except (RuntimeError, TimeoutError, AssertionError, subprocess.CalledProcessError) as exc:
        return fail(profile, stages, type(exc).__name__ + ":" + str(exc)[:120])
    finally:
        stop(eligible_holder[0])
        stop(lowtier_holder[0])
        stop(java_holder[0])


def run_profile_a(
    root: Path, java_cache: Path, i2pd_bin: Path, daemon: Path, interop: Path, work: Path,
) -> int:
    """Healthy-stock population: eligible `fN` beside stock i2pd `f`."""
    profile = "a"
    stages = {"stock_role_shaped": 0, "eligible_role_shaped": 0,
              "both_present_in_java_netdb": 0,
              "eligible_java_tier_n": 0, "stock_floodfill_capable": 0,
              "eligible_in_floodfill_peer_set": 0, "stock_in_floodfill_peer_set": 0,
              "eligible_selected_for_local_key": 0, "stock_selected_for_local_key": 0,
              "java_store_job_succeeded": 0, "java_stock_transport_send": 0,
              "store_recipient_attributed": 0}
    stock_holder: list[subprocess.Popen[bytes] | None] = [None]
    eligible_holder: list[subprocess.Popen[bytes] | None] = [None]
    java_holder: list[subprocess.Popen[bytes] | None] = [None]
    try:
        stock_hash, _ = start_i2pd_stock(
            root, interop, i2pd_bin, work, "stock", stock_holder)
        stages["stock_role_shaped"] = 1
        eligible_raw, eligible_hash, eligible_ntcp, eligible_control = run_daemon_shape(
            interop, root, daemon, work, "eligible", ELIGIBLE_RATES, True,
            "floodfill qualification activated",
        )
        stages["eligible_role_shaped"] = 1
        eligible_config = work / "eligible.toml"
        restart_daemon(root, daemon, eligible_config, eligible_control, eligible_ntcp,
                       interop, work, "eligible", True,
                       work / "eligible.log", eligible_holder)

        java_data = work / "java"
        java_data.mkdir(mode=0o700)
        java_seed(java_data, eligible_raw.read_bytes(), eligible_hash)
        stock_ri = work / "stockdata" / "router.info"
        java_seed(java_data, stock_ri.read_bytes(), stock_hash)
        java_ntcp = reserve(socket.SOCK_STREAM)
        java_diag = reserve(socket.SOCK_STREAM)
        java_publish = reserve(socket.SOCK_STREAM)
        java_log = work / "java.log"
        start_java(root, java_cache, work, java_data, java_ntcp,
                   java_diag, java_publish, java_log, java_holder)
        java_ri_path = java_data / "router" / "router.info"
        java_hash, java_summary = router_info_hash(interop, java_ri_path, root, java_ntcp)
        assert_router(java_summary, label="java", ntcp2_only=True,
                      expected_loopback_port=java_ntcp)
        _ = java_hash
        deadline = time.monotonic() + 15
        eligible_hex = eligible_hash.hex()
        stock_hex = stock_hash.hex()
        while time.monotonic() < deadline:
            eligible_caps = java_caps(java_diag, eligible_hex, deadline)
            stock_caps = java_caps(java_diag, stock_hex, deadline)
            if eligible_caps.get("present") and stock_caps.get("present"):
                break
            time.sleep(0.25)
        else:
            return fail(profile, stages, "seeded-candidates-not-visible-in-java-netdb")
        stages["both_present_in_java_netdb"] = 1
        if eligible_caps.get("bandwidth_tier") != "N":
            return fail(profile, stages, "eligible-tier-not-n-as-observed-by-stock-java")
        stages["eligible_java_tier_n"] = 1
        if stock_caps.get("has_floodfill_capability") is not True:
            return fail(profile, stages, "stock-without-floodfill-capability")
        stages["stock_floodfill_capable"] = 1
        peers = java_peers_ff(java_diag, time.monotonic() + 5)
        if eligible_hex not in peers:
            return fail(profile, stages, "eligible-absent-from-java-floodfill-peer-set")
        stages["eligible_in_floodfill_peer_set"] = 1
        if stock_hex not in peers:
            return fail(profile, stages, "stock-absent-from-java-floodfill-peer-set")
        stages["stock_in_floodfill_peer_set"] = 1
        if "selected_for_local_router_info=true" not in java_transport(
                java_diag, eligible_hex, time.monotonic() + 5):
            return fail(profile, stages, "eligible-not-selected-for-local-key")
        stages["eligible_selected_for_local_key"] = 1
        if "selected_for_local_router_info=true" not in java_transport(
                java_diag, stock_hex, time.monotonic() + 5):
            return fail(profile, stages, "stock-not-selected-for-local-key")
        stages["stock_selected_for_local_key"] = 1

        deadline = time.monotonic() + RUN_TIMEOUT
        _, succeeded = wait_store_effect(java_diag, java_publish, deadline)
        if not succeeded:
            return fail(profile, stages, "java-stock-store-job-not-successful")
        stages["java_store_job_succeeded"] = 1
        # Same close-flush note as profile B: stop Java before
        # counting its log text.
        stop(java_holder[0])
        java_holder[0] = None
        java_text = java_runtime_log(java_data, java_log)
        # Same path note as profile B: the triggered job travels
        # StoreJob/OutNetMessage, so the stock send line is the
        # observable. Recipient follows from the daemon-side effect
        # (i2pr) or its absence with a successful job (stock).
        if count_in(java_text, "Attempting to send on transport NTCP") < 1:
            return fail(profile, stages, "java-no-stock-transport-send")
        stages["java_stock_transport_send"] = 1
        eligible_text = (work / "eligible.log").read_text(errors="replace")
        if count_in(eligible_text,
                    "authenticated floodfill DatabaseStore accepted with response owed") >= 1:
            recipient = "eligible-i2pr-candidate"
        else:
            recipient = "stock-peer"
        stages["store_recipient_attributed"] = 1
        emit_receipt(profile, stages, {
            "recipient": recipient,
            "stock_tier_observed": str(stock_caps.get("bandwidth_tier", "unknown")),
        })
        return 0
    except (RuntimeError, TimeoutError, AssertionError, subprocess.CalledProcessError) as exc:
        return fail(profile, stages, type(exc).__name__ + ":" + str(exc)[:120])
    finally:
        stop(stock_holder[0])
        stop(eligible_holder[0])
        stop(java_holder[0])


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--profile", choices=("a", "b", "all"), default="all")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[3]
    java_cache, _, i2pd_cache, daemon, interop = check_environment(root)
    keep = os.environ.get("I2PR_PLAN479_KEEP_SCRATCH", "0") == "1"
    results: dict[str, int] = {}
    with tempfile.TemporaryDirectory(prefix="i2pr-plan479-") as temporary:
        work = Path(temporary)
        os.chmod(work, 0o700)
        if args.profile in ("b", "all"):
            with tempfile.TemporaryDirectory(prefix="i2pr-plan479-B-", dir=str(work)) as b_tmp:
                results["b"] = run_profile_b(
                    root, java_cache, daemon, interop, Path(b_tmp))
                if results["b"] != 0 and keep:
                    snap = (root / "target/interop/plan479-scratch-snaps"
                            / f"b-{int(time.time())}")
                    snap.parent.mkdir(parents=True, exist_ok=True)
                    import shutil as _shutil

                    _shutil.copytree(b_tmp, snap, symlinks=True)
                    print(f"plan479: scratch snapshot at {snap}")
        if args.profile in ("a", "all"):
            with tempfile.TemporaryDirectory(prefix="i2pr-plan479-A-", dir=str(work)) as a_tmp:
                results["a"] = run_profile_a(
                    root, java_cache, i2pd_cache / "bin" / "i2pd",
                    daemon, interop, Path(a_tmp))
                if results["a"] != 0 and keep:
                    snap = (root / "target/interop/plan479-scratch-snaps"
                            / f"a-{int(time.time())}")
                    snap.parent.mkdir(parents=True, exist_ok=True)
                    import shutil as _shutil

                    _shutil.copytree(a_tmp, snap, symlinks=True)
                    print(f"plan479: scratch snapshot at {snap}")
    print("plan479_profile_results=" + json.dumps(
        results, sort_keys=True, separators=(",", ":")))
    return 0 if results and all(rc == 0 for rc in results.values()) else 1


if __name__ == "__main__":
    raise SystemExit(main())
