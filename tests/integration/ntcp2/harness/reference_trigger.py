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
"""

from __future__ import annotations

from dataclasses import dataclass
from enum import Enum


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


class ReferenceTrigger:
    @property
    def trigger_kind(self) -> TriggerKind:
        raise NotImplementedError

    def verify_auto_dial(self) -> TriggerResult:
        raise NotImplementedError

    def issue_trigger(self) -> TriggerResult:
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


class I2pdReferenceTrigger(ReferenceTrigger):
    """i2pd auto-dial verification via HTTP control interface.

    i2pd 2.60.0 with the sole imported peer RouterInfo will auto-dial
    when the transport manager finds a reachable peer. The harness
    verifies this by checking the HTTP /jsonrpc status endpoint for an
    established NTCP2 session.

    If auto-dial is not deterministic, the harness issues an HTTP
    /jsonrpc ConnectPeer command to initiate the connection. This
    exercises the i2pd router's authenticated NTCP2 transport
    without bypassing it.

    Pinned source: router/i2pd/Transports.cpp, router/i2pd/NTCP2Transport.cpp
    """

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
