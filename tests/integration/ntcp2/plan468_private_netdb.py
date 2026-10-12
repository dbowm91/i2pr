#!/usr/bin/env python3
"""Build a frozen five-router private floodfill NetDB for Plan 468."""

from __future__ import annotations

import base64
from datetime import datetime, timezone
import argparse
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


PIN = "635b013a612ff47278ef02acf8580a28e10e26c5"
PASSWORD = "plan468-controlled-local-test-only"
STOCK_COUNT = 4
STARTUP_TIMEOUT = 60
NETDB_READY = re.compile(r"NetDb: (\d+) routers loaded \((\d+) floodfils\)")
STOCK_PUBLISH = re.compile(r"Router: Publishing our RouterInfo to ([A-Za-z0-9~-]{4})\. reply token=(\d+)")


def parse_netdb_inventory(log_text: str) -> tuple[int, int] | None:
    """Return the latest exact pinned-source inventory receipt, if present."""
    matches = list(NETDB_READY.finditer(log_text))
    if not matches:
        return None
    return tuple(map(int, matches[-1].groups()))


def reserve(kind: int) -> int:
    with socket.socket(socket.AF_INET, kind) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def stop(child: subprocess.Popen[bytes] | None) -> None:
    if child is None or child.poll() is not None:
        return
    child.send_signal(signal.SIGTERM)
    try:
        child.wait(timeout=10)
    except subprocess.TimeoutExpired:
        child.kill()
        child.wait(timeout=5)


def control(port: int, payload: dict[str, object]) -> dict[str, object]:
    connection = http.client.HTTPSConnection(
        "127.0.0.1", port, timeout=3,
        context=__import__("ssl")._create_unverified_context(),
    )
    connection.request("POST", "/", json.dumps(payload).encode(),
                       {"Content-Type": "application/json"})
    response = connection.getresponse()
    raw = response.read(1_100_000)
    connection.close()
    if response.status != 200:
        raise RuntimeError(f"I2PControl returned HTTP {response.status}")
    parsed = json.loads(raw)
    if "error" in parsed:
        raise RuntimeError("I2PControl rejected bounded fixture request")
    return parsed


def fetch_i2pr_router_info(port: int, interop: Path, path: Path, deadline: float) -> bytes:
    token: str | None = None
    last_fields = "router_info_unavailable"
    while time.monotonic() < deadline:
        try:
            if token is None:
                token = str(control(port, {
                    "jsonrpc": "2.0", "id": 1, "method": "Authenticate",
                    "params": {"API": 1, "Password": PASSWORD},
                })["result"]["Token"])
            result = control(port, {
                "jsonrpc": "2.0", "id": 2, "method": "RouterInfo",
                "params": {"Token": token, "i2p.router.info": None},
            })["result"]
            encoded = result.get("i2p.router.info")
            if isinstance(encoded, str) and encoded:
                standard = encoded.translate(str.maketrans("-~", "+/"))
                raw = base64.b64decode(standard + "=" * (-len(standard) % 4), validate=True)
                path.write_bytes(raw)
                summary = json.loads(subprocess.check_output(
                    [str(interop), "ntcp2", "router-info-summary", "--router-info", str(path)],
                    text=True,
                ))
                last_fields = json.dumps({key: summary.get(key) for key in (
                    "network_id", "version", "floodfill", "address_count", "addresses"
                )}, separators=(",", ":"))
                version_parts = tuple(int(part) for part in summary.get("version", "").split("."))
                if (summary.get("network_id") == "2"
                    and version_parts >= (0, 9, 62)
                    and summary.get("floodfill") is True
                    and summary.get("address_count") == 2):
                    return raw
        except (OSError, KeyError, TypeError, ValueError, RuntimeError):
            token = None
        time.sleep(0.2)
    raise TimeoutError(f"controlled i2pr eligible RouterInfo did not publish before deadline: {last_fields}")


def storage_path(datadir: Path, router_id: str) -> Path:
    safe = router_id.replace("/", "-").replace("\\", "-")
    return datadir / "netDb" / f"r{router_id[0]}" / f"routerInfo-{safe}.dat"


def daemon_cache_path(datadir: Path, router_hash: bytes) -> Path:
    return datadir / "netdb" / "routers" / router_hash.hex()


