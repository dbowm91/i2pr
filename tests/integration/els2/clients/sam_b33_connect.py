#!/usr/bin/env python3
"""Plan 381 §WP2 — the reference SAM client for a blinded (b33) connect.

This exists as its own file rather than as a heredoc inside the lane runner
because the SAM dance is where every non-obvious part of this lane lives, and a
heredoc makes that logic unlintable, untestable and unreadable. Three facts
about i2pd 2.61.0 at pin 635b013a… are load-bearing and all three are cheap to
get wrong in a way that produces a misleading error:

1. `SESSION CREATE` **requires** `DESTINATION`. Empty, or anything that is
   neither base64 nor the literal `TRANSIENT`, is answered `INVALID_KEY`
   (`libi2pd_client/SAM.cpp:427-441`). `TRANSIENT` mints a fresh ephemeral
   destination, which is what a client connect wants.

2. `SESSION CREATE` and `STREAM CONNECT` must be on **separate** connections. A
   successful create marks *that connection's* socket type `Session`
   (`SAM.cpp:449`), and `ProcessStreamConnect` then refuses it with
   `Socket already in use` (`SAM.cpp:529-533`). Sessions are bridge-wide
   (`m_Owner.FindSession`), so the create and the connect work on two
   connections. Doing both on one is the obvious thing to write and it fails
   with a message that sounds like a port collision.

3. The connect is gated on the client's own tunnel pool. Minting a fresh
   destination builds that pool, so the `SESSION CREATE` read is slow — tens of
   seconds. A short read timeout turns a working lane into a spurious failure,
   which is why this is one long bounded wait and not a retry loop.

The destination must carry a `.b32.i2p` suffix: `AddressBook::GetAddress`
matches `.b32.i2p` literally and has no `.b33.i2p` branch
(`libi2pd_client/AddressBook.cpp:454-461`). What makes an address blinded is
its 35-byte body, not the suffix.

Environment:
  I2PR_ELS2_SAM_PORT   required — the reference SAM port on loopback
  I2PR_ELS2_SAM_DEST   required — the destination, `.b32.i2p` spelling
  I2PR_ELS2_SAM_EXPECT optional — a banner the fixture must return

Exit code 0 only when the stream was established *and* the expected banner came
back. Nothing here is evidence; the lane records counts and outcomes only.
"""

from __future__ import annotations

import os
import socket
import sys

HELLO = "HELLO VERSION MIN=3.1 MAX=3.1"
SESSION_CREATE = "SESSION CREATE STYLE=STREAM ID={sid} DESTINATION=TRANSIENT"

# Minting a transient destination builds a tunnel pool before SAM answers.
# This is a bounded wait for a state change, not a retry of a failed attempt.
SESSION_TIMEOUT_S = 240.0
CONNECT_TIMEOUT_S = 120.0
PAYLOAD_TIMEOUT_S = 60.0


class SamError(RuntimeError):
    """A SAM command answered something other than success."""


def _open(port: int) -> tuple[socket.socket, object]:
    conn = socket.create_connection(("127.0.0.1", port), timeout=30)
    return conn, conn.makefile("rwb")


def _send(handle: tuple[socket.socket, object], line: str, timeout: float) -> str:
    conn, stream = handle
    conn.settimeout(timeout)
    stream.write((line + "\n").encode())
    stream.flush()
    reply = stream.readline()
    if not reply:
        raise SamError(f"{line.split()[0]}: connection closed without a reply")
    return reply.decode(errors="replace").rstrip()


def _expect_ok(reply: str, what: str) -> str:
    if "RESULT=OK" not in reply:
        raise SamError(f"{what} was not OK: {reply}")
    return reply


def _require_env(name: str) -> str:
    value = os.environ.get(name, "")
    if not value:
        # Missing environment fails; it never degrades into a skip.
        raise SamError(f"{name} is not set")
    return value


def run() -> int:
    sam_port = int(_require_env("I2PR_ELS2_SAM_PORT"))
    destination = _require_env("I2PR_ELS2_SAM_DEST")
    expected = os.environ.get("I2PR_ELS2_SAM_EXPECT", "")

    if ".i2p" not in destination:
        raise SamError(f"destination is not an .i2p host: {destination!r}")
    if not destination.endswith(".b32.i2p"):
        # Not a guess about i2pd's mood: GetAddress has no other branch, so
        # this would be answered INVALID_KEY and read like a crypto failure.
        raise SamError(
            f"destination must use the .b32.i2p spelling i2pd accepts, got {destination!r}"
        )

    # Overridable per attempt: a timed-out attempt leaves its session ID
    # held by the bridge, and reusing it answers DUPLICATED_ID — which reads
    # like a mesh defect but is only the retry tripping over its own past.
    session_id = os.environ.get("I2PR_ELS2_SAM_SESSION_ID", "plan381")
    create = _open(sam_port)
    try:
        _expect_ok(_send(create, HELLO, 30.0), "HELLO")
        created = _expect_ok(
            _send(create, SESSION_CREATE.format(sid=session_id), SESSION_TIMEOUT_S),
            "SESSION CREATE",
        )
    finally:
        create[0].close()

    # A second, independent connection. See fact 2 in the module docstring.
    connect = _open(sam_port)
    try:
        _expect_ok(_send(connect, HELLO, 30.0), "HELLO")
        _expect_ok(
            _send(
                connect,
                f"STREAM CONNECT ID={session_id} DESTINATION={destination} PORT=0",
                CONNECT_TIMEOUT_S,
            ),
            "STREAM CONNECT",
        )
        connect[0].settimeout(PAYLOAD_TIMEOUT_S)
        payload = connect[0].recv(4096)
    finally:
        connect[0].close()

    print(f"sam: destination accepted ({len(destination)} chars)")
    print(f"sam: session created ({len(created)} chars reply)")
    print(f"sam: payload {len(payload)} bytes: {payload[:64]!r}")

    if not payload:
        print("sam: FAIL empty payload", file=sys.stderr)
        return 1
    if expected and expected.encode() not in payload:
        print(f"sam: FAIL payload did not carry {expected!r}", file=sys.stderr)
        return 1
    print("sam: ok")
    return 0


def main() -> int:
    try:
        return run()
    except SamError as error:
        print(f"sam: FAIL {error}", file=sys.stderr)
        return 1
    except OSError as error:
        print(f"sam: FAIL {type(error).__name__}: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())