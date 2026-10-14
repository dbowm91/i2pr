#!/usr/bin/env python3
"""Start pinned loopback i2pd peers and qualify SAM through ordinary i2pr run."""

import hashlib
import json
import os
from pathlib import Path
import random
import re
import shutil
import socket
import subprocess
import tempfile
import time
from collections import Counter
from urllib.request import urlopen


PIN = "635b013a612ff47278ef02acf8580a28e10e26c5"
VERSION = "2.61.0"
ROOT = Path(__file__).resolve().parents[3]
I2PD_CACHE = ROOT / "target/interop/cache/ssu2/i2pd" / PIN
I2PD = Path(os.environ.get("I2PR_I2PD_BIN", I2PD_CACHE / "bin/i2pd"))
DRIVER = ROOT / "tests/integration/core-router-recovery/plan458_sam_driver.py"
EVIDENCE = Path(os.environ.get("I2PR_PLAN458_EVIDENCE_DIR", ROOT / "target/interop/plan458-sam-product-evidence"))
ALPHABET = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ-~"
USED_PORTS = set()


def parse_stock_transit_rows(body):
    """Return only the row count from i2pd's pinned transit page HTML."""
    return body.count('<tr class="tcell_center">')


ANSI_ESCAPE = re.compile(r"\x1b\[[0-9;]*m")


def strip_ansi(line):
    """Remove terminal color escapes the daemon emits even when piped."""
    return ANSI_ESCAPE.sub("", line)


def parse_attempt_receipt_lines(lines):
    attempt_messages = {}
    stage_counts = Counter()
    transport_stage_counts = Counter()
    source_owned_delivery_dispositions = Counter()
    emitted_message_ids = set()
    postbuild_summary = None
    owner_install_receipts = []
    owner_eviction_events = 0
    owner_eviction_removed_owner = 0
    owner_eviction_removed_receive = 0
    inbound_build_wait_receipts = []
    for raw_line in lines:
        line = strip_ansi(raw_line)
        attempt = re.search(r"build_attempt_id\s*=\s*(\d+)", line)
        message = re.search(r"i2np_message_id\s*=\s*(\d+)", line)
        transition = re.search(r'transition\s*=\s*"?([a-z_]+)', line)
        if attempt and message and transition:
            attempt_messages[int(attempt.group(1))] = int(message.group(1))
            stage_counts[transition.group(1)] += 1
        elif attempt and transition:
            stage_counts[transition.group(1)] += 1
        if transition and "SSU2 message send receipt" in line:
            transport_stage_counts[transition.group(1)] += 1
        if (
            "destination build source-owned delivery result" in line
            or "destination inbound build attempt" in line
        ):
            direction = re.search(r'direction\s*=\s*["\']?([A-Za-z]+)', line)
            disposition = re.search(
                r'originator_write_disposition\s*=\s*["\']?([A-Za-z]+)', line
            )
            safe_directions = {"Inbound": "inbound", "Outbound": "outbound"}
            safe_dispositions = {
                "Pending": "pending",
                "AllOriginatorDeliveriesEmitted": "all-emitted",
                "PartialEmission": "partial-emission",
                "NotEmitted": "not-emitted",
                "UnobservedTransport": "unobserved-transport",
            }
            if (
                direction
                and disposition
                and direction.group(1) in safe_directions
                and disposition.group(1) in safe_dispositions
            ):
                label = (
                    f"{safe_directions[direction.group(1)]}:"
                    f"{safe_dispositions[disposition.group(1)]}"
                )
                source_owned_delivery_dispositions[label] += 1
        if transition and transition.group(1) == "udp_emitted":
            ids = re.search(r"i2np_message_ids\s*=\s*\[([0-9, ]*)\]", line)
            if ids:
                emitted_message_ids.update(
                    int(value) for value in ids.group(1).split(",") if value.strip()
                )
        if "destination inbound owners installed" in line:
            installed = re.search(
                r"inbound_receive_count\s*=\s*(\d+).*?inbound_owner_count\s*=\s*(\d+)",
                line,
            )
            if installed:
                owner_install_receipts.append(
                    {"receive_count": int(installed.group(1)),
                     "owner_count": int(installed.group(2))}
                )
        if "destination inbound owner removed on pool eviction" in line:
            owner_eviction_events += 1
            owner_eviction_removed_owner += int("removed_owner=true" in line)
            owner_eviction_removed_receive += int("removed_receive=true" in line)
        if re.search(r'transition\s*=\s*"?inbound_build_wait_expired', line):
            values = {
                key: re.search(rf"{key}\s*=\s*(\d+)", line)
                for key in (
                    "inbound_routed", "inbound_orphans",
                    "inbound_orphans_peer_match", "inbound_orphans_message_match",
                    "inbound_orphans_peer_id_plus_one", "inbound_orphans_peer_id_minus_one",
                    "inbound_orphans_peer_id_other",
                    "inbound_gateway_messages", "inbound_gateway_peer_match",
                    "inbound_gateway_route_match", "authenticated_candidates",
                    "candidate_peer_matches", "candidate_message_id_matches",
                    "candidate_peer_match_id_plus_one",
                    "candidate_peer_match_id_minus_one",
                    "candidate_peer_match_id_other", "pending_builds",
                )
            }
            if all(match for match in values.values()):
                inbound_build_wait_receipts.append(
                    {key: int(match.group(1)) for key, match in values.items()}
                )
        summary = re.search(
            r"inbound_receive_count\s*=\s*(\d+).*?lease_count\s*=\s*(\d+).*?"
            r"inbound_owner_registered\s*=\s*(true|false).*?"
            r"inbound_owner_count\s*=\s*(\d+)",
            line,
        )
        if summary:
            postbuild_summary = {
                "inbound_receive_count": int(summary.group(1)),
                "lease_count": int(summary.group(2)),
                "inbound_owner_registered": summary.group(3) == "true",
                "inbound_owner_count": int(summary.group(4)),
            }
    return {
        "attempts_observed": len(attempt_messages),
        "stage_counts": dict(sorted(stage_counts.items())),
        "transport_stage_counts": dict(sorted(transport_stage_counts.items())),
        "source_owned_delivery_dispositions": dict(
            sorted(source_owned_delivery_dispositions.items())
        ),
        "udp_emission_correlated_attempts": sum(
            message_id in emitted_message_ids for message_id in attempt_messages.values()
        ),
        "response_matched_attempts": stage_counts["authenticated_reply_matched"],
        "install_committed_attempts": stage_counts["install_committed"],
        "postbuild_summary": postbuild_summary,
        "owner_install_receipts": owner_install_receipts,
        "owner_eviction_events": owner_eviction_events,
        "owner_eviction_removed_owner": owner_eviction_removed_owner,
        "owner_eviction_removed_receive": owner_eviction_removed_receive,
        "inbound_build_wait_receipts": inbound_build_wait_receipts,
    }


