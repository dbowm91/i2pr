"""Reference-only control triggers for non-deterministic auto-dial.

When a reference router does not automatically dial a sole imported peer
after RouterInfo exchange, the harness issues a private implementation-
specific trigger confined to the isolated namespace environment.

Each trigger:
- operates only within the disposable namespace;
- uses implementation-specific but protocol-valid mechanisms;
- does not bypass the reference router's authenticated transport;
- records a typed observation rather than log substring matching.

Pinned references:
- Java I2P 2.12.0 (revision 2800040deee9bb376567b671ef2e9c34cf3e30b6)
- i2pd 2.60.0 (revision f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e)

Plan 045 D4: ``send()`` performs the per-direction SAM v3 or HTTP
JSON-RPC dial inside the disposable namespace, returning a typed
observation. ``verify_auto_dial`` and ``issue_trigger`` remain typed
placeholders for callers that still want the auto/manual detection
helper surface.
"""

from __future__ import annotations

import json
import os
import socket
import subprocess
import time
from dataclasses import dataclass
from enum import Enum
from pathlib import Path

try:
    from .interop_topology import ProcessPlacement, TopologyContractError
except ImportError:  # pragma: no cover - direct harness-module execution
    from interop_topology import ProcessPlacement, TopologyContractError  # type: ignore

_JAVA_SAM_HELLO = b"HELLO VERSION MIN=3.0 MAX=3.0\n"
_I2P_BASE64_ALPHABET = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-~"


def _i2p_base64(data: bytes) -> str:
    """Encode ``data`` using the I2P base64 alphabet (RFC 4648 §5 mod:
    ``A-Z a-z 0-9 - ~``; no padding). Matches the destination format
    accepted by i2pd's SAM v3 ``DESTINATION=`` parameter.
    """
    if not data:
        return ""
    out = bytearray()
    view = memoryview(data)
    end = len(data)
    i = 0
    while i + 3 <= end:
        a = view[i]
        b = view[i + 1]
        c = view[i + 2]
        out.append(_I2P_BASE64_ALPHABET[a >> 2])
        out.append(_I2P_BASE64_ALPHABET[((a & 0x03) << 4) | (b >> 4)])
        out.append(_I2P_BASE64_ALPHABET[((b & 0x0F) << 2) | (c >> 6)])
        out.append(_I2P_BASE64_ALPHABET[c & 0x3F])
        i += 3
    rem = end - i
    if rem == 1:
        a = view[i]
        out.append(_I2P_BASE64_ALPHABET[a >> 2])
        out.append(_I2P_BASE64_ALPHABET[(a & 0x03) << 4])
    elif rem == 2:
        a = view[i]
        b = view[i + 1]
        out.append(_I2P_BASE64_ALPHABET[a >> 2])
        out.append(_I2P_BASE64_ALPHABET[((a & 0x03) << 4) | (b >> 4)])
        out.append(_I2P_BASE64_ALPHABET[(b & 0x0F) << 2])
    return out.decode("ascii")


class TriggerKind(Enum):
    JAVA_SAM_DIAL = "java-sam-dial"
    I2PD_HTTP_DIAL = "i2pd-http-dial"
    AUTO_DIAL_VERIFIED = "auto-dial-verified"
    UNSUPPORTED = "unsupported"


@dataclass(frozen=True)
class TriggerResult:
    kind: TriggerKind
    observed: bool
    description: str
    timed_out: bool = False


class ReferenceTrigger:
    @property
    def trigger_kind(self) -> TriggerKind:
        raise NotImplementedError

    def verify_auto_dial(self) -> TriggerResult:
        raise NotImplementedError

    def issue_trigger(self) -> TriggerResult:
        raise NotImplementedError

    def send(
        self,
        i2pr_is_initiator: bool,
        ref_endpoint: object,
        run_dir: "object",
        placement: ProcessPlacement | None = None,
    ) -> TriggerResult:
        """Plan 045 D4: per-direction SAM/HTTP dial within the namespace."""
        raise NotImplementedError


