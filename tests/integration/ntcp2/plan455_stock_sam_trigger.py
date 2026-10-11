#!/usr/bin/env python3
"""Run one stock-only SAM STREAM control with NTCP2 as the sole transport."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "tests/integration/core-router-recovery"))

from run_plan458_sam_product import (
    DRIVER,
    I2PD,
    PIN,
    VERSION,
    Lane,
    free_port,
)


class StockNtcp2Lane(Lane):
    def __init__(self):
        super().__init__()
        self.ports["f_sam"] = free_port()
        self.ports["c_sam"] = free_port()
        self.evidence = self.scratch / "stock-sam-result.json"

    def conf(self, name, family, sam_port):
        super().conf(name, family, sam_port)
        config = self.scratch / name / "i2pd.conf"
        text = config.read_text()
        start = text.index("[ssu2]\n")
        end = text.index("[http]\n", start)
        text = (
            text[:start]
            + f"""[ssu2]
enabled = false
published = false
port = {self.ports[name]}
[ntcp2]
enabled = true
published = true
port = {self.ports[name]}
"""
            + text[end:]
        )
        config.write_text(text)

    def wait_reference(self, name):
        deadline = time.monotonic() + 60
        router_info = self.scratch / name / "data/router.info"
        marker = f"NTCP2: Start listening v4 TCP port {self.ports[name]}".encode()
        log = self.logs / f"{name}.log"
        while time.monotonic() < deadline:
            if router_info.exists() and marker in log.read_bytes():
                return router_info
            process = next((child for child in reversed(self.children) if child.poll() is None), None)
            if process is None:
                break
            time.sleep(0.25)
        raise RuntimeError(f"pinned NTCP2-only stock peer {name} failed readiness")

    def run(self):
        if not I2PD.is_file() or not os.access(I2PD, os.X_OK):
            raise RuntimeError("pinned i2pd binary is missing")
        source_pin = I2PD.parent.parent / "source-revision.txt"
        if not source_pin.is_file() or source_pin.read_text().strip() != PIN:
            raise RuntimeError("pinned i2pd source receipt is missing or mismatched")
        version = subprocess.run([str(I2PD), "--version"], check=True, capture_output=True, text=True)
        if f"i2pd version {VERSION}" not in version.stdout + version.stderr:
            raise RuntimeError("i2pd binary version mismatch")

        subprocess.run(["cargo", "build", "--locked", "-p", "i2pr-daemon"], cwd=ROOT, check=True)
        self.conf("f", "lane-f", self.ports["f_sam"])
        self.conf("c", "lane-c", self.ports["c_sam"])
        self.conf("n", "lane-n", 0)
        self.conf("d", "lane-d", 0)
        peers = ("f", "c", "n", "d")

        for name in peers:
            self.start_reference(name)
        initial = {name: self.wait_reference(name) for name in peers}
        self.stop_references()

        for name in peers:
            for peer in peers:
                if peer != name:
                    self.seed_i2pd(name, initial[peer])
        for name in peers:
            self.start_reference(name)
        family_names = ("lane-f", "lane-c", "lane-n", "lane-d")
        family_records = {name: self.wait_reference(name) for name in peers}
        deadline = time.monotonic() + 90
        while time.monotonic() < deadline:
            if all(family.encode() in family_records[name].read_bytes()
                   for family, name in zip(family_names, peers)):
                break
            time.sleep(0.5)
        else:
            raise RuntimeError("stock peers did not publish their signed family identities")

        self.stop_references()
        for target in peers:
            for peer in peers:
                if peer != target:
                    self.seed_i2pd(target, family_records[peer])
        for name in peers:
            self.start_reference(name)
        for name in peers:
            self.wait_reference(name)

        driver = subprocess.run(
            [
                "python3", str(DRIVER),
                "--i2pr-sam", f"127.0.0.1:{self.ports['f_sam']}",
                "--i2pd-sam", f"127.0.0.1:{self.ports['c_sam']}",
                "--timeout", "180",
                "--evidence", str(self.evidence),
                "--stock-control",
            ],
            cwd=ROOT,
            timeout=600,
            check=False,
        )
        if driver.returncode != 0:
            raise RuntimeError(f"stock SAM STREAM control stopped with exit {driver.returncode}")
        result = json.loads(self.evidence.read_text())
        if set(result.get("directions", {})) != {"stock-f-to-stock-c", "stock-c-to-stock-f"}:
            raise RuntimeError("stock SAM STREAM control did not complete both directions")

        # SSU2 was disabled in every reference process. A successful stock
        # STREAM exchange therefore used the advertised NTCP2-only peer set.
        ntcp2_peer_count = sum(
            1 for name in peers
            if "NTCP2: Start listening v4 TCP port" in (self.logs / f"{name}.log").read_text(errors="replace")
        )
        if ntcp2_peer_count != len(peers):
            raise RuntimeError("not every stock peer retained its NTCP2-only listener")
        peer_logs = [
            (self.logs / f"{name}.log").read_text(errors="replace")
            for name in peers
        ]
        authenticated_markers = sum(
            log.count("NTCP2: SessionConfirmed sent") for log in peer_logs
        )
        decrypted_frame_markers = sum(
            log.count("NTCP2: data frame authenticated and decrypted")
            for log in peer_logs
        )
        for direction, row in result["directions"].items():
            print(f"plan455_stock_sam_{direction}=passed bytes={row['length']}")
        print(f"plan455_ntcp2_only_stock_peers={ntcp2_peer_count}")
        # Pinned debug markers are auxiliary: this fixture already proves the
        # normal SAM request/reply traversed the only enabled router transport.
        print(f"plan455_ntcp2_session_confirmed_sent_markers={authenticated_markers}")
        print(f"plan455_ntcp2_authenticated_frame_markers={decrypted_frame_markers}")


def main():
    lane = StockNtcp2Lane()
    try:
        lane.run()
    except Exception as exc:
        print(f"Plan 455 stock SAM trigger blocked: {exc}", file=sys.stderr)
        raise SystemExit(1)
    finally:
        lane.stop_references()
        shutil.rmtree(lane.scratch, ignore_errors=True)


if __name__ == "__main__":
    main()