def free_port():
    # Stay below the usual ephemeral-source range. Reference SSU2 sockets
    # make many outbound connections while the daemon starts; high ephemeral
    # listener ports can otherwise collide with those source-port choices.
    for _ in range(1000):
        port = random.randrange(18000, 30000)
        if port in USED_PORTS:
            continue
        sock = socket.socket()
        try:
            sock.bind(("127.0.0.1", port))
            USED_PORTS.add(port)
            return port
        except OSError:
            pass
        finally:
            sock.close()
    raise RuntimeError("could not reserve a private loopback port")


def i2p_b64(data):
    import base64

    return base64.b64encode(data).decode().translate(str.maketrans("+/", "-~")).rstrip("=")


def router_identity(path):
    raw = path.read_bytes()
    if len(raw) < 387:
        raise RuntimeError("router.info is shorter than a RouterIdentity")
    certificate_length = int.from_bytes(raw[385:387], "big")
    encoded_length = 387 + certificate_length
    if len(raw) < encoded_length:
        raise RuntimeError("router.info has a truncated RouterIdentity certificate")
    return raw[:encoded_length]


def storage_ident(path):
    return i2p_b64(hashlib.sha256(router_identity(path)).digest())


def cache_name(path):
    return hashlib.sha256(router_identity(path)).hexdigest()


