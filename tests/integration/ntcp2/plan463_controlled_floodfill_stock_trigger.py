#!/usr/bin/env python3
"""Plan 463: pristine pinned i2pd Explore -> live i2pr floodfill reply."""

from __future__ import annotations

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


PIN = "635b013a612ff47278ef02acf8580a28e10e26c5"
WAIT_SECONDS = 180
PASSWORD = "plan463-controlled-local-test-only"


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


def request(port: int, payload: dict[str, object]) -> dict[str, object]:
    connection = http.client.HTTPSConnection(
        "127.0.0.1", port, timeout=3, context=__import__("ssl")._create_unverified_context()
    )
    body = json.dumps(payload).encode()
    connection.request("POST", "/", body, {"Content-Type": "application/json"})
    response = connection.getresponse()
    raw = response.read(1_100_000)
    connection.close()
    if response.status != 200:
        raise RuntimeError(f"I2PControl returned HTTP {response.status}")
    parsed = json.loads(raw)
    if "error" in parsed:
        raise RuntimeError("I2PControl rejected the bounded qualification request")
    return parsed


def wait_router_info(port: int, deadline: float) -> tuple[str | None, bytes]:
    token: str | None = None
    last_state = "control listener unavailable"
    while time.monotonic() < deadline:
        try:
            if token is None:
                auth = request(
                    port,
                    {
                        "jsonrpc": "2.0",
                        "id": 1,
                        "method": "Authenticate",
                        "params": {"API": 1, "Password": PASSWORD},
                    },
                )
                token = str(auth["result"]["Token"])
            result = request(
                port,
                {
                    "jsonrpc": "2.0",
                    "id": 2,
                    "method": "RouterInfo",
                    "params": {
                        "Token": token,
                        "i2p.router.info": None,
                        "i2p.router.id": None,
                    },
                },
            )["result"]
            encoded = result.get("i2p.router.info")
            router_id = result.get("i2p.router.id")
            if isinstance(encoded, str) and encoded:
                standard = encoded.translate(str.maketrans("-~", "+/"))
                raw = base64.b64decode(standard + "=" * (-len(standard) % 4), validate=True)
                return router_id if isinstance(router_id, str) else None, raw
            last_state = (
                "published fields present but values unavailable; "
                f"info={type(encoded).__name__}, id={type(router_id).__name__}"
            )
        except (OSError, KeyError, TypeError, ValueError, RuntimeError) as error:
            last_state = f"{type(error).__name__}: {error}"
        time.sleep(0.2)
    raise TimeoutError(f"controlled RouterInfo was not published before the deadline ({last_state})")