def stock_publish_target(
    router_hash: bytes,
    records: list[tuple[bytes, str, int, int, bytes]],
    date_utc: str,
) -> bytes | None:
    """Mirror pinned CreateRoutingKey + XOR/memcmp current-day selection."""
    if len(router_hash) != 32 or not re.fullmatch(r"\d{8}", date_utc):
        return None
    routing_key = hashlib.sha256(router_hash + date_utc.encode("ascii")).digest()
    # RouterContext::HandlePublishTimer adds the publishing floodfill's own
    # hash to m_PublishExcluded before calling GetClosestFloodfill.
    eligible = [
        identity_hash
        for _, _, _, _, identity_hash in records
        if identity_hash != router_hash
    ]
    return min(
        eligible,
        key=lambda peer: bytes(left ^ right for left, right in zip(routing_key, peer)),
        default=None,
    )


def stock_config(
    path: Path,
    tcp_port: int,
    udp_port: int,
    *,
    ssu2_enabled: bool = True,
    ntcp2_enabled: bool = True,
) -> None:
    path.write_text(f'''daemon = false
loglevel = debug
netid = 2
address4 = 127.0.0.1
host = 127.0.0.1
port = 0
ipv4 = true
ipv6 = false
nat = false
notransit = true
floodfill = true
reservedrange = false
bandwidth = L
[ssu2]
enabled = {str(ssu2_enabled).lower()}
published = true
port = {udp_port}
[ntcp2]
enabled = {str(ntcp2_enabled).lower()}
published = true
port = {tcp_port}
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
''', encoding="utf-8")
    path.chmod(0o600)