class JavaReferenceTrigger(ReferenceTrigger):
    """Java I2P auto-dial verification via SAM v3 bridge.

    Java I2P 2.12.0 with the sole imported peer RouterInfo will auto-dial
    when the router's integration manager finds a reachable peer in the
    netDb. The harness verifies this by checking the SAM v3 session
    status output for an established NTCP2 connection.

    If auto-dial is not deterministic, the harness issues a SAM v3
    SessionCreate + SessionStyle=STREAM command to initiate the connection.
    This exercises the Java router's authenticated NTCP2 transport
    without bypassing it.

    Pinned source: router/java/src/net/i2p/router/transport/ntcp/NTCP2Transport.java
    """

    # SAM v3 port the Java I2P router exposes in the disposable namespace.
    DEFAULT_SAM_PORT = 7656

    @property
    def trigger_kind(self) -> TriggerKind:
        return TriggerKind.JAVA_SAM_DIAL

    def verify_auto_dial(self) -> TriggerResult:
        return TriggerResult(
            kind=self.trigger_kind,
            observed=False,
            description="Java auto-dial verification pending; requires SAM v3 structured output",
        )

    def issue_trigger(self) -> TriggerResult:
        return TriggerResult(
            kind=self.trigger_kind,
            observed=False,
            description=(
                "SAM v3 SessionCreate STREAM trigger pending; "
                "exercises authenticated NTCP2 transport"
            ),
        )

    def send(
        self,
        i2pr_is_initiator: bool,
        ref_endpoint: object,
        run_dir: "object",
        placement: ProcessPlacement | None = None,
    ) -> TriggerResult:
        if i2pr_is_initiator:
            return TriggerResult(
                kind=self.trigger_kind,
                observed=False,
                description="java-i2pr direction auto-dials from i2pr side; SAM trigger not required",
                timed_out=False,
            )
        try:
            port = int(os.environ.get("I2PR_JAVA_SAM_PORT", str(self.DEFAULT_SAM_PORT)))
            host = getattr(ref_endpoint, "local_address", "127.0.0.1")
            payload = (
                'HELLO VERSION MIN=3.0 MAX=3.0\n'
                'SESSION CREATE STYLE=STREAM ID=i2pr-interop DESTINATION=TRANSIENT\n'
            ).encode("ascii")
            if placement is None:
                namespace = getattr(ref_endpoint, "namespace", None)
                if namespace is None:
                    return TriggerResult(
                        kind=self.trigger_kind,
                        observed=False,
                        description="java reference namespace missing",
                    )
                prefix = [] if os.geteuid() == 0 else ["sudo", "-n"]
                command = prefix + ["ip", "netns", "exec", str(namespace), "python3", "-c", _SAM_PROBE]
            else:
                try:
                    command = placement.command(["python3", "-c", _SAM_PROBE])
                except TopologyContractError as exc:
                    return TriggerResult(
                        kind=self.trigger_kind,
                        observed=False,
                        description=f"java-sam-trigger-placement-error: {exc.code}",
                    )
            completed = subprocess.run(
                command,
                input=json.dumps({"host": host, "port": port, "payload": payload.decode("ascii")}),
                capture_output=True,
                text=True,
                timeout=5.0,
                check=False,
            )
        except (subprocess.TimeoutExpired, OSError) as exc:
            return TriggerResult(
                kind=self.trigger_kind,
                observed=False,
                description=f"java-sam-trigger-error: {exc.__class__.__name__}",
                timed_out=isinstance(exc, subprocess.TimeoutExpired),
            )
        if completed.returncode != 0:
            return TriggerResult(
                kind=self.trigger_kind,
                observed=False,
                description=f"java-sam-trigger-failed: {completed.stderr.strip()[:64] or 'no-stderr'}",
            )
        return TriggerResult(
            kind=self.trigger_kind,
            observed=True,
            description="java-sam-trigger-stream-session-issued",
        )


