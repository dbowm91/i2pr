#!/usr/bin/env python3
"""Controlled normal-Java NTCP2 router-product qualification (Plan 470).

The Java Router is constructed through the pinned stock Router(Properties)
entrypoint. All router state is private to a temporary directory and all
addresses are loopback. Only sanitized stage counters are printed.
"""

from __future__ import annotations

import base64
import hashlib
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
JAVA_CONTROL_PASSWORD = "plan470-loopback-only"
START_TIMEOUT = 50
RUN_TIMEOUT = 45
MAX_ROUTER_INFO = 786_432


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


def wait_valid_router_info(
    interop: Path,
    path: Path,
    cwd: Path,
    deadline: float,
    expected_loopback_port: int | None = None,
) -> tuple[bytes, dict[str, object]]:
    while time.monotonic() < deadline:
        try:
            return router_info_hash(interop, path, cwd, expected_loopback_port)
        except (OSError, ValueError, subprocess.CalledProcessError, json.JSONDecodeError):
            time.sleep(0.1)
    raise TimeoutError("signed-router-info-not-stable")


def i2p_base64(raw: bytes) -> str:
    return base64.b64encode(raw).decode("ascii").replace("+", "-").replace("/", "~").rstrip("=")


def java_store_path(root: Path, digest: bytes) -> Path:
    ident = i2p_base64(digest)
    return root / "netDb" / f"r{ident[0]}" / f"routerInfo-{ident}.dat"


def daemon_store_path(root: Path, digest: bytes) -> Path:
    return root / "netdb" / "routers" / digest.hex()


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