def wait_stock_ready(
    child: subprocess.Popen[bytes],
    log: Path,
    port: int,
    deadline: float,
    *,
    require_ntcp2: bool = True,
) -> tuple[int, int, str]:
    listener = f"NTCP2: Start listening v4 TCP port {port}"
    while time.monotonic() < deadline:
        if child.poll() is not None:
            raise RuntimeError("stock floodfill exited before startup inventory")
        text = log.read_text(errors="replace") if log.exists() else ""
        inventory = parse_netdb_inventory(text)
        if inventory is not None and (listener in text or not require_ntcp2):
            routers, floodfills = inventory
            return routers, floodfills, text
        time.sleep(0.1)
    raise TimeoutError("stock floodfill did not publish bounded startup inventory")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--stock-ssu2-only",
        action="store_true",
        help="disable stock NTCP2 while keeping SSU2 active for the Plan 461 family row",
    )
    parser.add_argument(
        "--request-driven",
        action="store_true",
        help="restart i2pr from a fresh private cache to qualify route-miss-triggered NTCP2 delivery",
    )
    arguments = parser.parse_args()
    root = Path(__file__).resolve().parents[3]
    i2pd = Path(os.environ.get(
        "I2PR_PLAN468_I2PD_BIN",
        root / "target/interop/cache/ssu2/i2pd" / PIN / "bin/i2pd",
    ))
    source = root / "target/interop/cache/ssu2/i2pd-source"
    if not i2pd.is_file() or not os.access(i2pd, os.X_OK):
        print("plan468: blocked; exact-pinned i2pd binary unavailable", file=sys.stderr)
        return 2
    if not source.is_dir() or subprocess.check_output(
        ["git", "-C", str(source), "rev-parse", "HEAD"], text=True
    ).strip() != PIN:
        print("plan468: blocked; exact-pinned source unavailable", file=sys.stderr)
        return 2
    if "i2pd version 2.61.0" not in subprocess.check_output(
        [str(i2pd), "--version"], stderr=subprocess.STDOUT, text=True
    ):
        print("plan468: blocked; stock binary does not match the source pin", file=sys.stderr)
        return 2

    subprocess.run(["cargo", "build", "--locked", "-p", "i2pr-daemon",
                    "-p", "i2pr-interop"], cwd=root, check=True)
    daemon, interop = root / "target/debug/i2pr", root / "target/debug/i2pr-interop"
    with tempfile.TemporaryDirectory(prefix="i2pr-plan468-") as temporary:
        work = Path(temporary)
        os.chmod(work, 0o700)
        daemon_data = work / "i2pr"
        daemon_data.mkdir(mode=0o700)
        tcp, udp, control_port = reserve(socket.SOCK_STREAM), reserve(socket.SOCK_DGRAM), reserve(socket.SOCK_STREAM)
        daemon_config = work / "i2pr.toml"
        daemon_config.write_text(f'''schema_version = 1
[router]
data_dir = {json.dumps(str(daemon_data))}
[transport.ntcp2]
enabled = true
qualification_profile = "controlled_loopback"
bind_address = "127.0.0.1"
listen_port = {tcp}
[ssu2]
enabled = true
bind_ipv4 = "127.0.0.1"
bind_ipv6 = ""
port = {udp}
[floodfill]
enabled = true
qualification_profile = "controlled_loopback"
send_reply_token_probe = true
send_initiator_lookup_probe = {str(not arguments.stock_ssu2_only).lower()}
[i2pcontrol]
enabled = true
bind_address = "127.0.0.1"
port = {control_port}
password = {json.dumps(PASSWORD)}
''', encoding="utf-8")
        daemon_config.chmod(0o600)
        subprocess.run([str(daemon), "identity", "generate", "--config", str(daemon_config)],
                       cwd=root, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        daemon_log = work / "i2pr.log"
        daemon_child: subprocess.Popen[bytes] | None = None
        stock_children: list[subprocess.Popen[bytes]] = []
        try:
            with daemon_log.open("wb") as output:
                daemon_child = subprocess.Popen([str(daemon), "run", "--config", str(daemon_config)],
                                                cwd=root, stdout=output, stderr=subprocess.STDOUT)
                activation_end = time.monotonic() + STARTUP_TIMEOUT
                while time.monotonic() < activation_end:
                    if daemon_child.poll() is not None:
                        raise RuntimeError("i2pr controlled floodfill exited during startup")
                    if "floodfill qualification activated" in daemon_log.read_text(errors="replace"):
                        break
                    time.sleep(0.1)
                else:
                    raise TimeoutError("i2pr controlled floodfill role did not activate")
                candidate_path = work / "candidate.routerInfo"
                try:
                    candidate = fetch_i2pr_router_info(
                        control_port, interop, candidate_path, time.monotonic() + 30
                    )
                except TimeoutError:
                    daemon_text = daemon_log.read_text(errors="replace")
                    if "inspection string over ceiling for i2p.router.info" in daemon_text:
                        inspection_reason = "over_bound"
                    elif "inspection local RouterInfo base64 malformed" in daemon_text:
                        inspection_reason = "malformed_base64"
                    elif "controlled floodfill RouterInfo base64 encoding failed" in daemon_text:
                        inspection_reason = "encoder_failure"
                    else:
                        inspection_reason = "unclassified"
                    print(
                        "plan468_i2pr_publish_receipt=" + json.dumps({
                            "floodfill_active": "floodfill qualification activated" in daemon_text,
                            "inspection_publish_rejected": "controlled floodfill RouterInfo inspection publication rejected" in daemon_text,
                            "local_info_install_failed": "shared NTCP2 RouterInfo failed SSU2 installation" in daemon_text,
                            "floodfill_snapshot_published": "controlled floodfill RouterInfo inspection snapshot published" in daemon_text,
                            "updated_snapshot_published": "updated local RouterInfo inspection snapshot published" in daemon_text,
                            "inspection_publish_reason": inspection_reason,
                        }, separators=(",", ":")),
                        flush=True,
                    )
                    raise
                candidate_info = json.loads(subprocess.check_output(
                    [str(interop), "ntcp2", "router-hash", "--router-info", str(candidate_path)],
                    cwd=root, text=True,
                ))
                candidate_hash = candidate_info["router_hash_sha256"]
                candidate_ident = (base64.b64encode(bytes.fromhex(candidate_hash)).decode()
                                   .replace("+", "-").replace("/", "~").rstrip("="))
                candidate_summary = json.loads(subprocess.check_output(
                    [str(interop), "ntcp2", "router-info-summary", "--router-info", str(candidate_path)],
                    cwd=root, text=True,
                ))
                print(
                    "plan468_candidate_static_fields="
                    + json.dumps({key: candidate_summary.get(key) for key in (
                        "network_id", "version", "floodfill", "address_count", "addresses"
                    )}, separators=(",", ":")),
                    flush=True,
                )
                version_parts = tuple(int(part) for part in candidate_summary.get("version", "").split("."))
                if (candidate_summary.get("network_id") != "2"
                    or version_parts < (0, 9, 62)
                    or candidate_summary.get("floodfill") is not True
                    or candidate_summary.get("address_count") != 2):
                    raise RuntimeError("candidate failed static netId/version/caps/address preflight")

                # This first process exists only to create and inspect the
                # signed local RouterInfo. The qualification process starts
                # later from a persisted stock RouterInfo cache with no live
                # peer sessions, so its product lookup must cause the dial.
                if arguments.request_driven:
                    stop(daemon_child)
                    daemon_child = None
                    output.close()
                    daemon_log.write_text("", encoding="utf-8")

                # First run each independent stock owner normally so it creates
                # its own signed, live floodfill RouterInfo and transport keys.
                stock_records: list[tuple[bytes, int, int, bytes]] = []
                for index in range(STOCK_COUNT):
                    data = work / f"stock-{index}"
                    data.mkdir(mode=0o700)
                    port = reserve(socket.SOCK_STREAM)
                    udp_port = reserve(socket.SOCK_DGRAM)
                    config = work / f"stock-{index}.conf"
                    stock_config(config, port, udp_port)
                    log = work / f"stock-{index}.log"
                    with (work / f"stock-{index}.stdout").open("wb") as output:
                        child = subprocess.Popen([str(i2pd), f"--conf={config}", f"--datadir={data}",
                                                  "--log=file", f"--logfile={log}"], cwd=work,
                                                 stdout=output, stderr=subprocess.STDOUT)
                        try:
                            deadline = time.monotonic() + STARTUP_TIMEOUT
                            while time.monotonic() < deadline:
                                if child.poll() is not None:
                                    raise RuntimeError("stock owner exited while creating RouterInfo")
                                router_info = data / "router.info"
                                if router_info.is_file() and router_info.stat().st_size:
                                    text = log.read_text(errors="replace") if log.exists() else ""
                                    if f"NTCP2: Start listening v4 TCP port {port}" in text:
                                        break
                                time.sleep(0.1)
                            else:
                                raise TimeoutError("stock owner did not create a listening floodfill")
                            raw = (data / "router.info").read_bytes()
                            ri_path = work / f"stock-{index}.routerInfo"
                            ri_path.write_bytes(raw)
                            validated = json.loads(subprocess.check_output(
                                [str(interop), "ntcp2", "router-hash", "--router-info", str(ri_path)],
                                cwd=root, text=True,
                            ))
                            stock_summary = json.loads(subprocess.check_output(
                                [str(interop), "ntcp2", "router-info-summary", "--router-info", str(ri_path)],
                                cwd=root, text=True,
                            ))
                            styles = {address["style"] for address in stock_summary["addresses"]}
                            if (stock_summary.get("network_id") != "2"
                                or tuple(int(part) for part in stock_summary.get("version", "").split(".")) < (0, 9, 62)
                                or stock_summary.get("floodfill") is not True
                                or styles != {"SSU2", "NTCP2"}
                                or not all(address["loopback"] and address["port_valid"]
                                           and address["static_key"] and address["i_key"]
                                           for address in stock_summary["addresses"])):
                                raise RuntimeError("stock owner RouterInfo failed dual-transport floodfill preflight")
                            ident = (base64.b64encode(bytes.fromhex(validated["router_hash_sha256"])).decode()
                                     .replace("+", "-").replace("/", "~").rstrip("="))
                            stock_hash = bytes.fromhex(validated["router_hash_sha256"])
                            stock_records.append((raw, port, udp_port, stock_hash))
                            if not ident:
                                raise RuntimeError("stock RouterIdentity hash was empty")
                        finally:
                            stop(child)

                candidate_hash_bytes = bytes.fromhex(candidate_hash)
                records: list[tuple[bytes, str, int, int, bytes]] = [
                    (candidate, candidate_ident, tcp, udp, candidate_hash_bytes)
                ]
                for index, (raw, port, udp_port, stock_hash) in enumerate(stock_records):
                    ri_path = work / f"stock-{index}.routerInfo"
                    info = json.loads(subprocess.check_output(
                        [str(interop), "ntcp2", "router-hash", "--router-info", str(ri_path)],
                        cwd=root, text=True,
                    ))
                    ident = (base64.b64encode(bytes.fromhex(info["router_hash_sha256"])).decode()
                             .replace("+", "-").replace("/", "~").rstrip("="))
                    records.append((raw, ident, port, udp_port, stock_hash))
                if len(records) != 5 or len({identity for _, identity, _, _, _ in records}) != 5:
                    raise RuntimeError("private fixture does not contain five independent identities")

                date_utc = datetime.now(timezone.utc).strftime("%Y%m%d")
                preselected = [
                    index for index, (_, _, _, _, own_hash) in enumerate(records[1:])
                    if stock_publish_target(own_hash, records, date_utc) == candidate_hash_bytes
                ]

                if arguments.request_driven and not arguments.stock_ssu2_only:
                    for raw, _, _, _, router_hash in records[1:2]:
                        target = daemon_cache_path(daemon_data, router_hash)
                        target.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
                        target.parent.chmod(0o700)
                        target.write_bytes(raw)
                        target.chmod(0o600)

                # Four independent source-pinned stock owners each load the
                # exact same five signed, generated RouterInfos before NetDb::Start.
                healthy_owners: list[tuple[subprocess.Popen[bytes], Path, int]] = []
                owner_records = records[1:2] if arguments.request_driven else records[1:]
                for index, (_, own_ident, own_port, own_udp_port, _) in enumerate(owner_records):
                    data = work / f"stock-{index}"
                    owner_inventory = records[1:] if arguments.request_driven else records
                    for raw, identity, _, _, _ in owner_inventory:
                        target = storage_path(data, identity)
                        target.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
                        target.write_bytes(raw)
                        target.chmod(0o600)
                    config = work / f"healthy-{index}.conf"
                    # Keep the live signed inventory dual-transport, but make
                    # the publisher's actual new connection unambiguously
                    # NTCP2 for Plan 469. This is an ordinary stock config
                    # choice, not a RouterInfo mutation or source patch.
                    stock_config(
                        config,
                        own_port,
                        own_udp_port,
                        ssu2_enabled=arguments.stock_ssu2_only,
                        ntcp2_enabled=not arguments.stock_ssu2_only,
                    )
                    log = work / f"healthy-{index}.log"
                    child = subprocess.Popen([str(i2pd), f"--conf={config}", f"--datadir={data}",
                                              "--log=file", f"--logfile={log}"], cwd=work,
                                             stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                    stock_children.append(child)
                    healthy_owners.append((child, log, index))
                    routers, floodfills, startup_log = wait_stock_ready(
                        child,
                        log,
                        own_port,
                        time.monotonic() + STARTUP_TIMEOUT,
                        require_ntcp2=not arguments.stock_ssu2_only,
                    )
                    expected_owner_inventory = 4 if arguments.request_driven else 5
                    if routers != expected_owner_inventory or floodfills != expected_owner_inventory:
                        rejected_candidate = f"routerInfo-{candidate_ident}" in startup_log and "invalid or too old" in startup_log
                        rejected_stock = any(
                            f"routerInfo-{identity}" in startup_log and "invalid or too old" in startup_log
                            for _, identity, _, _, _ in records[1:]
                        )
                        source_checks = {
                            "signature": "Signature verification failed",
                            "malformed": "Malformed message",
                            "netid": "Unexpected netId",
                            "missing_required": "invalid or too old",
                            "address": "Unexpected value",
                            "identity": "Identity length",
                        }
                        source_reasons = ",".join(
                            name for name, marker in source_checks.items() if marker in startup_log
                        ) or "none"
                        detail_categories: list[str] = []
                        for line in startup_log.splitlines():
                            if "RouterInfo:" not in line:
                                continue
                            if "Signature verification failed" in line:
                                detail_categories.append("signature_invalid")
                            elif "Malformed message" in line:
                                detail_categories.append("message_malformed")
                            elif "Unexpected netId" in line:
                                detail_categories.append("network_id_mismatch")
                            elif "too long" in line:
                                detail_categories.append("record_oversize")
                            elif "Unexpected value" in line:
                                detail_categories.append("address_value_invalid")
                            elif "Unexpected field" in line:
                                detail_categories.append("address_field_unsupported")
                            elif "conversion error" in line:
                                detail_categories.append("address_number_invalid")
                            else:
                                detail_categories.append("router_info_parse_warning")
                        raise RuntimeError(
                            "stock startup inventory admitted "
                            f"routers={routers},floodfils={floodfills}; "
                            f"expected exactly {expected_owner_inventory} each; "
                            f"candidate_rejected={int(rejected_candidate)},stock_record_rejected={int(rejected_stock)}; "
                            f"source_diagnostic_categories={source_reasons}; "
                            f"router_info_diagnostic_categories={','.join(sorted(set(detail_categories)))}"
                        )
                    if not arguments.request_driven and not storage_path(data, candidate_ident).is_file():
                        raise RuntimeError("candidate RouterInfo missing from exact stock storage path")
                    if not arguments.stock_ssu2_only:
                        with socket.create_connection(("127.0.0.1", own_port), timeout=2):
                            pass

                if arguments.request_driven:
                    # The stock peer is already listening, while its normal
                    # first publish timer has not yet fired. Starting i2pr in
                    # this window avoids a connect race without giving it an
                    # established inbound route before the product lookup.
                    with daemon_log.open("ab") as daemon_output:
                        daemon_child = subprocess.Popen(
                            [str(daemon), "run", "--config", str(daemon_config)],
                            cwd=root,
                            stdout=daemon_output,
                            stderr=subprocess.STDOUT,
                        )
                    activation_end = time.monotonic() + STARTUP_TIMEOUT
                    while time.monotonic() < activation_end:
                        if daemon_child.poll() is not None:
                            raise RuntimeError("qualification i2pr daemon exited during startup")
                        daemon_text = daemon_log.read_text(errors="replace")
                        if "floodfill qualification activated" in daemon_text:
                            break
                        time.sleep(0.1)
                    else:
                        raise TimeoutError("qualification i2pr daemon did not activate")

                # Stock's normal initial RouterContext::Publish fires after
                # ten seconds. Observe exactly this bounded first operation;
                # its target selection is precomputed from the pinned source.
                publish_deadline = time.monotonic() + 12
                publish_observations: dict[int, tuple[str, bool]] = {}
                abbrev_categories: dict[str, list[str]] = {}
                for index, (_, identity, _, _, identity_hash) in enumerate(records):
                    category = "candidate" if identity_hash == candidate_hash_bytes else "stock"
                    abbrev = identity[:4]
                    abbrev_categories.setdefault(abbrev, []).append(category)
                while time.monotonic() < publish_deadline and len(publish_observations) < len(healthy_owners):
                    for child, log, index in healthy_owners:
                        if index in publish_observations:
                            continue
                        text = log.read_text(errors="replace") if log.exists() else ""
                        matches = list(STOCK_PUBLISH.finditer(text))
                        if matches:
                            target = matches[-1].group(1)
                            categories = abbrev_categories.get(target, [])
                            category = categories[0] if len(categories) == 1 else "ambiguous"
                            no_tunnel = "Can't publish our RouterInfo. No tunnels." in text
                            publish_observations[index] = (category, no_tunnel)
                        elif child.poll() is not None:
                            publish_observations[index] = ("process_exited", False)
                    if len(publish_observations) < STOCK_COUNT:
                        time.sleep(0.1)

                observed_candidate_publish = sum(
                    target == "candidate" and not no_tunnel
                    for target, no_tunnel in publish_observations.values()
                )
                observed_other_publish = sum(
                    target == "stock" and not no_tunnel
                    for target, no_tunnel in publish_observations.values()
                )
                observed_no_tunnel = sum(
                    no_tunnel for _, no_tunnel in publish_observations.values()
                )
                owner_confirmations = {
                    index: "Router: Publishing confirmed." in log.read_text(errors="replace")
                    for _, log, index in healthy_owners if log.exists()
                }
                candidate_confirmed = sum(
                    target == "candidate" and owner_confirmations.get(index, False)
                    for index, (target, _) in publish_observations.items()
                )
                other_confirmed = sum(
                    target == "stock" and owner_confirmations.get(index, False)
                    for index, (target, _) in publish_observations.items()
                )
                lookup_deadline = time.monotonic() + (95 if arguments.request_driven else 125)
                daemon_text = ""
                while not arguments.stock_ssu2_only and time.monotonic() < lookup_deadline:
                    daemon_text = daemon_log.read_text(errors="replace")
                    if (
                        "controlled NTCP2 initiator DatabaseLookup response correlated"
                        in daemon_text
                    ) or (not arguments.request_driven and observed_candidate_publish == 0):
                        break
                    time.sleep(0.2)
                daemon_text = daemon_log.read_text(errors="replace")
                outbound_links = daemon_text.count("NTCP2 authenticated outbound link promoted")
                bootstrap_stores_applied = daemon_text.count(
                    "authenticated RouterInfo store applied to bootstrap peer store"
                )
                ssu2_stores_applied = sum(
                    re.search(r'transport_family="?ssu2"?', line) is not None
                    and (
                        "authenticated RouterInfo store applied to bootstrap peer store" in line
                        or "authenticated floodfill DatabaseStore accepted with response owed" in line
                    )
                    for line in daemon_text.splitlines()
                )
                selected_ssu2_deliveries = sum(
                    re.search(r'transport_family="?ssu2"?', line) is not None
                    and "authenticated floodfill I2NP reply delivered" in line
                    for line in daemon_text.splitlines()
                )
                selected_ntcp2_deliveries = sum(
                    re.search(r'transport_family="?ntcp2"?', line) is not None
                    and "authenticated floodfill I2NP reply delivered" in line
                    for line in daemon_text.splitlines()
                )
                outbound_snapshots = [
                    int(value) for value in re.findall(
                        r"validated_peer_records=(\d+)", daemon_text
                    )
                ]
                outbound_dial_failures = daemon_text.count(
                    "controlled NTCP2 outbound peer dial attempt did not authenticate"
                )
                outbound_failure_stages = re.findall(
                    r"stage=\"([a-z_]+)\"",
                    daemon_text,
                )
                outbound_rejection_stages = re.findall(
                    r"stage=\"([a-z_]+)\"[^\n]*controlled NTCP2 outbound dial rejected",
                    daemon_text,
                )
                initiator_lookup_submitted = daemon_text.count(
                    "controlled request-driven DatabaseLookup submitted over authenticated NTCP2"
                )
                initiator_lookup_route_misses = daemon_text.count(
                    "controlled product DatabaseLookup requested a peer dial after route miss"
                )
                request_probe_markers = {
                    "scheduled": "controlled request-driven NTCP2 lookup worker scheduled",
                    "started": "controlled request-driven NTCP2 lookup worker started",
                    "no_bootstrap": "controlled request-driven NTCP2 lookup has no bootstrap owner",
                    "no_local_info": "controlled request-driven NTCP2 lookup has no published local RouterInfo",
                    "no_eligible_peer": "controlled request-driven NTCP2 lookup found no eligible peer",
                    "no_miss": "controlled request-driven NTCP2 lookup did not begin with a route miss",
                    "dial_timeout": "controlled request-driven NTCP2 peer dial did not complete",
                    "submitted": "controlled request-driven DatabaseLookup submitted over authenticated NTCP2",
                }
                initiator_lookup_correlated = daemon_text.count(
                    "controlled NTCP2 initiator DatabaseLookup response correlated"
                )
                lookup_submitted_generations = [
                    int(match.group(1))
                    for line in daemon_text.splitlines()
                    if "controlled request-driven DatabaseLookup submitted over authenticated NTCP2" in line
                    and (match := re.search(r"link_generation=(\d+)", line)) is not None
                ]
                lookup_correlated_generations = [
                    int(match.group(1))
                    for line in daemon_text.splitlines()
                    if "controlled NTCP2 initiator DatabaseLookup response correlated" in line
                    and (match := re.search(r"link_generation=(\d+)", line)) is not None
                ]
                inbound_links = daemon_text.count("NTCP2 authenticated inbound link promoted")
                forwarded_messages = daemon_text.count(
                    "authenticated NTCP2 I2NP message forwarded to router ingress"
                )
                accepted_stores = daemon_text.count(
                    "authenticated floodfill DatabaseStore accepted with response owed"
                )
                submitted_responses = daemon_text.count(
                    "authenticated floodfill DatabaseStore response submitted"
                )
                daemon_probe_submitted = daemon_text.count(
                    "controlled NTCP2 RouterInfo reply-token probe submitted"
                )
                daemon_probe_correlated = daemon_text.count(
                    "controlled NTCP2 RouterInfo DeliveryStatus correlation completed"
                )
                owner_receipt_categories: list[str] = []
                for _, log, index in healthy_owners:
                    if index not in publish_observations:
                        owner_receipt_categories.append("missing")
                        continue
                    target, _ = publish_observations[index]
                    owner_receipt_categories.append(
                        target + ("-confirmed" if owner_confirmations.get(index, False) else "-unconfirmed")
                    )
                minimum_receipts = 1 if arguments.stock_ssu2_only else observed_candidate_publish
                request_dial_complete = (
                    initiator_lookup_route_misses == 1
                    and initiator_lookup_submitted == 1
                    and initiator_lookup_correlated == 1
                    and len(lookup_submitted_generations) == 1
                    and lookup_submitted_generations[0] in lookup_correlated_generations
                    and outbound_links > 0
                )
                stock_product_chain_complete = observed_candidate_publish > 0 and not (
                    (
                        candidate_confirmed == 0
                        if arguments.stock_ssu2_only
                        else candidate_confirmed != observed_candidate_publish
                    )
                    or (
                        ssu2_stores_applied < minimum_receipts
                        if arguments.stock_ssu2_only
                        else inbound_links < minimum_receipts
                    )
                    or (
                        ssu2_stores_applied
                        if arguments.stock_ssu2_only
                        else forwarded_messages
                    ) < minimum_receipts
                    or accepted_stores < minimum_receipts
                    or submitted_responses < minimum_receipts
                    or (
                        selected_ssu2_deliveries < minimum_receipts
                        if arguments.stock_ssu2_only
                        else (
                            daemon_probe_submitted < minimum_receipts
                            or daemon_probe_correlated < minimum_receipts
                            or outbound_links == 0
                            or initiator_lookup_submitted == 0
                            or initiator_lookup_correlated == 0
                            or len(lookup_submitted_generations) != 1
                            or lookup_submitted_generations[0] not in lookup_correlated_generations
                        )
                    )
                )
                chain_complete = request_dial_complete if arguments.request_driven else stock_product_chain_complete
                print("plan468_reference_pin=635b013a612ff47278ef02acf8580a28e10e26c5")
                print("plan468_candidate_ri_present=1")
                print("plan468_candidate_ri_valid=1")
                print("plan468_candidate_static_preflight=1")
                print("plan468_candidate_role_admitted=1")
                print("plan468_stock_owners_active=" + str(len(healthy_owners)))
                print("plan468_stock_ssu2_owners_active=" + str(len(healthy_owners) if arguments.stock_ssu2_only else 0))
                print("plan468_stock_ssu2_eligible_inventory_records=4")
                print("plan468_loaded_routers_per_stock_owner=5")
                print("plan468_loaded_floodfils_per_stock_owner=5")
                print("plan468_candidate_stock_inventory_member=1")
                print("plan468_stock_selector_precomputed=1")
                print("plan468_stock_selector_candidate_selected_by_owner=" + str(len(preselected)))
                print("plan468_precomputed_other_floodfill_target_count=" + str(STOCK_COUNT - len(preselected)))
                print("plan468_stock_publish_receipts=" + str(len(publish_observations)))
                print("plan468_stock_publish_target_candidate=" + str(observed_candidate_publish))
                print("plan468_stock_publish_target_other=" + str(observed_other_publish))
                print("plan468_stock_publish_no_tunnel=" + str(observed_no_tunnel))
                print("plan468_stock_publish_confirmed=" + str(candidate_confirmed + other_confirmed))
                print("plan469_candidate_publish_confirmed=" + str(candidate_confirmed))
                print("plan469_other_publish_confirmed=" + str(other_confirmed))
                print("plan469_fresh_i2pr_ntcp2_inbound=" + str(inbound_links))
                print("plan459_fresh_i2pr_ntcp2_outbound=" + str(outbound_links))
                print("plan459_bootstrap_peer_records_applied=" + str(bootstrap_stores_applied))
                print("plan461_ssu2_bootstrap_records_applied=" + str(ssu2_stores_applied))
                print("plan461_ssu2_product_deliveries=" + str(selected_ssu2_deliveries))
                print("plan461_ntcp2_product_deliveries=" + str(selected_ntcp2_deliveries))
                print("plan459_max_outbound_peer_snapshot=" + str(max(outbound_snapshots, default=0)))
                print("plan459_outbound_dial_failures=" + str(outbound_dial_failures))
                print("plan459_outbound_failure_stages=" + ",".join(outbound_failure_stages))
                print("plan461_outbound_rejection_stages=" + ",".join(outbound_rejection_stages))
                print("plan459_database_lookup_submitted=" + str(initiator_lookup_submitted))
                print("plan459_database_lookup_correlated=" + str(initiator_lookup_correlated))
                print("plan461_request_route_misses=" + str(initiator_lookup_route_misses))
                for marker, phrase in request_probe_markers.items():
                    print(f"plan461_probe_{marker}=" + str(daemon_text.count(phrase)))
                no_peer_lines = [
                    line.strip()
                    for line in daemon_text.splitlines()
                    if "controlled request-driven NTCP2 lookup found no eligible peer" in line
                ]
                print("plan461_probe_peer_diagnostic=" + (no_peer_lines[-1] if no_peer_lines else "none"))
                print("plan461_request_link_generation=" + str(lookup_submitted_generations[0] if lookup_submitted_generations else 0))
                print("plan461_correlated_link_generations=" + ",".join(map(str, lookup_correlated_generations)))
                print("plan469_i2np_forwarded_to_product=" + str(forwarded_messages))
                print("plan469_product_store_accepted=" + str(accepted_stores))
                print("plan469_product_response_submitted=" + str(submitted_responses))
                print("plan467_daemon_probe_submitted=" + str(daemon_probe_submitted))
                print("plan467_daemon_probe_correlated=" + str(daemon_probe_correlated))
                print("plan468_private_netdb_ready=1")
                print("plan461_stock_ssu2_only=" + str(int(arguments.stock_ssu2_only)))
                print("plan461_request_driven_mode=" + str(int(arguments.request_driven)))
                print("plan469_owner_publish_receipts=" + ",".join(owner_receipt_categories))
                if not chain_complete:
                    print(
                        ("plan461_request_dial=" if arguments.request_driven
                         else "plan461_ssu2_product_chain=" if arguments.stock_ssu2_only
                         else "plan469_product_reply_chain=")
                        + "blocked; "
                        f"candidate_targeted={observed_candidate_publish},"
                        f"candidate_confirmed={candidate_confirmed},inbound={inbound_links},"
                        f"forwarded={forwarded_messages},stores={accepted_stores},"
                        f"responses={submitted_responses},daemon_probe="
                        f"{daemon_probe_submitted}/{daemon_probe_correlated},"
                        f"initiator_lookup={initiator_lookup_submitted}/"
                        f"{initiator_lookup_correlated}",
                        file=sys.stderr,
                    )
                    return 3
                return 0
        finally:
            for child in reversed(stock_children):
                stop(child)
            stop(daemon_child)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, subprocess.CalledProcessError, TimeoutError, ValueError) as error:
        print(f"plan468: blocked; {type(error).__name__}: {error}", file=sys.stderr)
        raise SystemExit(3)
