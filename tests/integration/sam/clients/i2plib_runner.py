#!/usr/bin/env python3
"""Plan 150 external SAM runner using the pinned i2plib SAM surface.

The preferred libsam3 snapshot cannot consume the 608-character Ed25519
private destination emitted by i2pr: its public API rejects values shorter
than ``SAM3_PRIVKEY_MIN_SIZE`` (884) before sending SESSION CREATE. This
runner is the explicitly labelled independent-client substitute per Plan
150 sections 2.3 and 6. It uses the unmodified i2plib.sam message and I2P
Base64 helpers, while this small harness owns the socket lifecycle.

The other mandatory independent client is i2psam. This runner is not an
i2pr implementation and never imports an i2pr crate or module.

Usage:

    i2plib_runner connect <host> <port> <peer_pub> <send> <expect> <silent>
                         [<private_file>]
    i2plib_runner accept  <host> <port> <send> <expect> <silent>
                         [<private_file>]
    i2plib_runner import  <host> <port> <private_file> <public_file>

Private and public destination files are temporary harness inputs. The
private value is never printed and the caller owns their cleanup.
"""

from __future__ import annotations

import os
import socket
import sys
import threading
import time
from pathlib import Path

# Resolve the exact pinned checkout before importing the external package.
_DEFAULT_I2PLIB_ROOT = (
    Path(__file__).resolve().parents[4]
    / "target/interop/cache/sam/i2plib/6edf51cd5d21cc745aa7e23cb98c582144884fa8"
)
_I2PLIB_ROOT = os.environ.get("I2PLIB_ROOT", str(_DEFAULT_I2PLIB_ROOT))
if os.path.isdir(_I2PLIB_ROOT) and _I2PLIB_ROOT not in sys.path:
    sys.path.insert(0, _I2PLIB_ROOT)

import i2plib.sam as i2psam  # noqa: E402

SAM_PUB_BYTES = 391
TRANSFER_BARRIER = b"\x00PLAN150-TRANSFER-DONE\x00"
TRANSFER_ACK = b"\x00PLAN150-TRANSFER-ACK\x00"


class BufferedSocket:
    """Socket wrapper that preserves bytes read past a SAM line."""

    def __init__(self, sock: socket.socket) -> None:
        self.sock = sock
        self.buffer = bytearray()

    def sendall(self, data: bytes) -> None:
        self.sock.sendall(data)

    def close(self) -> None:
        try:
            self.sock.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        self.sock.close()

    def recv_line(self, timeout_s: float) -> str:
        deadline = time.monotonic() + timeout_s
        while True:
            newline = self.buffer.find(b"\n")
            if newline >= 0:
                line = bytes(self.buffer[: newline + 1])
                del self.buffer[: newline + 1]
                return line.decode("utf-8", errors="replace").rstrip("\r\n")
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError("SAM line deadline exceeded")
            self.sock.settimeout(max(0.05, remaining))
            chunk = self.sock.recv(4096)
            if not chunk:
                raise EOFError("SAM socket closed while reading a line")
            self.buffer.extend(chunk)

    def recv_raw(self, size: int, timeout_s: float) -> bytes:
        out = bytearray()
        if self.buffer:
            take = min(size, len(self.buffer))
            out.extend(self.buffer[:take])
            del self.buffer[:take]
        deadline = time.monotonic() + timeout_s
        while len(out) < size:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError(
                    f"raw read deadline exceeded got={len(out)} want={size}"
                )
            self.sock.settimeout(max(0.05, remaining))
            chunk = self.sock.recv(size - len(out))
            if not chunk:
                raise EOFError(f"raw socket closed got={len(out)} want={size}")
            out.extend(chunk)
        return bytes(out)


def open_socket(host: str, port: int) -> BufferedSocket:
    return BufferedSocket(socket.create_connection((host, port), timeout=10.0))


def public_from_private(private_destination: str) -> str:
    private_bytes = i2psam.i2p_b64decode(private_destination)
    if len(private_bytes) < SAM_PUB_BYTES:
        raise ValueError("external private destination is shorter than its public part")
    return i2psam.i2p_b64encode(private_bytes[:SAM_PUB_BYTES])


def open_session(
    host: str, port: int, session_id: str, private_destination: str | None = None
) -> tuple[BufferedSocket, str]:
    sam = open_socket(host, port)
    try:
        sam.sendall(i2psam.hello("3.1", "3.1"))
        hello = sam.recv_line(5.0)
        if not hello.startswith("HELLO REPLY RESULT=OK"):
            raise RuntimeError("HELLO VERSION 3.1 was rejected")
        destination = private_destination or i2psam.TRANSIENT_DESTINATION
        sam.sendall(i2psam.session_create("STREAM", session_id, destination))
        reply = i2psam.Message(sam.recv_line(5.0))
        if not reply.ok:
            raise RuntimeError("SESSION CREATE was rejected")
        return sam, public_from_private(reply["DESTINATION"])
    except Exception:
        sam.close()
        raise