def fetch_daemon_router_info(port: int, output: Path, deadline: float) -> tuple[bytes, dict[str, object]]:
    token: str | None = None
    last_error = "unavailable"
    while time.monotonic() < deadline:
        try:
            if token is None:
                token = str(control(port, {
                    "jsonrpc": "2.0", "id": 1, "method": "Authenticate",
                    "params": {"API": 1, "Password": JAVA_CONTROL_PASSWORD},
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
                return raw, result
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


def daemon_seed(seed_root: Path, raw: bytes, digest: bytes) -> None:
    path = daemon_store_path(seed_root, digest)
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
            if response.startswith(("P220-EV ", "P470-EV ", "P470-STORE ", "P470-LISTENER ", "P470-LOOKUP ")):
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


def wait_java_lookup_admission(port: int, child: subprocess.Popen[bytes], deadline: float) -> bool:
    while time.monotonic() < deadline:
        if child.poll() is not None:
            raise RuntimeError("java-router-exited-before-lookup-admission")
        if java_diagnostic(port, "P470-LOOKUP-THROTTLE", min(deadline, time.monotonic() + 1)) == "P470-LOOKUP ready=true":
            return True
        time.sleep(0.2)
    return False


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


def wait_java_candidate(
    port: int, digest: bytes, deadline: float
) -> tuple[bool, bool, bool, bool, bool, str]:
    expected = digest.hex()
    stored_present = False
    stored_f = False
    selector_available = False
    peer_admitted = False
    stock_transport_preflight = "unavailable"
    while time.monotonic() < deadline:
        stored = java_diagnostic(port, "P220-STORED-RI " + expected,
                                 min(deadline, time.monotonic() + 1))
        peers = java_diagnostic(port, "P220-PEERS-FLOODFILL",
                                min(deadline, time.monotonic() + 1))
        stored_present = "present=true" in stored
        stored_f = "has_floodfill_capability=true" in stored
        selector_available = peers != "unavailable"
        admitted = expected in peers
        peer_admitted = peer_admitted or admitted
        if admitted:
            stock_transport_preflight = java_diagnostic(
                port, "P470-TRANSPORT " + expected,
                min(deadline, time.monotonic() + 2),
            )
            if "floodfill_enabled=true" in stock_transport_preflight:
                return stored_present, stored_f, True, selector_available, True, stock_transport_preflight
        time.sleep(0.25)
    return stored_present, stored_f, peer_admitted, selector_available, False, stock_transport_preflight


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
    expected_loopback_port: int | None = None, require_floodfill: bool = False
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


def main() -> int:
    root = Path(__file__).resolve().parents[3]
    java_cache = Path(os.environ.get("I2PR_PLAN470_JAVA_CACHE", root / "target/interop/cache/m6-java" / JAVA_PIN))
    java_source = Path(os.environ.get("I2PR_PLAN470_JAVA_SOURCE", root / "target/interop/m6-java-sources" / f"i2p.i2p-{JAVA_PIN}"))
    if not (java_cache / "lib" / "router.jar").is_file():
        print("plan470: environment-unavailable: exact Java 2.13.0 cache absent", file=sys.stderr)
        return 2
    revision = subprocess.check_output(["git", "-C", str(java_source), "rev-parse", "HEAD"], text=True).strip()
    if revision != JAVA_PIN:
        print("plan470: exact Java source pin mismatch", file=sys.stderr)
        return 2

    daemon = root / "target/debug/i2pr"
    interop = root / "target/debug/i2pr-interop"
    subprocess.run(["cargo", "build", "--locked", "-p", "i2pr-daemon", "-p", "i2pr-interop"], cwd=root, check=True)
    if not daemon.is_file() or not interop.is_file():
        raise RuntimeError("workspace-binaries-missing")

    with tempfile.TemporaryDirectory(prefix="i2pr-plan470-") as temporary:
        work = Path(temporary)
        os.chmod(work, 0o700)
        java_data = work / "java"
        daemon_data = work / "i2pr"
        java_data.mkdir(mode=0o700)
        daemon_data.mkdir(mode=0o700)
        ntcp_port = reserve(socket.SOCK_STREAM)
        daemon_ntcp_port = reserve(socket.SOCK_STREAM)
        control_port = reserve(socket.SOCK_STREAM)
        java_diag_port = reserve(socket.SOCK_STREAM)
        java_publish_port = reserve(socket.SOCK_STREAM)
        config = work / "i2pr.toml"
        ssu_port = reserve(socket.SOCK_DGRAM)
        config.write_text(f'''schema_version = 1
[router]
data_dir = {json.dumps(str(daemon_data))}
[transport.ntcp2]
enabled = true
qualification_profile = "controlled_loopback"
bind_address = "127.0.0.1"
listen_port = {daemon_ntcp_port}
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
[i2pcontrol]
enabled = true
bind_address = "127.0.0.1"
port = {control_port}
password = {json.dumps(JAVA_CONTROL_PASSWORD)}
''', encoding="utf-8")
        config.chmod(0o600)
        subprocess.run([str(daemon), "identity", "generate", "--config", str(config)], cwd=root,
                       check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        daemon_log = work / "i2pr.log"
        java_log = work / "java.log"
        java_build = work / "java-build"
        java_build.mkdir(mode=0o700)
        java_src = root / "tests/integration/m6-interop/java"
        java_sources = [java_src / "ControlledRouter.java", java_src / "ReferenceRawDestination.java",
                        java_src / "ReferenceStreamingService.java"]
        java_sources.extend(sorted((java_src / "net/i2p/router/networkdb/kademlia").glob("*.java")))
        java_cp = ":".join([str(java_cache / "lib" / "*") , str(java_cache / "*")])
        subprocess.run(["javac", "-d", str(java_build), "-cp", java_cp, *map(str, java_sources)],
                       cwd=root, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        classpath = f"{java_build}:{java_cp}"
        daemon_child: subprocess.Popen[bytes] | None = None
        java_child: subprocess.Popen[bytes] | None = None
        stages = {"java_listener": 0, "signed_router_infos": 0,
                  "java_candidate_loaded": 0, "java_candidate_admitted": 0,
                  "java_candidate_selected": 0, "java_ntcp2_bid_available": 0,
                  "java_router_publish_eligible": 0,
                  "java_floodfill_active": 0,
                  "java_routerinfo_store_eligible": 0,
                  "java_candidate_not_banlisted": 0,
                  "java_ntcp_not_marked_unreachable": 0,
                  "java_store_job_submitted": 0,
                  "java_store_job_succeeded": 0,
                  "java_stock_transport_send": 0,
                  "java_outbound_tcp_connected": 0,
                  "java_lookup_admission_ready": 0,
                  "daemon_lookup_worker_scheduled": 0,
                  "daemon_lookup_submitted": 0, "daemon_lookup_correlated": 0,
                  "daemon_outbound_link_promoted": 0,
                  "daemon_probe_submitted": 0, "daemon_probe_correlated": 0,
                  "daemon_inbound_product_effect": 0}
        java_counters_before: dict[str, int] = {}
        java_counters_after: dict[str, int] = {}
        try:
            # Bootstrap only the real i2pr-signed RouterInfo. No peer or
            # product traffic is enabled during this preparation process.
            with daemon_log.open("wb") as output:
                daemon_child = subprocess.Popen([str(daemon), "run", "--config", str(config)], cwd=root,
                                                stdout=output, stderr=subprocess.STDOUT)
                daemon_raw_path = work / "i2pr.routerInfo"
                deadline = time.monotonic() + START_TIMEOUT
                while True:
                    daemon_raw, _ = fetch_daemon_router_info(control_port, daemon_raw_path, deadline)
                    daemon_hash, daemon_summary = router_info_hash(
                        interop, daemon_raw_path, root, daemon_ntcp_port
                    )
                    if daemon_summary.get("floodfill") is True:
                        break
                    if time.monotonic() >= deadline:
                        raise TimeoutError("controlled-floodfill-router-info-not-published")
                    time.sleep(0.2)
            stop(daemon_child)
            daemon_child = None
            assert_router(
                daemon_summary, label="i2pr", ntcp2_only=False,
                expected_loopback_port=daemon_ntcp_port, require_floodfill=True,
            )

            # Seed Java's actual persistent NetDB before starting its normal
            # Router. This is a signed, current i2pr RouterInfo, not a selector
            # result manufactured by the test helper.
            java_seed(java_data, daemon_raw, daemon_hash)
            with java_log.open("wb") as output:
                java_child = subprocess.Popen([
                    "java", "-Xms128m", "-Xmx512m", "-Di2p.dir.base=" + str(java_cache),
                    "-cp", classpath, "ControlledRouter", str(java_data), "127.0.0.1",
                    str(ntcp_port), "0", "0", str(java_diag_port), "ntcp2", str(java_publish_port),
                ], cwd=root, stdout=output, stderr=subprocess.STDOUT)
                # Do not open an empty connection to the NTCP listener:
                # stock Java blocks the source address after pre-data EOF.
                wait_java_listener(java_child, java_diag_port, time.monotonic() + START_TIMEOUT)
                # Java's RouterInfo/key material is written under
                # i2p.dir.router, which ControlledRouter sets to
                # <data-dir>/router. NetDB itself remains <data-dir>/netDb.
                java_ri_path = java_data / "router" / "router.info"
                deadline = time.monotonic() + START_TIMEOUT
                while not java_ri_path.is_file() and time.monotonic() < deadline:
                    if java_child.poll() is not None:
                        raise RuntimeError("java-router-exited-before-routerinfo")
                    time.sleep(0.1)
                java_raw = java_ri_path.read_bytes()
                if len(java_raw) > MAX_ROUTER_INFO:
                    raise RuntimeError("java-router-info-too-large")
            java_hash, java_summary = wait_valid_router_info(
                interop, java_ri_path, root, time.monotonic() + 15, ntcp_port
            )
            assert_router(
                java_summary, label="java", ntcp2_only=True,
                expected_loopback_port=ntcp_port,
            )
            stages["java_listener"] = 1
            stages["signed_router_infos"] = 2
            jhash = daemon_hash.hex()
            (candidate_present, candidate_has_f, candidate_admitted,
             selector_available, _, stock_transport_preflight) = wait_java_candidate(
                java_diag_port, daemon_hash, time.monotonic() + 15
            )
            stages["java_candidate_loaded"] = int(candidate_present and candidate_has_f)
            stages["java_candidate_admitted"] = int(candidate_admitted)
            stages["java_candidate_selected"] = int(
                "selected_for_local_router_info=true" in stock_transport_preflight
            )
            stages["java_ntcp2_bid_available"] = int(
                "ntcp2_bid_available=true" in stock_transport_preflight
                and "bid_uses_ntcp_transport=true" in stock_transport_preflight
            )
            stages["java_router_publish_eligible"] = int(
                "router_hidden=false" in stock_transport_preflight
            )
            stages["java_floodfill_active"] = int(
                "floodfill_enabled=true" in stock_transport_preflight
            )
            stages["java_routerinfo_store_eligible"] = int(
                "accepts_routerinfo_store=true" in stock_transport_preflight
            )
            stages["java_candidate_not_banlisted"] = int(
                "candidate_banlisted=false" in stock_transport_preflight
            )
            stages["java_ntcp_not_marked_unreachable"] = int(
                "ntcp_transport_unreachable=false" in stock_transport_preflight
            )
            print("plan470_java_selector_preflight=" + json.dumps({
                "candidate_present_in_main_netdb": candidate_present,
                "candidate_signed_f_capability": candidate_has_f,
                "candidate_loaded_with_f": bool(stages["java_candidate_loaded"]),
                "candidate_in_floodfill_peer_set": bool(stages["java_candidate_admitted"]),
                "selector_report_available": selector_available,
                "stock_selector_and_ntcp_bid": stock_transport_preflight,
            }, sort_keys=True, separators=(",", ":")))
            if not stages["java_candidate_admitted"] or not all(
                token in stock_transport_preflight
                for token in (
                    "observable=true", "candidate_present=true",
                    "selected_for_local_router_info=true",
                    "ntcp_transport_available=true", "ntcp2_bid_available=true",
                    "bid_uses_ntcp_transport=true", "router_hidden=false",
                    "floodfill_enabled=true", "accepts_routerinfo_store=true",
                    "candidate_banlisted=false", "ntcp_transport_unreachable=false",
                )
            ):
                missing = (
                    "java-floodfill-peer-admission"
                    if not stages["java_candidate_admitted"]
                    else (
                        "java-router-hidden-publication-gate"
                        if not stages["java_router_publish_eligible"]
                        else (
                            "java-floodfill-role-activation"
                            if not stages["java_floodfill_active"]
                            else (
                                "java-stock-routerinfo-store-policy"
                                if not stages["java_routerinfo_store_eligible"]
                                else (
                                    "java-peer-ban-or-unreachable-policy"
                                    if not stages["java_candidate_not_banlisted"]
                                    or not stages["java_ntcp_not_marked_unreachable"]
                                    else "java-stock-selector-or-ntcp2-bid"
                                )
                            )
                        )
                    )
                )
                print("plan470: first-missing-stage=" + missing, file=sys.stderr)
                return 1
            daemon_seed(daemon_data, java_raw, java_hash)
            java_store = java_store_path(java_data, daemon_hash)
            seeded_java_hash = hashlib.sha256(java_store.read_bytes()).hexdigest()
            daemon_store = daemon_store_path(daemon_data, java_hash)
            seeded_daemon_hash = hashlib.sha256(daemon_store.read_bytes()).hexdigest()

            # i2pr now knows the exact pinned Java RouterInfo and Java has
            # loaded i2pr's signed floodfill RouterInfo from persistent NetDB.
            with daemon_log.open("wb") as output:
                daemon_child = subprocess.Popen([str(daemon), "run", "--config", str(config)], cwd=root,
                                                stdout=output, stderr=subprocess.STDOUT)
                daemon_live_ri = work / "i2pr-live.routerInfo"
                ready_deadline = time.monotonic() + 35
                while True:
                    if daemon_child.poll() is not None:
                        print("plan470: daemon-exited-before-routerinfo-ready", file=sys.stderr)
                        return 1
                    fetch_daemon_router_info(control_port, daemon_live_ri, ready_deadline)
                    _, daemon_live_summary = router_info_hash(
                        interop, daemon_live_ri, root, daemon_ntcp_port
                    )
                    if daemon_live_summary.get("floodfill") is True:
                        break
                    if time.monotonic() >= ready_deadline:
                        print("plan470: controlled-floodfill-routerinfo-not-ready", file=sys.stderr)
                        return 1
                    time.sleep(0.2)
                assert_router(
                    daemon_live_summary, label="i2pr-live", ntcp2_only=False,
                    expected_loopback_port=daemon_ntcp_port, require_floodfill=True,
                )
                java_counters_before = java_send_counters(
                    java_diag_port, time.monotonic() + 3
                )
                stages["java_store_job_submitted"] = int(trigger_java_store_job(
                    java_publish_port, time.monotonic() + 15
                ))
                if not stages["java_store_job_submitted"]:
                    print("plan470: first-missing-stage=java-stock-reply-requested-store-job", file=sys.stderr)
                    return 1
                deadline = time.monotonic() + RUN_TIMEOUT
                while time.monotonic() < deadline:
                    if daemon_child.poll() is not None or java_child.poll() is not None:
                        raise RuntimeError("qualification-router-exited")
                    daemon_text = daemon_log.read_text(errors="replace")
                    java_text = java_runtime_log(java_data, java_log)
                    stages["java_stock_transport_send"] = int(
                        "Attempting to send on transport NTCP" in java_text
                    )
                    java_store_status = java_diagnostic(
                        java_diag_port, "P470-STORE-STATUS", min(deadline, time.monotonic() + 1)
                    )
                    stages["java_store_job_succeeded"] = int(
                        "P470-STORE submitted=1 succeeded=1 failed=0" in java_store_status
                    )
                    stages["java_outbound_tcp_connected"] = int(
                        "connected? true" in java_text
                    )
                    stages["daemon_probe_submitted"] = int(
                        "controlled NTCP2 RouterInfo reply-token probe submitted" in daemon_text
                    )
                    stages["daemon_probe_correlated"] = int(
                        "controlled NTCP2 RouterInfo DeliveryStatus correlation completed" in daemon_text
                    )
                    stages["daemon_inbound_product_effect"] = int(
                        "authenticated floodfill DatabaseStore accepted with response owed" in daemon_text
                    )
                    if (stages["java_store_job_succeeded"]
                            and stages["daemon_probe_correlated"]
                            and stages["daemon_inbound_product_effect"]):
                        break
                    time.sleep(0.2)
            daemon_text_phase1 = daemon_log.read_text(errors="replace")
            java_counters_after_phase1 = java_send_counters(
                java_diag_port, time.monotonic() + 3
            )
            stop(daemon_child)
            daemon_child = None

            # Java 2.13.0's normal LookupThrottler leaves max=0 until its
            # first 120-second cleaner tick. Poll the read-only stock-state
            # observer so phase 2 begins only after normal admission is live.
            stages["java_lookup_admission_ready"] = int(wait_java_lookup_admission(
                java_diag_port, java_child, time.monotonic() + 160
            ))
            if not stages["java_lookup_admission_ready"]:
                print("plan470: first-missing-stage=java-stock-lookup-throttle-initialization", file=sys.stderr)
                return 1

            # Start a separate process generation with the same signed router
            # identity and NetDB. The first process has been closed, so this
            # phase must create a fresh i2pr-initiated TCP/NTCP2 session.
            config_text = config.read_text(encoding="utf-8")
            if "send_initiator_lookup_probe = false" not in config_text:
                raise RuntimeError("controlled-lookup-phase-config-mismatch")
            config.write_text(
                config_text.replace(
                    "send_initiator_lookup_probe = false",
                    "send_initiator_lookup_probe = true",
                    1,
                ),
                encoding="utf-8",
            )
            config.chmod(0o600)
            with daemon_log.open("wb") as output:
                daemon_child = subprocess.Popen([str(daemon), "run", "--config", str(config)], cwd=root,
                                                stdout=output, stderr=subprocess.STDOUT)
                restart_deadline = time.monotonic() + START_TIMEOUT
                while True:
                    if daemon_child.poll() is not None:
                        raise RuntimeError("daemon-exited-before-fresh-initiator-phase")
                    fetch_daemon_router_info(
                        control_port, work / "i2pr-phase2.routerInfo", restart_deadline
                    )
                    if time.monotonic() >= restart_deadline:
                        raise TimeoutError("daemon-fresh-initiator-phase-startup")
                    break
                deadline = time.monotonic() + RUN_TIMEOUT
                while time.monotonic() < deadline:
                    if daemon_child.poll() is not None or java_child.poll() is not None:
                        raise RuntimeError("qualification-router-exited")
                    phase2_text = daemon_log.read_text(errors="replace")
                    stages["daemon_lookup_worker_scheduled"] = int(
                        "controlled request-driven NTCP2 lookup worker scheduled" in phase2_text
                    )
                    stages["daemon_lookup_submitted"] = int(
                        "controlled request-driven DatabaseLookup submitted over authenticated NTCP2" in phase2_text
                    )
                    stages["daemon_lookup_correlated"] = int(
                        "controlled NTCP2 initiator DatabaseLookup response correlated" in phase2_text
                    )
                    stages["daemon_outbound_link_promoted"] = int(
                        "NTCP2 authenticated outbound link promoted" in phase2_text
                    )
                    if (stages["daemon_lookup_worker_scheduled"]
                            and stages["daemon_lookup_submitted"]
                            and stages["daemon_outbound_link_promoted"]
                            and stages["daemon_lookup_correlated"]):
                        break
                    time.sleep(0.2)
            java_counters_after = java_send_counters(
                java_diag_port, time.monotonic() + 3
            )
            java_counter_deltas = {
                key: max(0, java_counters_after[key] - java_counters_before[key])
                for key in java_counters_before.keys() & java_counters_after.keys()
                if java_counters_before[key] >= 0 and java_counters_after[key] >= 0
            }
            java_phase2_counter_deltas = {
                key: max(0, java_counters_after[key] - java_counters_after_phase1[key])
                for key in java_counters_after_phase1.keys() & java_counters_after.keys()
                if java_counters_after_phase1[key] >= 0 and java_counters_after[key] >= 0
            }
            java_text = java_runtime_log(java_data, java_log)
            daemon_text = daemon_text_phase1 + "\n" + daemon_log.read_text(errors="replace")
            java_store_status = java_diagnostic(
                java_diag_port, "P470-STORE-STATUS", time.monotonic() + 2
            )
            # Do not persist or print raw router logs. Counts are stage-specific
            # and the identity/hash itself is omitted from the receipt.
            print("plan470_java_ntcp2_stages=" + json.dumps(stages, sort_keys=True, separators=(",", ":")))
            print("plan470_java_stock_send_counter_deltas=" + json.dumps(
                java_counter_deltas, sort_keys=True, separators=(",", ":")
            ))
            print("plan470_java_phase2_counter_deltas=" + json.dumps(
                java_phase2_counter_deltas, sort_keys=True, separators=(",", ":")
            ))
            print("plan470_java_store_job_status=" + java_store_status)
            print("plan470_reference_pin=" + JAVA_VERSION + "@" + JAVA_PIN)
            if not all(stages.values()):
                missing = next(k for k, v in stages.items() if not v)
                attribution = "plan470: first-missing-stage=" + missing
                if missing == "daemon_outbound_link_promoted" and stages.get("daemon_lookup_submitted"):
                    # Plan 479: a missing promotion with a submitted lookup
                    # is ambiguous between a stock-side connect-timeout
                    # storm and a daemon-side promotion failure. Attach
                    # the stock NTCP failure deltas so the exit-1 line
                    # classifies itself; -1 means unobservable, never zero.
                    attribution += " stock_ntcp_connect_failures=" + json.dumps({
                        key: java_phase2_counter_deltas.get(key, -1)
                        for key in (
                            "ntcp_connectFailedTimeout",
                            "ntcp_connectFailedTimeoutIOE",
                            "ntcp_connectFailedIOE",
                            "ntcp_outboundEstablishFailed",
                        )
                    }, sort_keys=True, separators=(",", ":"))
                print(attribution, file=sys.stderr)
                failure_stages = re.findall(
                    r'failure_stage=["\']?([a-z_]+)', daemon_text
                )
                handshake_stage_io = re.findall(
                    r'controlled NTCP2 outbound handshake stage.*?stage=["\']?([a-z_]+).*?'
                    r'io_result=["\']?([a-z-]+)',
                    daemon_text,
                )
                inbound_handshake_stage_io = re.findall(
                    r'controlled NTCP2 inbound handshake stage.*?stage=["\']?([a-z_]+).*?'
                    r'io_result=["\']?([a-z-]+)',
                    daemon_text,
                )
                token_counts = {
                    # Counts only: the stock flood lines contain RouterHashes,
                    # which must never leave the private scratch log.
                    "java_stock_flood_dispatch_candidates": java_text.count("Flooding the entry for "),
                    "java_stock_flood_dispatch_summary": java_text.count("Flooded the data to "),
                    "java_stock_transport_send_attempts": java_text.count("Attempting to send on transport "),
                    "java_stock_ntcp_send_attempts": java_text.count("Attempting to send on transport NTCP"),
                    "java_ntcp_bad_address_constructor_failures": java_text.count("bad address? "),
                    "java_ntcp_address_changed_before_connect": java_text.count(
                        "we bid on a peer with a bad address?"
                    ),
                    "java_ntcp_channel_open_failures": java_text.count("Error opening a channel"),
                    "java_ntcp_channel_registration_events": java_text.count("Registering "),
                    "java_ntcp_connect_stage_counts": {
                        token: java_text.count(token)
                        for token in (
                            "processing connect for", "connected? true", "connected? false",
                            "error connecting on", "error connecting to",
                            "unresolved address connecting", "Possible early disconnect",
                            "Closing connection", "Receive session request but near connection limit",
                        )
                    },
                    "java_inbound_handshake_gate_counts": {
                        token: java_text.count(token)
                        for token in (
                            "Receive session request from blocklisted IP",
                            "Blocking accept of IP with count",
                            "Probabalistic drop incoming",
                            "EOF on inbound before receiving any",
                            "Short buffer got",
                            "Bad PK msg 1",
                            "Bad msg 1",
                            "Bad network id",
                            "Bad version",
                            "Bad msg 3 payload",
                            "Failed msg3p2",
                            "Dropping inbound connection from wrong network",
                            "Clocks too skewed",
                            "verification successful for",
                            "Multiple connections on remove",
                            "Old connection closed",
                        )
                    },
                    "java_stock_flood_candidate_rejected": java_text.count("Too old, not flooding "),
                    "java_publish_logs": sum(java_text.count(token) for token in (
                        "Publishing our RI", "Delay publishing our RI", "Publishing our RI after delay"
                    )),
                    "java_log_files": sum(1 for path in (java_data / "logs").rglob("*") if path.is_file()),
                    "java_log_bytes": sum(path.stat().st_size for path in (java_data / "logs").rglob("*")
                                          if path.is_file()),
                    "java_ntcp2_logs": java_text.lower().count("ntcp2"),
                    "java_store_logs": java_text.lower().count("store"),
                    "java_lookup_drop_reasons": {
                        "direct_lookup_from_local_hash_without_transport_peer": java_text.count(
                            "direct lookup request from us"
                        ),
                        "direct_lookup_from_floodfill": java_text.count(
                            "direct lookup request from floodfill"
                        ),
                        "lookup_throttled": java_text.count("(throttled), reply was to:"),
                        "received_but_no_drop_warning": java_text.count("netDb.lookupsDropped"),
                    },
                    "java_router_logs": java_text.lower().count("routerinfo"),
                    "java_error_logs": java_text.lower().count("error") + java_text.lower().count("exception"),
                    "daemon_ntcp2_logs": daemon_text.lower().count("ntcp2"),
                    "daemon_handshake_logs": daemon_text.lower().count("handshake"),
                    "daemon_authentication_logs": daemon_text.lower().count("authenticat"),
                    "daemon_inbound_handshake_rejections": daemon_text.count(
                        "controlled NTCP2 inbound handshake rejected"
                    ),
                    "daemon_ntcp2_store_accept_logs": daemon_text.count(
                        "authenticated floodfill DatabaseStore accepted with response owed"
                    ),
                    "daemon_store_response_submitted": daemon_text.count(
                        "authenticated floodfill DatabaseStore response submitted"
                    ),
                    "daemon_probe_submitted_logs": daemon_text.count(
                        "controlled NTCP2 RouterInfo reply-token probe submitted"
                    ),
                    "daemon_lookup_worker_logs": daemon_text.count(
                        "controlled request-driven NTCP2 lookup worker scheduled"
                    ),
                    "daemon_lookup_submission_logs": daemon_text.count(
                        "controlled request-driven DatabaseLookup submitted over authenticated NTCP2"
                    ),
                    "daemon_outbound_link_logs": daemon_text.count(
                        "NTCP2 authenticated outbound link promoted"
                    ),
                    "daemon_outbound_dial_rejection_stages": dict(sorted(
                        (stage, len(re.findall(
                            rf'stage=[\"\']?{re.escape(stage)}(?:[\"\']|[, ])', daemon_text
                        )))
                        for stage in (
                            "router_info_signature", "network_id", "router_hash",
                            "router_hash_mismatch", "clock_unavailable",
                            "no_loopback_ntcp2_address", "tcp_connect", "ephemeral_key",
                            "initiator_state", "noise_handshake",
                            "authenticated_peer_mismatch", "link_promotion",
                        )
                    )),
                    "daemon_outbound_handshake_failure_stages": dict(sorted(
                        (stage, failure_stages.count(stage))
                        for stage in set(failure_stages)
                    )),
                    "daemon_outbound_handshake_stage_io": dict(sorted(
                        (f"{stage}:{io_result}", handshake_stage_io.count((stage, io_result)))
                        for stage, io_result in set(handshake_stage_io)
                    )),
                    "daemon_inbound_handshake_stage_io": dict(sorted(
                        (f"{stage}:{io_result}", inbound_handshake_stage_io.count((stage, io_result)))
                        for stage, io_result in set(inbound_handshake_stage_io)
                    )),
                    "java_ntcp2_log_tokens": {
                        token: java_text.lower().count(token)
                        for token in (
                            "ntcp", "databaselookup", "databasestore", "republish",
                            "rebuilding new routerinfo", "bad msg 1", "bad pk msg 1",
                            "bad version", "wrong network", "clock skew",
                            "session request", "connection established",
                        )
                    },
                }
                token_counts["daemon_inbound_failure_stages"] = dict(
                    sorted((stage, failure_stages.count(stage)) for stage in set(failure_stages))
                )
                print("plan470_sanitized_log_counts=" + json.dumps(token_counts, sort_keys=True, separators=(",", ":")))
                return 1
        finally:
            stop(daemon_child)
            stop(java_child)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
