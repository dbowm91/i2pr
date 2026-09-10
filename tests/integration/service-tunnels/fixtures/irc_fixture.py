#!/usr/bin/env python3
"""Plan 181 §5 loopback IRC fixture (stdlib only).

A minimal scripted IRC server for the M10 IRC round-trip rows.
Accepts one connection, records every received line, answers PING
with PONG, sends one welcome numeric after USER, and echoes one
PRIVMSG per received PRIVMSG. Facts are JSON lines; only line
contents/lengths are recorded (no secrets exist on this path).
"""

import argparse
import json
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
    facts = []
    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind(("127.0.0.1", args.port))
    listener.listen(1)
    listener.settimeout(60)
    print(f"PORT={listener.getsockname()[1]}", flush=True)
    try:
        stream, _ = listener.accept()
    except socket.timeout:
        return 1
    nick = "peer"
    try:
        with stream:
            stream.settimeout(20)
            buffered = [b""]
            welcomed = False
            while True:
                line = read_line(buffered, stream)
                if line is None:
                    break
                facts.append({"event": "line", "line": line})
                upper = line.upper()
                if line.upper().startswith("NICK"):
                    parts = line.split()
                    if len(parts) >= 2:
                        nick = parts[1]
                elif upper.startswith("USER") and not welcomed:
                    welcomed = True
                    facts.append({"event": "user", "line": line})
                    try:
                        stream.sendall(
                            f":fixture.test 001 {nick} :Welcome\r\n".encode("ascii")
                        )
                        stream.sendall(b"PING :fixture123\r\n")
                    except OSError:
                        break
                elif upper.startswith("PING"):
                    token = line.split(None, 1)[1] if " " in line else "x"
                    try:
                        stream.sendall(f"PONG :{token}\r\n".encode("latin-1", "replace"))
                    except OSError:
                        break
                elif upper.startswith("PRIVMSG"):
                    try:
                        stream.sendall(b":fixture.test PRIVMSG #chan :echo-hello\r\n")
                    except OSError:
                        break
                elif upper.startswith("QUIT"):
                    break
    except OSError:
        pass
    finally:
        facts.append({"event": "done", "lines": len(facts)})
        with open(args.facts, "w", encoding="utf-8") as handle:
            for fact in facts:
                handle.write(json.dumps(fact, sort_keys=True) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