class Lane:
    def __init__(self):
        self.scratch = Path(tempfile.mkdtemp(prefix="i2pr-plan458-"))
        self.children = []
        self.ports = {
            "f": free_port(),
            "c": free_port(),
            "n": free_port(),
            "d": free_port(),
            "f_sam": free_port(),
        }
        for name in ("f", "c", "n", "d"):
            self.ports[f"{name}_http"] = free_port()
        self.stock_transit_before = {}
        self.stock_log_offsets = {}
        self.logs = self.scratch / "logs"
        self.logs.mkdir()

    def conf(self, name, family, sam_port):
        home = self.scratch / name
        data = home / "data"
        (data / "family").mkdir(parents=True, exist_ok=True)
        subprocess.run(
            ["openssl", "ecparam", "-genkey", "-name", "prime256v1", "-noout", "-out", str(data / "family" / f"{family}.key")],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        port = self.ports[name]
        enabled = "true" if sam_port else "false"
        text = f"""daemon = false
loglevel = debug
netid = 2
family = {family}
address4 = 127.0.0.1
host = 127.0.0.1
port = {port}
ipv4 = true
ipv6 = false
nat = false
notransit = false
floodfill = true
reservedrange = false
bandwidth = L
[ssu2]
enabled = true
published = true
port = {port}
[ntcp2]
enabled = false
published = false
[http]
enabled = true
address = 127.0.0.1
port = {self.ports[f"{name}_http"]}
[httpproxy]
enabled = false
[socksproxy]
enabled = false
[sam]
enabled = {enabled}
address = 127.0.0.1
port = {sam_port}
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
"""
        (home / "i2pd.conf").write_text(text)
        (home / "tunnels.conf").write_text("")

    def stock_transit_count(self, name):
        """Read only the count of stock-owned transit rows over loopback HTTP."""
        endpoint = f"http://127.0.0.1:{self.ports[f'{name}_http']}/?page=transit_tunnels"
        try:
            with urlopen(endpoint, timeout=1) as response:
                body = response.read(1_000_000).decode("utf-8", errors="strict")
        except (OSError, UnicodeError, TimeoutError):
            return None
        return parse_stock_transit_rows(body)

    def stock_build_receipt(self):
        """Aggregate pinned stock logs and read-only inventory; never retain text."""
        counts = Counter()
        deltas = []
        for name in ("f", "c", "n", "d"):
            path = self.logs / f"{name}.log"
            if path.is_file():
                with path.open("rb") as log:
                    start = self.stock_log_offsets.get(name, 0)
                    size = path.stat().st_size
                    if size < start:
                        start = 0
                    start = max(start, size - 2_000_000)
                    log.seek(start)
                    text = log.read(2_000_000).decode("utf-8", errors="replace")
            else:
                text = ""
            counts["short_build_records_observed"] += text.count(
                "TransitTunnel: ShortTunnelBuild "
            )
            counts["short_request_decrypt_failures"] += text.count(
                "TransitTunnel: Can't decrypt short request record"
            )
            counts["short_request_local_next_hop_rejections"] += text.count(
                "TransitTunnel: Next ident is ours in short request record"
            )
            counts["transit_tunnel_create_events"] += sum(
                text.count(token)
                for token in (
                    "TransitTunnel: endpoint ",
                    "TransitTunnel: gateway ",
                )
            )
            counts["transit_reply_send_failures"] += text.count(
                "TransitTunnel: Failed to send reply for transit tunnel"
            )
            before = self.stock_transit_before.get(name)
            after = self.stock_transit_count(name)
            if before is not None and after is not None:
                deltas.append(after - before)
        return {
            **dict(sorted(counts.items())),
            "transit_inventory_delta_total": sum(deltas) if len(deltas) == 4 else None,
            "transit_inventory_peers_observed": len(deltas),
        }

    def start_reference(self, name):
        home = self.scratch / name
        log = self.logs / f"{name}.log"
        handle = log.open("wb")
        process = subprocess.Popen(
            [
                str(I2PD),
                f"--conf={home / 'i2pd.conf'}",
                f"--tunconf={home / 'tunnels.conf'}",
                f"--datadir={home / 'data'}",
                "--log=file",
                f"--logfile={log}",
            ],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            start_new_session=True,
        )
        handle.close()
        self.children.append(process)
        return process

    def wait_reference(self, name):
        deadline = time.monotonic() + 60
        router_info = self.scratch / name / "data/router.info"
        marker = f"Start listening on 127.0.0.1:{self.ports[name]}".encode()
        log = self.logs / f"{name}.log"
        while time.monotonic() < deadline:
            if router_info.exists() and marker in log.read_bytes():
                return router_info
            process = next((p for p in reversed(self.children) if p.poll() is None), None)
            if process is None:
                break
            time.sleep(0.25)
        raise RuntimeError(f"i2pd {name} failed readiness")

    def stop_references(self):
        for process in self.children:
            if process.poll() is None:
                try:
                    os.killpg(process.pid, 15)
                except ProcessLookupError:
                    pass
        for process in self.children:
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        self.children.clear()

    def seed_i2pd(self, target, source):
        base = self.scratch / target / "data/netDb"
        ident = storage_ident(source)
        for bucket in ALPHABET:
            directory = base / f"r{bucket}"
            directory.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, directory / f"routerInfo-{ident}.dat")

    def seed_i2pr(self, source):
        data_dir = self.scratch / "i2pr-data"
        data_dir.mkdir(parents=True, exist_ok=True, mode=0o700)
        os.chmod(data_dir, 0o700)
        netdb = data_dir / "netdb"
        netdb.mkdir(mode=0o700, exist_ok=True)
        os.chmod(netdb, 0o700)
        directory = self.scratch / "i2pr-data/netdb/routers"
        directory.mkdir(parents=True, exist_ok=True, mode=0o700)
        os.chmod(directory, 0o700)
        target = directory / cache_name(source)
        shutil.copyfile(source, target)
        os.chmod(target, 0o600)

    def router_network_sam_failure_stage(self):
        log_paths = (self.scratch / "i2pr.stdout", self.scratch / "i2pr.stderr")
        for log in log_paths:
            if not log.is_file():
                continue
            for raw_line in log.read_text(errors="replace").splitlines():
                line = strip_ansi(raw_line)
                if "router-network SAM destination provisioning failed" not in line:
                    continue
                match = re.search(r'failure_stage=["\']?([a-z0-9-]+)', line)
                if match:
                    return match.group(1)
        return None

    def router_network_sam_attempt_receipt(self):
        log_paths = (self.scratch / "i2pr.stdout", self.scratch / "i2pr.stderr")
        if not any(log.is_file() for log in log_paths):
            return {"attempts_observed": 0, "stage_counts": {}}
        lines = []
        for log in log_paths:
            if log.is_file():
                lines.extend(log.read_text(errors="replace").splitlines())
        return {
            **parse_attempt_receipt_lines(lines),
            "terminal_failure_stage": self.router_network_sam_failure_stage(),
        }

    def run(self):
        if not I2PD.is_file() or not os.access(I2PD, os.X_OK):
            raise RuntimeError("pinned i2pd binary is missing")
        source_pin = I2PD_CACHE / "source-revision.txt"
        if not source_pin.is_file() or source_pin.read_text().strip() != PIN:
            raise RuntimeError("pinned i2pd source receipt is missing or mismatched")
        version = subprocess.run([str(I2PD), "--version"], check=True, capture_output=True, text=True)
        if f"i2pd version {VERSION}" not in version.stdout + version.stderr:
            raise RuntimeError("i2pd binary version mismatch")
        subprocess.run(["cargo", "build", "--locked", "-p", "i2pr-daemon"], cwd=ROOT, check=True)

        self.conf("f", "lane-f", self.ports["f_sam"])
        self.conf("c", "lane-c", 0)
        self.conf("n", "lane-n", 0)
        self.conf("d", "lane-d", 0)
        peers = ("f", "c", "n", "d")
        for name in peers:
            self.start_reference(name)
        initial = {name: self.wait_reference(name) for name in peers}
        self.stop_references()
        time.sleep(1)

        for name in peers:
            for peer in peers:
                if peer != name:
                    self.seed_i2pd(name, initial[peer])
        for name in peers:
            self.start_reference(name)

        # Floodfill UpdateStats signs each configured family onto its RouterInfo.
        final_ri = {}
        for name in peers:
            final_ri[name] = self.wait_reference(name)
        families = ("lane-f", "lane-c", "lane-n", "lane-d")
        deadline = time.monotonic() + 90
        while time.monotonic() < deadline:
            if all(family.encode() in path.read_bytes() for family, path in zip(families, final_ri.values())):
                break
            time.sleep(0.5)
        else:
            raise RuntimeError("i2pd did not publish the four signed family identities")

        # The second cycle signed each router's family into its own RouterInfo,
        # while each peer still held the familyless first-cycle copy. Reseed
        # the updated signed records and restart once so every transit candidate
        # has the same family-aware view before the daemon selects tunnel hops.
        self.stop_references()
        for target in peers:
            for peer in peers:
                if peer != target:
                    self.seed_i2pd(target, final_ri[peer])
        for name in peers:
            self.start_reference(name)
        for name in peers:
            final_ri[name] = self.wait_reference(name)
        for name in peers:
            self.seed_i2pr(final_ri[name])

        # Establish the stock-owned baseline through i2pd's existing
        # loopback-only, read-only transit-tunnel page before i2pr can build.
        self.stock_transit_before = {
            name: self.stock_transit_count(name) for name in peers
        }

        # Select daemon listener ports only after the multi-minute reference
        # preparation, so another local test cannot claim a stale reservation.
        self.ports["i2pr_sam"] = free_port()
        self.ports["i2pr_ssu2"] = free_port()

        config = self.scratch / "i2pr.toml"
        config.write_text(
            f'''schema_version = 1
[router]
data_dir = "{self.scratch / "i2pr-data"}"
[netdb]
enabled = true
min_router_infos = 3
min_floodfill_advertisers = 1
[sam]
enabled = true
bind_address = "127.0.0.1"
port = {self.ports["i2pr_sam"]}
udp_port = 0
[ssu2]
enabled = true
qualification_profile = "controlled_loopback"
bind_ipv4 = "127.0.0.1"
bind_ipv6 = ""
port = {self.ports["i2pr_ssu2"]}
advertise = false
introducer_service = false
'''
        )
        subprocess.run([str(ROOT / "target/debug/i2pr"), "identity", "generate", "--config", str(config)], cwd=ROOT, check=True, stdout=subprocess.DEVNULL)
        local_router_info = self.scratch / "i2pr-router.info"
        subprocess.run(
            [
                "cargo",
                "run",
                "--locked",
                "-p",
                "i2pr-daemon",
                "--example",
                "plan458_export_router_info",
                "--",
                str(self.scratch / "i2pr-data"),
                str(self.ports["i2pr_ssu2"]),
                str(local_router_info),
            ],
            cwd=ROOT,
            check=True,
            stdout=subprocess.DEVNULL,
        )
        for name in peers:
            self.seed_i2pd(name, local_router_info)
        self.stock_log_offsets = {
            name: (self.logs / f"{name}.log").stat().st_size for name in peers
        }
        router = subprocess.Popen(
            [str(ROOT / "target/debug/i2pr"), "run", "--config", str(config)],
            cwd=ROOT,
            stdin=subprocess.DEVNULL,
            stdout=(self.scratch / "i2pr.stdout").open("wb"),
            stderr=(self.scratch / "i2pr.stderr").open("wb"),
            start_new_session=True,
        )
        self.children.append(router)
        endpoint = ("127.0.0.1", self.ports["i2pr_sam"])
        deadline = time.monotonic() + 60
        while time.monotonic() < deadline:
            if router.poll() is not None:
                raise RuntimeError("ordinary i2pr run exited before SAM readiness")
            try:
                with socket.create_connection(endpoint, timeout=0.25):
                    break
            except OSError:
                time.sleep(0.25)
        else:
            raise RuntimeError("ordinary i2pr run did not bind SAM")

        driver = subprocess.run(
            [
                "python3",
                str(DRIVER),
                "--i2pr-sam",
                f"127.0.0.1:{self.ports['i2pr_sam']}",
                "--i2pd-sam",
                f"127.0.0.1:{self.ports['f_sam']}",
                "--timeout",
                "240",
                "--evidence",
                str(EVIDENCE / "result.json"),
            ],
            cwd=ROOT,
            timeout=900,
            check=False,
        )
        if driver.returncode:
            stage = self.router_network_sam_failure_stage()
            suffix = f"; router_product_failure_stage={stage}" if stage else ""
            raise RuntimeError(
                f"SAM driver stopped with exit {driver.returncode}{suffix}"
            )