def main() -> int:
    root = Path(__file__).resolve().parents[3]
    i2pd = Path(
        os.environ.get(
            "I2PR_PLAN463_I2PD_BIN",
            root / "target/interop/cache/ssu2/i2pd" / PIN / "bin/i2pd",
        )
    )
    source = root / "target/interop/cache/ssu2/i2pd-source"
    if not i2pd.is_file() or not os.access(i2pd, os.X_OK):
        print("plan463: blocked; exact-pinned i2pd binary unavailable", file=sys.stderr)
        return 2
    if not source.is_dir() or subprocess.check_output(
        ["git", "-C", str(source), "rev-parse", "HEAD"], text=True
    ).strip() != PIN:
        print("plan463: blocked; exact-pinned i2pd source unavailable", file=sys.stderr)
        return 2
    if "i2pd version 2.61.0" not in subprocess.check_output(
        [str(i2pd), "--version"], stderr=subprocess.STDOUT, text=True
    ):
        print("plan463: blocked; binary version does not match the source pin", file=sys.stderr)
        return 2

    subprocess.run(
        ["cargo", "build", "--locked", "-p", "i2pr-daemon", "-p", "i2pr-interop"],
        cwd=root,
        check=True,
    )
    daemon = root / "target/debug/i2pr"
    interop = root / "target/debug/i2pr-interop"
    tcp_port, udp_port, control_port = reserve(socket.SOCK_STREAM), reserve(socket.SOCK_DGRAM), reserve(socket.SOCK_STREAM)
    daemon_child: subprocess.Popen[bytes] | None = None
    stock_child: subprocess.Popen[bytes] | None = None
    with tempfile.TemporaryDirectory(prefix="i2pr-plan463-") as temporary:
        work = Path(temporary)
        daemon_data = work / "daemon"
        stock_data = work / "stock"
        daemon_data.mkdir(mode=0o700)
        stock_data.mkdir(mode=0o700)
        daemon_config = work / "daemon.toml"
        daemon_config.write_text(
            f'''schema_version = 1
[router]
data_dir = {json.dumps(str(daemon_data))}
[transport.ntcp2]
enabled = true
qualification_profile = "controlled_loopback"
bind_address = "127.0.0.1"
listen_port = {tcp_port}
[ssu2]
enabled = true
bind_ipv4 = "127.0.0.1"
bind_ipv6 = ""
port = {udp_port}
[floodfill]
enabled = true
qualification_profile = "controlled_loopback"
[i2pcontrol]
enabled = true
bind_address = "127.0.0.1"
port = {control_port}
password = {json.dumps(PASSWORD)}
''',
            encoding="utf-8",
        )
        daemon_config.chmod(0o600)
        subprocess.run(
            [str(daemon), "identity", "generate", "--config", str(daemon_config)],
            cwd=root,
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
        )
        daemon_log = work / "daemon.log"
        with daemon_log.open("wb") as output:
            daemon_child = subprocess.Popen(
                [str(daemon), "run", "--config", str(daemon_config)],
                cwd=root,
                stdout=output,
                stderr=subprocess.STDOUT,
            )
            try:
                activation_deadline = time.monotonic() + 60
                while time.monotonic() < activation_deadline:
                    if daemon_child.poll() is not None:
                        lines = daemon_log.read_text(errors="replace").splitlines()
                        relevant = [
                            re.sub(r"[A-Za-z0-9+~/=-]{43,88}", "<redacted>", line)
                            for line in lines
                            if re.search(r"error|failed|floodfill|NTCP2|SSU2|control", line, re.I)
                        ]
                        print("plan463_daemon_exit=1", file=sys.stderr)
                        for line in relevant[-12:]:
                            print(line[:400], file=sys.stderr)
                        raise RuntimeError("daemon exited before controlled floodfill activation")
                    if "floodfill qualification activated" in daemon_log.read_text(errors="replace"):
                        break
                    time.sleep(0.2)
                else:
                    lines = daemon_log.read_text(errors="replace").splitlines()
                    relevant = [
                        re.sub(r"[A-Za-z0-9+~/=-]{43,88}", "<redacted>", line)
                        for line in lines
                        if re.search(r"error|failed|floodfill|NTCP2|SSU2|control", line, re.I)
                    ]
                    for line in relevant[-12:]:
                        print(line[:400], file=sys.stderr)
                    raise TimeoutError("controlled floodfill role did not activate")
                try:
                    router_id, router_info = wait_router_info(
                        control_port, time.monotonic() + 15
                    )
                except TimeoutError:
                    lines = daemon_log.read_text(errors="replace").splitlines()
                    relevant = [
                        re.sub(r"[A-Za-z0-9+~/=-]{43,88}", "<redacted>", line)
                        for line in lines
                        if re.search(r"error|failed|floodfill|inspection|RouterInfo", line, re.I)
                    ]
                    for line in relevant[-12:]:
                        print(line[:400], file=sys.stderr)
                    raise
                if len(router_info) > 786_432:
                    raise RuntimeError("controlled RouterInfo exceeded its byte bound")
                router_info_path = work / "controlled-router.info"
                router_info_path.write_bytes(router_info)
                if router_id is None:
                    identity = json.loads(
                        subprocess.check_output(
                            [str(interop), "ntcp2", "router-hash", "--router-info", str(router_info_path)],
                            cwd=root,
                            text=True,
                        )
                    )
                    digest = bytes.fromhex(identity["router_hash_sha256"])
                    router_id = (
                        base64.b64encode(digest)
                        .decode("ascii")
                        .replace("+", "-")
                        .replace("/", "~")
                        .rstrip("=")
                    )

                stock_netdb = stock_data / "netDb" / f"r{router_id[0]}"
                stock_netdb.mkdir(parents=True, mode=0o700)
                storage_id = router_id.replace("/", "-").replace("\\", "-")
                (stock_netdb / f"routerInfo-{storage_id}.dat").write_bytes(router_info)
                stock_config = work / "stock.conf"
                stock_config.write_text(
                    f'''daemon = false
loglevel = debug
netid = 2
address4 = 127.0.0.1
host = 127.0.0.1
port = 0
ipv4 = true
ipv6 = false
nat = false
notransit = true
floodfill = false
reservedrange = false
bandwidth = L
[ssu2]
enabled = false
published = false
[ntcp2]
enabled = true
published = true
port = {reserve(socket.SOCK_STREAM)}
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
''',
                    encoding="utf-8",
                )
                stock_log = work / "stock.log"
                with (work / "stock.stdout").open("wb") as output:
                    stock_child = subprocess.Popen(
                        [str(i2pd), f"--conf={stock_config}", f"--datadir={stock_data}",
                         "--log=file", f"--logfile={stock_log}"],
                        cwd=work,
                        stdout=output,
                        stderr=subprocess.STDOUT,
                    )
                    deadline = time.monotonic() + WAIT_SECONDS
                    while time.monotonic() < deadline:
                        if daemon_child.poll() is not None or stock_child.poll() is not None:
                            raise RuntimeError("a qualification process exited before receipt")
                        daemon_text = daemon_log.read_text(errors="replace")
                        stock_text = (
                            stock_log.read_text(errors="replace") if stock_log.exists() else ""
                        )
                        explore = "NetDbReq: Exploring new " in stock_text
                        accepted = "authenticated floodfill DatabaseLookup accepted and reply queued" in daemon_text
                        replies = stock_text.count("NetDbReq: DatabaseSearchReply for ")
                        late = stock_text.count("NetDbReq: Unsolicited or late database search reply for ")
                        if explore and accepted and replies > late:
                            print("plan463_stock_explore_triggered=1")
                            print("plan463_stock_explore_lookup_accepted=1")
                            print("plan463_stock_correlated_search_reply=1")
                            print("plan463_stock_fresh_ntcp2_initiator=1")
                            return 0
                        time.sleep(0.25)
                print("plan463_stock_explore_lookup_accepted=0")
                print("plan463_stock_correlated_search_reply=0")
                stock_text = stock_log.read_text(errors="replace") if stock_log.exists() else ""
                print(f"plan463_stock_explore_count={stock_text.count('NetDbReq: Exploring new ')}")
                print(
                    "plan463_stock_empty_netdb_count="
                    f"{stock_text.count('NetDbReq: No known routers, reseed seems to be totally failed')}"
                )
                relevant = [
                    re.sub(r"[A-Za-z0-9+~/=-]{43,88}", "<redacted>", line)
                    for line in stock_text.splitlines()
                    if re.search(r"error|failed|NetDbReq|floodfill|reserved|online|NTCP2", line, re.I)
                ]
                for line in relevant[-20:]:
                    print(line[:400], file=sys.stderr)
                if not stock_log.exists():
                    stock_stdout = (work / "stock.stdout").read_text(errors="replace")
                    relevant = [
                        re.sub(r"[A-Za-z0-9+~/=-]{43,88}", "<redacted>", line)
                        for line in stock_stdout.splitlines()
                        if re.search(r"error|failed|NetDbReq|NTCP2|database", line, re.I)
                    ]
                    for line in relevant[-12:]:
                        print(line[:400], file=sys.stderr)
                return 3
            finally:
                stop(stock_child)
                stop(daemon_child)


if __name__ == "__main__":
    raise SystemExit(main())
