#!/usr/bin/env python3
"""Background holder for an i2pd SAM STREAM session.

Keeps a SAM STREAM session alive so the i2pd-owned destination's
LeaseSet2 stays published in the floodfill NetDB while i2pr looks
it up via the dedicated M6 interop lane.

Usage:
  python3 hold_sam_session.py <sam_port> <sid> <dest_path>

Writes:
  - <dest_path>: the SAM DEST REPLY PUB (public destination only)
  - exit status 0 if the SESSION CREATE reply carries RESULT=OK
"""

import os
import socket
import sys
import time

SAM_PORT = int(sys.argv[1])
SID = sys.argv[2]
DEST_PATH = sys.argv[3]


def transact(sock, command, buf_holder):
    sock.sendall(command.encode("ascii"))
    while b"\n" not in buf_holder[0]:
        chunk = sock.recv(65536)
        if not chunk:
            break
        buf_holder[0] += chunk
    line, _, rest = buf_holder[0].partition(b"\n")
    buf_holder[0] = rest
    return line.decode("latin-1", "replace")


def main() -> int:
    buf = [b""]
    sock = socket.create_connection(("127.0.0.1", SAM_PORT), timeout=15)
    sock.settimeout(60)
    try:
        hello = transact(sock, "HELLO VERSION MIN=3.1 MAX=3.1\n", buf)
        print(f"HELLO_REPLY={hello[:80]}", flush=True)
        if "RESULT=OK" not in hello:
            print(f"SAM hello failed: {hello[:120]}", file=sys.stderr, flush=True)
            return 1
        reply = transact(sock, "DEST GENERATE SIGNATURE_TYPE=7\n", buf)
        if not reply.startswith("DEST REPLY"):
            print(f"SAM DEST GENERATE failed: {reply[:120]}", file=sys.stderr, flush=True)
            return 1
        parts = reply.split(" ")
        pub_part = next((p[len("PUB="):] for p in parts if p.startswith("PUB=")), "")
        priv_part = next((p[len("PRIV="):] for p in parts if p.startswith("PRIV=")), "")
        if len(pub_part) < 512:
            print(f"PUB too short: {len(pub_part)}", file=sys.stderr, flush=True)
            return 1
        # SAM 3.1 SESSION CREATE expects the full destination string
        # (PUB || PRIV). i2pd's PRIV token holds the private-only
        # material; PUB holds the public destination. The SESSION
        # CREATE destination param is the full destination (matches
        # Plan 202 driver behavior). PRIV is the full dest in i2pd.
        sess = transact(
            sock,
            f"SESSION CREATE STYLE=STREAM ID={SID} DESTINATION={priv_part} SIGNATURE_TYPE=7 inbound.length=0 outbound.length=0\n",
            buf,
        )
        print(f"SESSION_REPLY={sess[:120]}", flush=True)
        if "RESULT=OK" not in sess:
            print(f"SAM SESSION CREATE failed: {sess[:200]}", file=sys.stderr, flush=True)
            return 1
        # Write the PUB AFTER SESSION CREATE so the harness can be
        # sure the i2pd-owned destination's tunnel pool is alive
        # (and its LeaseSet2 is queued for publication) before the
        # downstream drivers start their NetDB lookups.
        with open(DEST_PATH, "w", encoding="ascii") as f:
            f.write(pub_part + "\n")
        print(f"PUB_LEN={len(pub_part)}", flush=True)
        # Hold the SAM socket open for the rest of the test. The
        # child process inherits the socket file descriptor; i2pd's
        # SAM bridge ties the destination's tunnel pool lifetime to
        # this socket, so the LeaseSet2 stays published in the
        # floodfill NetDB while i2pr looks it up.
        while True:
            time.sleep(60)
    except KeyboardInterrupt:
        return 0
    except Exception as exc:
        print(f"holder failed: {exc}", file=sys.stderr, flush=True)
        return 1
    finally:
        sock.close()


if __name__ == "__main__":
    raise SystemExit(main())
