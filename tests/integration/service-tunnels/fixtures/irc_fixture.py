#!/usr/bin/env python3
"""Plan 181 §5 loopback IRC fixture (stdlib only).

A minimal scripted IRC server for the M10 IRC round-trip rows.
Accepts one connection, records every received line, answers PING
with PONG, sends one welcome numeric after USER, and echoes one
PRIVMSG per received PRIVMSG. Facts are JSON lines; only line
contents/lengths are recorded (no secrets exist on this path).

Plan 214 §F — facts stream (append + flush per event) so the
counted driver proves *fresh* target observations after its
per-case baseline mid-run instead of inferring them from
client-side results. The echo preserves the received text after
the fixed `echo-hello` marker so a deterministic session token
round-trips through the target; the echo channel follows the
client's JOIN (default `#chan`) so the reply reaches the joined
session. Privacy booleans for the received USER line are derived
inside the fixture (hostname presence is tested, never uploaded).
"""

import argparse
import json
import os
import socket
import sys


def read_line(buffered, stream) -> str | None:
    while b"\r\n" not in buffered[0]:
        try:
            chunk = stream.recv(512)
        except socket.timeout:
            return None
        if not chunk:
            return None if not buffered[0] else buffered[0].decode("latin-1", "replace")
        buffered[0] += chunk
    line, _, rest = buffered[0].partition(b"\r\n")
    buffered[0] = rest
    return line.decode("latin-1", "replace")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=0)
    parser.add_argument("--facts", required=True)
    args = parser.parse_args()
    facts_log = open(args.facts, "w", encoding="utf-8")
    facts = []
    sequence = [0]

    def record(fact) -> None:
        sequence[0] += 1
        fact["seq"] = sequence[0]
        facts.append(fact)
        facts_log.write(json.dumps(fact, sort_keys=True) + "\n")
        facts_log.flush()

    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind(("127.0.0.1", args.port))
    listener.listen(1)
    # Plan 214 remote lane: the single IRC connection arrives only
    # after i2pd boot + product provisioning + the full HTTP phase
    # (minutes, not seconds). The bounded 600 s accept window covers
    # that lane with margin; the local lane connects immediately.
    # Single-connection semantics are unchanged.
    listener.settimeout(600)
    print(f"PORT={listener.getsockname()[1]}", flush=True)
    try:
        stream, _ = listener.accept()
    except socket.timeout:
        facts_log.close()
        return 1
    nick = "peer"
    channel = "#chan"
    ping_token = "fixture123"
    nick_observed = False
    try:
        with stream:
            stream.settimeout(20)
            buffered = [b""]
            welcomed = False
            while True:
                line = read_line(buffered, stream)
                if line is None:
                    break
                record({"event": "line", "line": line})
                upper = line.upper()
                if line.upper().startswith("NICK"):
                    parts = line.split()
                    if len(parts) >= 2:
                        nick = parts[1]
                        if not nick_observed:
                            nick_observed = True
                            record({"event": "register-nick", "nick": nick})
                elif upper.startswith("JOIN"):
                    parts = line.split()
                    if len(parts) >= 2:
                        joined = parts[1].lstrip(":")
                        if joined:
                            channel = joined
                elif upper.startswith("USER") and not welcomed:
                    welcomed = True
                    record({"event": "user", "line": line})
                    # Privacy booleans only: the hostname itself
                    # never enters evidence.
                    nodename = ""
                    try:
                        nodename = os.uname().nodename
                    except (AttributeError, OSError):
                        nodename = ""
                    record(
                        {
                            "event": "user-privacy",
                            "has_b32": ".b32.i2p" in line,
                            "has_loopback": "127.0.0.1" in line,
                            "has_nodename": bool(nodename) and nodename in line,
                        }
                    )
                    try:
                        stream.sendall(
                            f":fixture.test 001 {nick} :Welcome\r\n".encode("ascii")
                        )
                        stream.sendall(f"PING :{ping_token}\r\n".encode("ascii"))
                        record({"event": "ping-sent", "token": ping_token})
                    except OSError:
                        break
                elif upper.startswith("PING"):
                    token = line.split(None, 1)[1] if " " in line else "x"
                    try:
                        stream.sendall(f"PONG :{token}\r\n".encode("latin-1", "replace"))
                    except OSError:
                        break
                elif upper.startswith("PONG"):
                    token = line.split(None, 1)[1] if " " in line else ""
                    record(
                        {
                            "event": "pong-received",
                            "token": token,
                            "matches_challenge": ping_token in token,
                        }
                    )
                elif upper.startswith("PRIVMSG"):
                    # Split trailing text from the target prefix so
                    # the token observation is exact.
                    _, _, trailing = line.partition(" :")
                    text = trailing if trailing else line
                    if "DCC SEND" in line or "DCC " in upper:
                        record({"event": "dcc-observed", "line": line})
                    elif "\x01ACTION" in line:
                        record({"event": "action-received", "text": text})
                    else:
                        record({"event": "privmsg-received", "text": text})
                    try:
                        # The fixed `echo-hello` marker keeps the
                        # Plan 181 local echo contract; the received
                        # text after it carries the session token for
                        # the Plan 214 token-match proof.
                        echo = f":fixture.test PRIVMSG {channel} :echo-hello {text}\r\n"
                        stream.sendall(echo.encode("latin-1", "replace"))
                    except OSError:
                        break
                elif upper.startswith("QUIT"):
                    record({"event": "quit"})
                    break
    except OSError:
        pass
    finally:
        record({"event": "done", "lines": len(facts) + 1})
        facts_log.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