class I2pdReferenceTrigger(ReferenceTrigger):
    """i2pd auto-dial verification via SAM v3 bridge.

    i2pd 2.60.0 with the sole imported peer RouterInfo will auto-dial
    when the transport manager finds a reachable peer. The harness
    verifies this by checking the SAM v3 ``SESSION STATUS`` response.

    If auto-dial is not deterministic, the harness issues a SAM v3
    ``SESSION CREATE STYLE=STREAM DESTINATION=<base64>`` command that
    creates a transient SAM destination and opens the authenticated
    NTCP2 connection to the i2pr RouterInfo. This exercises i2pd's
    authenticated NTCP2 transport without bypassing it.

    Note: ``?cmd=run_peer_test`` on the i2pd webconsole calls
    ``Transports::PeerTest`` which returns early when SSU2 is disabled;
    that endpoint is *not* an NTCP2 dial trigger.

    Pinned source: router/i2pd/Transports.cpp, router/i2pd/NTCP2Transport.cpp
    """

    DEFAULT_SAM_PORT = 7656

    @property
    def trigger_kind(self) -> TriggerKind:
        return TriggerKind.I2PD_HTTP_DIAL

    def verify_auto_dial(self) -> TriggerResult:
        return TriggerResult(
            kind=self.trigger_kind,
            observed=False,
            description="i2pd auto-dial verification pending; requires HTTP control structured output",
        )

    def issue_trigger(self) -> TriggerResult:
        return TriggerResult(
            kind=self.trigger_kind,
            observed=False,
            description=(
                "HTTP ConnectPeer trigger pending; "
                "exercises authenticated NTCP2 transport"
            ),
        )

    def send(
        self,
        i2pr_is_initiator: bool,
        ref_endpoint: object,
        run_dir: "object",
        placement: ProcessPlacement | None = None,
    ) -> TriggerResult:
        if i2pr_is_initiator:
            return TriggerResult(
                kind=self.trigger_kind,
                observed=False,
                description="i2pd-i2pr direction auto-dials from i2pr side; SAM trigger not required",
            )
        # Plan 045 D4: i2pd 2.60.0 has no I2PControl ConnectPeer endpoint
        # and the webconsole ``run_peer_test`` is an SSU2-only command
        # (``Transports::PeerTest`` returns early when SSU2 is disabled).
        # The only available path to force i2pd to initiate an NTCP2 dial
        # to an arbitrary destination is SAM v3 ``SESSION CREATE
        # STYLE=STREAM DESTINATION=<base64>``; i2pd's SAM layer opens a
        # transient destination and dials the target over NTCP2.
        destination_b64 = self._read_i2pr_destination_b64(run_dir)
        if not destination_b64:
            return TriggerResult(
                kind=self.trigger_kind,
                observed=False,
                description="i2pd-sam-trigger-destination-missing",
            )
        try:
            port = int(os.environ.get("I2PR_I2PD_SAM_PORT", str(self.DEFAULT_SAM_PORT)))
            host = "127.0.0.1"
            session_id = f"i2pr-interop-{int(time.time())}"
            hello = 'HELLO VERSION MIN=3.0 MAX=3.0\n'
            session_create = (
                f'SESSION CREATE STYLE=STREAM ID={session_id} DESTINATION={destination_b64}\n'
            )
            if placement is None:
                namespace = getattr(ref_endpoint, "namespace", None)
                if namespace is None:
                    return TriggerResult(
                        kind=self.trigger_kind,
                        observed=False,
                        description="i2pd reference namespace missing",
                    )
                prefix = [] if os.geteuid() == 0 else ["sudo", "-n"]
                command = prefix + [
                    "ip", "netns", "exec", str(namespace),
                    "python3", "-u", "-c", _SAM_PROBE,
                ]
            else:
                try:
                    command = placement.command(["python3", "-u", "-c", _SAM_PROBE])
                except TopologyContractError as exc:
                    return TriggerResult(
                        kind=self.trigger_kind,
                        observed=False,
                        description=f"i2pd-sam-trigger-placement-error: {exc.code}",
                    )
            completed = subprocess.run(
                command,
                input=json.dumps({
                    "host": host,
                    "port": port,
                    "hello": hello,
                    "session_create": session_create,
                }),
                capture_output=True,
                text=True,
                timeout=8.0,
                check=False,
            )
        except (subprocess.TimeoutExpired, OSError) as exc:
            return TriggerResult(
                kind=self.trigger_kind,
                observed=False,
                description=f"i2pd-sam-trigger-error: {exc.__class__.__name__}",
                timed_out=isinstance(exc, subprocess.TimeoutExpired),
            )
        if completed.returncode != 0:
            return TriggerResult(
                kind=self.trigger_kind,
                observed=False,
                description=(
                    f"i2pd-sam-trigger-failed: rc={completed.returncode} "
                    f"err={(completed.stderr.strip()[:200] or 'no-stderr')!r} "
                    f"out={(completed.stdout.strip()[:200] or 'no-stdout')!r}"
                ),
            )
        probe_payload: dict[str, object] = {}
        try:
            probe_payload = json.loads(completed.stdout)
        except json.JSONDecodeError:
            probe_payload = {}
        session_reply = str(probe_payload.get("session_reply", ""))
        hello_reply = str(probe_payload.get("hello_reply", ""))
        if "SESSION STATUS RESULT=OK" not in session_reply:
            return TriggerResult(
                kind=self.trigger_kind,
                observed=False,
                description=(
                    f"i2pd-sam-trigger-rejected: hello={hello_reply[:48]!r} "
                    f"session={session_reply[:128]!r}"
                ),
            )
        return TriggerResult(
            kind=self.trigger_kind,
            observed=True,
            description="i2pd-sam-stream-session-issued",
        )

    @staticmethod
    def _read_i2pr_destination_b64(run_dir: "object") -> str:
        """Extract the i2pr public destination (RouterIdentity in I2P
        base64) from ``<run_dir>/i2pr/state/router.info``. Plan 045 D4
        requires the reference to dial an explicit destination; the SAM
        v3 ``DESTINATION=`` parameter takes a RouterIdentity-encoded
        string. i2pr uses Certificate::Key over X25519/Ed25519: the
        leading 384 bytes are the key area, followed by 7 bytes of
        ``Certificate::Key`` header (1 type-code + 2 payload length + 4
        signing/crypto type codes), for a 391-byte identity total.
        """
        try:
            ri_path = Path(run_dir) / "i2pr" / "state" / "router.info"
            if not ri_path.exists():
                return ""
            data = ri_path.read_bytes()
        except OSError:
            return ""
        if len(data) < 391:
            return ""
        identity_bytes = data[:391]
        return _i2p_base64(identity_bytes)


