#!/usr/bin/env python3
"""Start pinned loopback i2pd peers and qualify SAM through ordinary i2pr run."""

import hashlib
import json
import os
import sys
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

sys.path.insert(0, str(Path(__file__).resolve().parent))
import plan458_sam_driver as driver  # noqa: E402  (sidecar path helper only; no lane logic)


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


def read_driver_leg_failure(evidence_path):
    """Return the driver's structured leg-failure record, if present.

    Fail-closed allowlist: only the documented v1 schema with a
    known leg label, a short error string, and a bounded numeric
    elapsed survive; a missing/unreadable sidecar or any other shape
    yields None rather than a partial record.
    """
    sidecar = Path(driver.sidecar_path(evidence_path))
    try:
        record = json.loads(sidecar.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None
    if not isinstance(record, dict):
        return None
    if record.get("schema") != "plan458-sam-leg-failure-v1":
        return None
    leg = record.get("leg")
    error = record.get("error")
    elapsed = record.get("elapsed_s")
    if leg not in ("i2pr-to-i2pd", "i2pd-to-i2pr"):
        return None
    if not isinstance(error, str) or len(error) > 200:
        return None
    if not isinstance(elapsed, (int, float)) or not 0 <= elapsed <= 3600:
        return None
    return {"leg": leg, "error": error, "elapsed_s": elapsed}


def parse_attempt_receipt_lines(lines):
    attempt_messages = {}
    stage_counts = Counter()
    transport_stage_counts = Counter()
    source_owned_delivery_dispositions = Counter()
    delivery_families = Counter()
    emitted_message_ids = set()
    postbuild_summary = None
    owner_install_receipts = []
    owner_eviction_events = 0
    owner_eviction_removed_owner = 0
    owner_eviction_removed_receive = 0
    inbound_build_wait_receipts = []
    stream_connect_handshake_wait_receipts = []
    stream_accept_data_phase_close_receipts = []
    accept_pump_started = 0
    delivery_admitted_cells = []
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
        if transition and transition.group(1) in ("delivery_admitted", "delivery_rejected"):
            family = re.search(r'delivery_family\s*=\s*"?([a-z0-9]+)', line)
            if family:
                delivery_families[family.group(1)] += 1
            direction = re.search(r'direction\s*=\s*"?([A-Za-z]+)', line)
            cell_count = re.search(r"cell_count\s*=\s*(\d+)", line)
            cell_ids = re.search(r"cell_message_ids\s*=\s*\[([0-9, ]*)\]", line)
            if direction and cell_count:
                delivery_admitted_cells.append(
                    {
                        "direction": direction.group(1).lower(),
                        "transition": transition.group(1),
                        "cell_count": int(cell_count.group(1)),
                        "cell_message_ids": [
                            int(value)
                            for value in (cell_ids.group(1).split(",") if cell_ids else [])
                            if value.strip()
                        ],
                    }
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
                    "inbound_orphans_peer_id_other", "inbound_orphans_retired_id_match",
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
        if re.search(r'transition\s*=\s*"?stream_connect_handshake_wait_expired', line):
            values = {
                key: re.search(rf"{key}\s*=\s*(\d+)", line)
                for key in (
                    "synack_observed", "synack_accepted", "synack_rejected",
                    "garlic_authenticated", "garlic_rejected",
                    "payloads_dequeued", "streaming_accepted",
                    "tunnel_data_received", "tunnel_data_cells_accepted",
                    "garlic_owner_matches", "garlic_owner_misses",
                    "direct_garlic_received",
                    "miss_service_unowned", "miss_exploratory_known",
                    "miss_retired_service", "miss_unknown",
                    "send_delivered", "send_unknown_peer",
                    "send_missing_factory", "send_factory_exhausted",
                    "send_delivery_failed",
                    "send_failed_ls2_missing", "send_failed_ls2_invalid",
                    "send_failed_ls2_install", "send_failed_bridge",
                    "send_sweeps", "send_queue_start", "send_queue_end",
                )
            }
            if all(match for match in values.values()):
                receipt = {key: int(match.group(1)) for key, match in values.items()}
                classes = (
                    receipt["miss_service_unowned"]
                    + receipt["miss_exploratory_known"]
                    + receipt["miss_retired_service"]
                    + receipt["miss_unknown"]
                )
                if classes != receipt["garlic_owner_misses"]:
                    continue
                # Plan 478 send-side split: the four LS2/bridge
                # causes must sum to the send-failure aggregate; a
                # line violating it is malformed, not evidence.
                split = (
                    receipt["send_failed_ls2_missing"]
                    + receipt["send_failed_ls2_invalid"]
                    + receipt["send_failed_ls2_install"]
                    + receipt["send_failed_bridge"]
                )
                if split != receipt["send_delivery_failed"]:
                    continue
                stream_connect_handshake_wait_receipts.append(receipt)
        if re.search(r'transition\s*=\s*"?stream_accept_data_phase_closed', line):
            values = {
                key: re.search(rf"{key}\s*=\s*(\d+)", line)
                for key in (
                    "data_observed", "data_accepted", "data_rejected",
                    "data_payload_bytes",
                    "canon_data_observed", "canon_data_accepted",
                    "canon_data_rejected", "canon_data_payload_bytes",
                    "garlic_authenticated", "garlic_rejected",
                    "payloads_dequeued", "streaming_accepted",
                    "connection_state_code", "undrained_bytes", "reset",
                    "bridge_present",
                )
            }
            if all(match for match in values.values()):
                receipt = {key: int(match.group(1)) for key, match in values.items()}
                # Both managers share the observed/accepted/rejected
                # invariant by construction; a line violating it is a
                # malformed receipt, not evidence.
                mirror = (
                    receipt["data_accepted"] + receipt["data_rejected"]
                    == receipt["data_observed"]
                )
                canon = (
                    receipt["canon_data_accepted"] + receipt["canon_data_rejected"]
                    == receipt["canon_data_observed"]
                )
                if not (mirror and canon):
                    continue
                # bridge_present is a 0/1 flag: 0 marks the torn-down
                # shape (bridge gone before close, all deltas zero by
                # construction); anything else is malformed.
                if receipt["bridge_present"] not in (0, 1):
                    continue
                stream_accept_data_phase_close_receipts.append(receipt)
        # Plan 478 pump lifecycle: each inbound raw-pump start is one
        # INFO line with no build_attempt_id, so it is counted
        # explicitly (never a stage count). Together with the close
        # receipts it splits bind-gap (0 starts) from unfinished-pump.
        if re.search(r'transition\s*=\s*"?stream_accept_pump_started', line):
            accept_pump_started += 1
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
        "delivery_families": dict(sorted(delivery_families.items())),
        "delivery_admitted_cells": delivery_admitted_cells,
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
        "stream_connect_handshake_wait_receipts": stream_connect_handshake_wait_receipts,
        "stream_accept_data_phase_close_receipts": stream_accept_data_phase_close_receipts,
        "accept_pump_started": accept_pump_started,
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
        self.stock_churn_before = {}
        self.mesh_quiet = False
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
        creator = getattr(self, "local_ident", None)
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
            if creator:
                counts["creator_ri_store_requests"] += text.count(
                    f"Store request: RouterInfo {creator}"
                )
                counts["creator_ri_added"] += text.count(
                    f"RouterInfo added: {creator}"
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
        # Mesh-quiescence gate: freshly restarted stock routers rebuild
        # their pools in a startup storm (tens of transit changes per
        # 30 s), and i2pr's first sessions plus RI propagation land in
        # the middle of it. Poll until two consecutive 30 s windows are
        # jointly quiet, bounded so a never-quiet mesh is recorded, not
        # fatal. Stock is never reconfigured; only i2pr start waits.
        self.mesh_quiet = False
        previous = dict(self.stock_transit_before)
        for _ in range(10):
            time.sleep(30)
            current = {name: self.stock_transit_count(name) for name in peers}
            deltas = {}
            for name in peers:
                before = previous.get(name)
                after = current.get(name)
                if before is None or after is None:
                    deltas[name] = None
                else:
                    deltas[name] = abs(after - before)
            self.stock_churn_before = deltas
            known = [v for v in deltas.values() if v is not None]
            if known and sum(known) <= 8:
                self.mesh_quiet = True
                break
            previous = current

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
        self.local_ident = storage_ident(local_router_info)
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


def should_wait_for_accept_close(driver_leg_failure):
    """Whether a late-flushing ACCEPT close receipt may still arrive.

    The daemon emits `stream_accept_data_phase_closed` when its raw
    pump finishes, which happens after the driver closes its SAM
    sockets — potentially milliseconds after the driver subprocess
    exits. Only a started reverse leg (driver reached the i2pd-to-i2pr
    read) can leave such a receipt pending; any other failure has no
    ACCEPT pump outstanding and must not burn the wait budget.
    """
    return (
        isinstance(driver_leg_failure, dict)
        and driver_leg_failure.get("leg") == "i2pd-to-i2pr"
    )


def wait_for_accept_close_receipt(stdout_path, budget_s=30.0):
    """Poll a live daemon stdout for a late ACCEPT close receipt.

    Returns `(found, waited_s)`. The daemon is still running while
    the lane writes its diagnostic (it is stopped only in `finally`),
    so a pump that finishes on driver-socket close flushes inside
    this window. Bounded: absence after the budget is itself evidence
    (pump never finished), not a reason to wait longer.
    """
    marker = b'transition="stream_accept_data_phase_closed"'
    if budget_s <= 0:
        return False, 0.0
    deadline = time.monotonic() + budget_s
    while True:
        try:
            if marker in Path(stdout_path).read_bytes():
                return True, round(budget_s - max(deadline - time.monotonic(), 0.0), 1)
        except OSError:
            pass
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            return False, round(budget_s, 1)
        time.sleep(min(0.5, remaining))


def router_binary_provenance():
    """Provenance of the i2pr binary the lane executes.

    Records the binary mtime/size plus the source-tree identity
    (HEAD SHA, dirty file count) so a receipt can never again be
    read against the wrong build. Read-only; a collection failure
    yields "unknown" fields rather than aborting the lane.
    """
    provenance = {
        "bin_mtime": "unknown",
        "bin_size": -1,
        "git_head": "unknown",
        "git_dirty_files": -1,
    }
    try:
        stat = (ROOT / "target/debug/i2pr").stat()
        provenance["bin_mtime"] = int(stat.st_mtime)
        provenance["bin_size"] = int(stat.st_size)
    except OSError:
        pass
    try:
        head = subprocess.run(
            ["git", "rev-parse", "--short", "HEAD"],
            cwd=ROOT, capture_output=True, text=True, timeout=30,
        )
        if head.returncode == 0:
            provenance["git_head"] = head.stdout.strip()
        dirty = subprocess.run(
            ["git", "status", "--short"],
            cwd=ROOT, capture_output=True, text=True, timeout=30,
        )
        if dirty.returncode == 0:
            provenance["git_dirty_files"] = len(
                [line for line in dirty.stdout.splitlines() if line.strip()]
            )
    except (OSError, ValueError):
        pass
    return provenance


def clean_lane_evidence(evidence_dir=None):
    """Remove previous-run lane outputs before a run starts.

    Covers result, manifest, diagnostic, AND the driver
    leg-failure sidecar: a previous run's sidecar must never merge
    into the next run's diagnostic as a stale `driver_leg_failure`.
    """
    root = Path(evidence_dir) if evidence_dir is not None else EVIDENCE
    root.mkdir(parents=True, exist_ok=True)
    for name in ("result.json", "manifest.json", "attempt-diagnostic.json"):
        (root / name).unlink(missing_ok=True)
    Path(driver.sidecar_path(root / "result.json")).unlink(missing_ok=True)


def main():
    clean_lane_evidence()
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
        driver_leg_failure = read_driver_leg_failure(EVIDENCE / "result.json")
        # Late-flush grace: the daemon's ACCEPT close receipt is
        # emitted when its raw pump finishes on driver-socket close,
        # which races the driver-subprocess exit. If a reverse leg
        # started, poll the still-running daemon's stdout boundedly
        # before snapshotting the diagnostic; otherwise parse at
        # once. The daemon is stopped only in `finally` below.
        accept_close_waited_s = 0.0
        accept_close_found = False
        if should_wait_for_accept_close(driver_leg_failure):
            accept_close_found, accept_close_waited_s = wait_for_accept_close_receipt(
                lane.scratch / "i2pr.stdout"
            )
        receipt = lane.router_network_sam_attempt_receipt()
        stock_receipt = lane.stock_build_receipt()
        provenance = router_binary_provenance()
        (EVIDENCE / "attempt-diagnostic.json").write_text(
            json.dumps(
                {"schema": "plan471-stock-build-receipt-v1", **receipt,
                 "stock": stock_receipt,
                 "driver_leg_failure": driver_leg_failure,
                 "accept_close_wait_s": accept_close_waited_s,
                 "accept_close_found": accept_close_found,
                 "router_binary_provenance": provenance,
                 "mesh_churn_before": lane.stock_churn_before,
                 "mesh_quiet": lane.mesh_quiet},
                sort_keys=True,
            )
            + "\n"
        )
        print(
            "Plan 471 sanitized stock/build receipt: "
            + json.dumps({**receipt, "stock": stock_receipt,
                          "driver_leg_failure": driver_leg_failure,
                          "accept_close_wait_s": accept_close_waited_s,
                          "accept_close_found": accept_close_found,
                          "router_binary_provenance": provenance,
                          "mesh_churn_before": lane.stock_churn_before,
                          "mesh_quiet": lane.mesh_quiet}, sort_keys=True),
            file=__import__("sys").stderr,
        )
        print(f"Plan 458 qualification failed: {exc}", file=__import__("sys").stderr)
        raise SystemExit(1)
    finally:
        lane.stop_references()
        keep = os.environ.get("I2PR_PLAN458_KEEP_SCRATCH") == "1"
        if keep:
            print(f"Plan 458 scratch preserved at {lane.scratch}", file=__import__("sys").stderr)
        else:
            shutil.rmtree(lane.scratch, ignore_errors=True)


if __name__ == "__main__":
    main()