def bidirectional_transfer(
    sam: BufferedSocket, send_data: bytes, expected_data: bytes
) -> int:
    """Send and receive at once so neither side relies on turn-taking."""

    send_error: list[BaseException] = []

    def writer() -> None:
        try:
            for offset in range(0, len(send_data), 4096):
                sam.sendall(send_data[offset : offset + 4096])
            sam.sendall(TRANSFER_BARRIER)
        except BaseException as error:  # report to the owning thread
            send_error.append(error)

    thread = threading.Thread(target=writer, name="i2plib-sam-writer")
    thread.start()
    try:
        received = sam.recv_raw(len(expected_data), 120.0)
        barrier = sam.recv_raw(len(TRANSFER_BARRIER), 120.0)
    except (EOFError, TimeoutError) as error:
        print(
            f"i2plib transfer receive failed expected={len(expected_data)}: {error}",
            file=sys.stderr,
        )
        thread.join(timeout=2.0)
        return 7
    thread.join(timeout=120.0)
    if thread.is_alive() or send_error:
        print("i2plib transfer send failed", file=sys.stderr)
        return 5
    if barrier != TRANSFER_BARRIER:
        print("i2plib transfer barrier mismatch", file=sys.stderr)
        return 8
    sam.sendall(TRANSFER_ACK)
    try:
        acknowledgement = sam.recv_raw(len(TRANSFER_ACK), 120.0)
    except (EOFError, TimeoutError) as error:
        print(f"i2plib transfer acknowledgement failed: {error}", file=sys.stderr)
        return 7
    if acknowledgement != TRANSFER_ACK:
        print("i2plib transfer acknowledgement mismatch", file=sys.stderr)
        return 8
    if received != expected_data:
        print("i2plib transfer payload mismatch", file=sys.stderr)
        return 8
    return 0


def parse_silent(value: str) -> bool:
    lowered = value.lower()
    if lowered in {"true", "1", "yes"}:
        return True
    if lowered in {"false", "0", "no"}:
        return False
    raise ValueError("silent must be true or false")


def run_connect(
    host: str,
    port: int,
    peer_public: str,
    send_path: Path,
    expect_path: Path,
    silent: bool,
    private_path: Path | None,
) -> int:
    send_data = send_path.read_bytes()
    expected_data = expect_path.read_bytes()
    private = private_path.read_text(encoding="ascii").strip() if private_path else None
    session_id = f"i2plib-connect-{os.getpid()}-{time.monotonic_ns()}"
    try:
        sam, _public = open_session(host, port, session_id, private)
        try:
            sam.sendall(
                i2psam.stream_connect(session_id, peer_public, str(silent).lower())
            )
            if not silent:
                status = sam.recv_line(20.0)
                if not status.startswith("STREAM STATUS RESULT=OK"):
                    return 4
            return bidirectional_transfer(sam, send_data, expected_data)
        finally:
            sam.close()
    except (OSError, RuntimeError, TimeoutError, EOFError, ValueError) as error:
        print(f"i2plib connect failed: {type(error).__name__}: {error}", file=sys.stderr)
        return 3


def run_accept(
    host: str,
    port: int,
    send_path: Path,
    expect_path: Path,
    silent: bool,
    private_path: Path | None,
) -> int:
    send_data = send_path.read_bytes()
    expected_data = expect_path.read_bytes()
    private = private_path.read_text(encoding="ascii").strip() if private_path else None
    session_id = f"i2plib-accept-{os.getpid()}-{time.monotonic_ns()}"
    try:
        sam, public = open_session(host, port, session_id, private)
        try:
            sys.stdout.write(public + "\n")
            sys.stdout.flush()
            sam.sendall(i2psam.stream_accept(session_id, str(silent).lower()))
            if not silent:
                status = sam.recv_line(25.0)
                if not status.startswith("STREAM STATUS RESULT=OK"):
                    return 4
                peer_line = sam.recv_line(5.0)
                if not peer_line.startswith("DESTINATION="):
                    return 10
            return bidirectional_transfer(sam, send_data, expected_data)
        finally:
            sam.close()
    except (OSError, RuntimeError, TimeoutError, EOFError, ValueError) as error:
        print(f"i2plib accept failed: {type(error).__name__}: {error}", file=sys.stderr)
        return 3


def run_import(host: str, port: int, private_path: Path, public_path: Path) -> int:
    private = private_path.read_text(encoding="ascii").strip()
    expected_public = public_path.read_text(encoding="ascii").strip()
    try:
        sam, public = open_session(
            host, port, f"i2plib-import-{os.getpid()}-{time.monotonic_ns()}", private
        )
        sam.close()
        return 0 if public == expected_public else 8
    except (OSError, RuntimeError, TimeoutError, EOFError, ValueError) as error:
        print(f"i2plib import failed: {type(error).__name__}: {error}", file=sys.stderr)
        return 3


def main(argv: list[str]) -> int:
    if not argv or argv[0] not in {"connect", "accept", "import"}:
        raise SystemExit(__doc__)
    role = argv[0]
    try:
        if role == "connect" and len(argv) in {7, 8}:
            host, port, peer, send, expect, silent = argv[1:7]
            private = Path(argv[7]) if len(argv) == 8 else None
            return run_connect(
                host,
                int(port),
                peer,
                Path(send),
                Path(expect),
                parse_silent(silent),
                private,
            )
        if role == "accept" and len(argv) in {6, 7}:
            host, port, send, expect, silent = argv[1:6]
            private = Path(argv[6]) if len(argv) == 7 else None
            return run_accept(
                host,
                int(port),
                Path(send),
                Path(expect),
                parse_silent(silent),
                private,
            )
        if role == "import" and len(argv) == 5:
            return run_import(argv[1], int(argv[2]), Path(argv[3]), Path(argv[4]))
    except (OSError, ValueError):
        return 64
    raise SystemExit(__doc__)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