_TRIGGER_REGISTRY: dict[str, type[ReferenceTrigger]] = {
    "java_i2p": JavaReferenceTrigger,
    "i2pd": I2pdReferenceTrigger,
}


def select_trigger(reference: str) -> ReferenceTrigger:
    cls = _TRIGGER_REGISTRY.get(reference)
    if cls is None:
        return _UnsupportedTrigger()
    return cls()


class _UnsupportedTrigger(ReferenceTrigger):
    @property
    def trigger_kind(self) -> TriggerKind:
        return TriggerKind.UNSUPPORTED

    def verify_auto_dial(self) -> TriggerResult:
        return TriggerResult(
            kind=self.trigger_kind,
            observed=False,
            description="unsupported reference implementation",
        )

    def issue_trigger(self) -> TriggerResult:
        return TriggerResult(
            kind=self.trigger_kind,
            observed=False,
            description="unsupported reference implementation",
        )

    def send(
        self,
        i2pr_is_initiator: bool,
        ref_endpoint: object,
        run_dir: "object",
        placement: ProcessPlacement | None = None,
    ) -> TriggerResult:
        return TriggerResult(
            kind=self.trigger_kind,
            observed=False,
            description="unsupported reference implementation",
        )


_SAM_PROBE = """
import json
import socket
import sys

config = json.loads(sys.stdin.read())
host = config["host"]
port = int(config["port"])
hello = config["hello"].encode("ascii")
session_create = config["session_create"].encode("ascii")

sock = socket.create_connection((host, port), timeout=3)
sock.settimeout(5)


def _recv_one_response(sock: socket.socket) -> str:
    buf = bytearray()
    while not buf.endswith(b"\\n"):
        chunk = sock.recv(4096)
        if not chunk:
            break
        buf.extend(chunk)
        if len(buf) >= 4096:
            break
    return buf.decode("ascii", errors="replace")


sock.sendall(hello)
hello_reply = _recv_one_response(sock)

sock.sendall(session_create)
session_reply = _recv_one_response(sock)

sock.close()
sys.stdout.write(json.dumps({"hello_reply": hello_reply, "session_reply": session_reply}))
"""