def main():
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    for name in ("result.json", "manifest.json", "attempt-diagnostic.json"):
        (EVIDENCE / name).unlink(missing_ok=True)
    lane = Lane()
    try:
        lane.run()
        result = json.loads((EVIDENCE / "result.json").read_text())
        result["stock_build_receipt"] = lane.stock_build_receipt()
        (EVIDENCE / "result.json").write_text(json.dumps(result, sort_keys=True) + "\n")
        (EVIDENCE / "manifest.json").write_text(
            json.dumps(
                {
                    "schema": "plan458-normal-sam-lane-v1",
                    "i2pd_version": VERSION,
                    "i2pd_revision": PIN,
                    "peer_count": 4,
                    "stock_build_receipt": result["stock_build_receipt"],
                    "transports_advertised_by_i2pr": 0,
                    "directions": result["directions"],
                },
                sort_keys=True,
            )
            + "\n"
        )
        print("Plan 458 normal-process SAM exchanges passed in both directions.")
    except Exception as exc:
        receipt = lane.router_network_sam_attempt_receipt()
        stock_receipt = lane.stock_build_receipt()
        (EVIDENCE / "attempt-diagnostic.json").write_text(
            json.dumps(
                {"schema": "plan471-stock-build-receipt-v1", **receipt,
                 "stock": stock_receipt},
                sort_keys=True,
            )
            + "\n"
        )
        print(
            "Plan 471 sanitized stock/build receipt: "
            + json.dumps({**receipt, "stock": stock_receipt}, sort_keys=True),
            file=__import__("sys").stderr,
        )
        print(f"Plan 458 qualification failed: {exc}", file=__import__("sys").stderr)
        raise SystemExit(1)
    finally:
        lane.stop_references()
        shutil.rmtree(lane.scratch, ignore_errors=True)


if __name__ == "__main__":
    main()
