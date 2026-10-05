#!/usr/bin/env python3
"""Guard the listener-independent daemon SAM/I2CP connection seams."""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SAM = (ROOT / "crates/i2pr-daemon/src/sam.rs").read_text(encoding="utf-8")
RAW = (ROOT / "crates/i2pr-daemon/src/sam/raw_stream.rs").read_text(encoding="utf-8")
I2CP = (ROOT / "crates/i2pr-daemon/src/i2cp.rs").read_text(encoding="utf-8")


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def function_region(source: str, name: str) -> str:
    """Return a function through its impl-level closing brace."""
    match = re.search(
        rf"^\s*(?:(?:pub(?:\([^)]*\))?)\s+)?(?:async\s+)?fn\s+{name}\b",
        source,
        re.M,
    )
    require(match is not None, f"missing function {name}")
    end = re.search(r"^    }\s*$", source[match.end() :], re.M)
    require(end is not None, f"could not find end of function {name}")
    return source[match.start() : match.end() + end.end()]


def check() -> None:
    sam_private = function_region(SAM, "drive_private_connection")
    i2cp_private = function_region(I2CP, "drive_private_connection")
    sam_serve = function_region(SAM, "serve")
    i2cp_serve = function_region(I2CP, "serve")

    require("SamIoStream" in sam_private, "private SAM driver must accept the shared async stream")
    require("SamConnectionOrigin::ManagedAppPrivate" in sam_private, "private SAM profile is implicit")
    require("try_acquire_owned" in sam_private and "handle_connection(" in sam_private,
            "private SAM must use bounded admission and the shared connection driver")
    require(not re.search(r"TcpStream|TcpListener|local_socket_pair|\.bind\(|\.connect\(", sam_private),
            "private SAM seam must not bind/connect host sockets or use a socket pair")

    require("I2cpIoStream" in i2cp_private, "private I2CP driver must accept the shared async stream")
    require("try_acquire_owned" in i2cp_private and "handle_connection(" in i2cp_private,
            "private I2CP must use bounded admission and the shared connection driver")
    require(not re.search(r"TcpStream|TcpListener|\.bind\(|\.connect\(", i2cp_private),
            "private I2CP seam must not bind/connect host sockets")

    require("handle_connection(" in sam_serve and "Box::new(stream)" in sam_serve,
            "SAM listener must adapt accepted TCP streams into the shared driver")
    require("SamConnectionOrigin::Loopback" in sam_serve, "SAM listener origin must be explicit")
    require("handle_connection(" in i2cp_serve and "Box::new(stream)" in i2cp_serve,
            "I2CP listener must adapt accepted TCP streams into the shared driver")

    forward_deny = re.search(
        r"SamConnectionOrigin::ManagedAppPrivate\s*=>\s*\{(?P<body>.*?)\n\s*\}\s*SamConnectionOrigin::Loopback",
        SAM,
        re.S,
    )
    require(forward_deny is not None, "SAM private FORWARD denial is missing")
    require("StreamForwardFailed" in forward_deny.group("body"), "private FORWARD must return a protocol error")
    require("execute_stream_forward" not in forward_deny.group("body"), "private FORWARD reaches host-side effects")

    require("pub stream: SamIoStream" in RAW, "raw STREAM handoff must retain the generic transport")
    require("use tokio::net::TcpStream;" not in RAW, "raw STREAM driver must not require a concrete TCP stream")
    require("local_socket_pair" in SAM, "loopback FORWARD's existing socket bridge disappeared")

    print("managed-app private SAM/I2CP connection seam checks passed")


if __name__ == "__main__":
    try:
        check()
    except RuntimeError as error:
        print(f"managed-app private client seam check failed: {error}", file=sys.stderr)
        raise SystemExit(1)
