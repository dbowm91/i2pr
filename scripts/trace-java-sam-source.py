#!/usr/bin/env python3
"""Emit Plan 411's bounded trace through the pinned Java SAM bind path."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

PIN = "9134f808337b401e8e53c73734c81fab04280c9d"
FACTS = {
    "SAMBridge.java": [
        ("SAMBridge.DEFAULT_DATAGRAM_HOST", 'DEFAULT_DATAGRAM_HOST = "127.0.0.1"'),
        ("SAMBridge.DEFAULT_DATAGRAM_PORT_INT", "DEFAULT_DATAGRAM_PORT_INT = 7655"),
        ("SAMBridge.getV3DatagramServer", "new SAMv3DatagramServer(this, host, port, props)"),
    ],
    "SAMv3Handler.java": [],
    "SAMv3DatagramServer.java": [
        ("SAMv3DatagramServer.constructor.bind", "_server.socket().bind(new InetSocketAddress(host, port))"),
    ],
}


def locate_unique(lines: list[str], token: str, label: str) -> int:
    hits = [index for index, line in enumerate(lines) if token in line]
    if len(hits) != 1:
        raise ValueError(f"source trace could not uniquely match {label}")
    return hits[0]


def emit(source_root: Path, output: Path) -> None:
    package = source_root / "apps/sam/java/src/net/i2p/sam"
    trace: list[dict[str, object]] = []

    def add(name: str, method: str, line: int, token: str) -> None:
        trace.append({
            "source": name,
            "method": method,
            "line": line + 1,
            "fact": hashlib.sha256(token.encode()).hexdigest()[:16],
        })

    for name, facts in FACTS.items():
        if name == "SAMv3Handler.java":
            continue
        lines = (package / name).read_text(encoding="utf-8").splitlines()
        for method, token in facts:
            add(name, method, locate_unique(lines, token, method), token)

    name = "SAMv3Handler.java"
    lines = (package / name).read_text(encoding="utf-8").splitlines()
    start = locate_unique(lines, "protected boolean execSessionMessage(", "execSessionMessage start")
    end = next((index for index in range(start + 1, len(lines)) if lines[index] == "\t}"), None)
    if end is None:
        raise ValueError("source trace could not delimit execSessionMessage")
    method_lines = lines[start:end + 1]
    branch = locate_unique(method_lines, 'style.equals("DATAGRAM") ||', "DATAGRAM branch")
    bind_token = "SAMv3DatagramServer dgs = bridge.getV3DatagramServer(props)"
    bind_hits = [index for index, line in enumerate(method_lines) if bind_token in line]
    if len(bind_hits) != 3:
        raise ValueError("source trace expected three stock DATAGRAM variants")
    bind = next((index for index in bind_hits if 0 < index - branch < 25), None)
    if bind is None:
        raise ValueError("source trace cannot associate DATAGRAM branch with server bind")
    add(name, "SAMv3Handler.execSessionMessage.DATAGRAM", start + branch,
        'style.equals("DATAGRAM") ||')
    add(name, "SAMv3Handler.execSessionMessage.getV3DatagramServer", start + bind, bind_token)

    io_token = "catch (IOException e)"
    io = locate_unique(method_lines, io_token, "execSessionMessage IOException catch")
    reply_token = "return writeString(SESSION_ERROR, e.getMessage())"
    reply_hits = [index for index, line in enumerate(method_lines) if reply_token in line]
    reply = next((index for index in reply_hits if 0 < index - io < 4), None)
    if reply is None:
        raise ValueError("source trace cannot associate IOException with SAM error reply")
    add(name, "SAMv3Handler.execSessionMessage.IOException", start + io, io_token)
    add(name, "SAMv3Handler.execSessionMessage.error-reply", start + reply, reply_token)

    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps({
        "schema": "i2pr-java-sam-source-trace-v1",
        "java_pin": PIN,
        "facts": trace,
    }, sort_keys=True, indent=2) + "\n", encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("source_root", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    emit(args.source_root, args.output)
    print("Java SAM source trace emitted")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
