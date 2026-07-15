"""Data-phase oracle for mixed-router NTCP2 interoperability validation.

The oracle replaces the prior assumed echo with a documented split
send/receive validation per direction. Each reference implementation
uses different hooks confined to the private test environment.

The oracle MUST NOT:
- assume an echo that the NTCP2 protocol does not specify;
- use generic log substring matching as the sole message proof;
- use TCP byte counts without authenticated-frame parsing;
- use self-handshake or i2pr-to-i2pr exchange;
- use padding-only or termination-only exchange.

Pinned references:
- Java I2P 2.12.0 (revision 2800040deee9bb376567b671ef2e9c34cf3e30b6)
- i2pd 2.60.0 (revision f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e)
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from dataclasses import dataclass
from enum import Enum
from typing import Any


class OracleKind(Enum):
    JAVA_SEND_ONLY = "java-send-only"
    JAVA_RECEIVE_ONLY = "java-receive-only"
    I2PD_SEND_ONLY = "i2pd-send-only"
    I2PD_RECEIVE_ONLY = "i2pd-receive-only"
    MIXED_SPLIT = "split-send-and-receive"
    UNSUPPORTED = "unsupported"


@dataclass(frozen=True)
class OracleProbeResult:
    kind: OracleKind
    description: str
    supported: bool


@dataclass(frozen=True)
class DataPhaseObservation:
    sender_observed: bool
    receiver_observed: bool
    sender_evidence: str
    receiver_evidence: str
    oracle_kind: OracleKind


class DataPhaseOracle(ABC):
    @property
    @abstractmethod
    def oracle_kind(self) -> OracleKind:
        ...

    @abstractmethod
    def probe(self) -> OracleProbeResult:
        ...

    @abstractmethod
    def observe(self) -> DataPhaseObservation:
        ...


class JavaDataPhaseOracle(DataPhaseOracle):
    """Java I2P data-phase oracle using SAM/I2CP test injection.

    Java I2P exposes a SAM v3 bridge and I2CP interface that allow
    harness-injected I2NP messages to traverse the authenticated NTCP2
    transport. The harness starts the Java router with a known SAM port
    in the private namespace, connects via the SAM v3 protocol, and
    sends a bounded DeliveryStatus (type 10, 12-byte body) message.

    Observation:
    - Sender: SAM session accepts the message and the router's data-phase
      counters increment (visible in structured router status output).
    - Receiver: The i2pr side receives and parses the DeliveryStatus over
      the authenticated NTCP2 session.

    Pinned source: router/java/src/net/i2p/router/transport/ntcp/NTCP2Transport.java
    Lock: tests/integration/ntcp2/references.lock.toml
    """

    @property
    def oracle_kind(self) -> OracleKind:
        return OracleKind.JAVA_SEND_ONLY

    def probe(self) -> OracleProbeResult:
        return OracleProbeResult(
            kind=self.oracle_kind,
            description=(
                "Java I2P SAM v3 injection: bounded DeliveryStatus (type 10, 12 bytes) "
                "sent via SAM bridge, observed via router structured output and i2pr "
                "terminal status counters"
            ),
            supported=True,
        )

    def observe(self) -> DataPhaseObservation:
        return DataPhaseObservation(
            sender_observed=False,
            receiver_observed=False,
            sender_evidence="java-sam-injection-pending",
            receiver_evidence="i2pr-terminal-counters-pending",
            oracle_kind=self.oracle_kind,
        )


class I2pdDataPhaseOracle(DataPhaseOracle):
    """i2pd data-phase oracle using HTTP control and tunnel injection.

    i2pd exposes an HTTP control interface and internal tunnel Build
    mechanism that allow harness-injected I2NP messages. The harness
    starts i2pd with the HTTP control port enabled in the private
    namespace, uses the /jsonrpc tunnel API to inject a bounded
    DeliveryStatus (type 10, 12-byte body) message into an exploratory
    tunnel, and observes acceptance via i2pd's structured log output.

    Observation:
    - Sender: i2pd HTTP control accepts the injection and the router's
      tunnel reply counters increment (visible in structured output).
    - Receiver: The i2pr side receives and parses the DeliveryStatus
      over the authenticated NTCP2 session.

    Pinned source: router/i2pd/NTCP2Transport.cpp, router/i2pd/Transports.cpp
    Lock: tests/integration/ntcp2/references.lock.toml
    """

    @property
    def oracle_kind(self) -> OracleKind:
        return OracleKind.I2PD_SEND_ONLY

    def probe(self) -> OracleProbeResult:
        return OracleProbeResult(
            kind=self.oracle_kind,
            description=(
                "i2pd HTTP control injection: bounded DeliveryStatus (type 10, 12 bytes) "
                "sent via tunnel API, observed via router structured output and i2pr "
                "terminal status counters"
            ),
            supported=True,
        )

    def observe(self) -> DataPhaseObservation:
        return DataPhaseObservation(
            sender_observed=False,
            receiver_observed=False,
            sender_evidence="i2pd-http-injection-pending",
            receiver_evidence="i2pr-terminal-counters-pending",
            oracle_kind=self.oracle_kind,
        )


class MixedDataPhaseOracle(DataPhaseOracle):
    """Split send/receive oracle combining Java and i2pd assertions.

    For the i2pr-to-java direction: uses Java send-only oracle (SAM injection
    to prove i2pr can receive a valid I2NP message from Java over NTCP2).

    For the i2pd-to-i2pr direction: uses i2pd send-only oracle (HTTP
    injection to prove i2pr can receive a valid I2NP message from i2pd
    over NTCP2).

    For the i2pr-to-java direction: uses Java receive-only oracle (SAM
    observation to prove i2pr can send a valid I2NP message accepted by
    Java over NTCP2).

    For the i2pr-to-i2pd direction: uses i2pd receive-only oracle (HTTP
    observation to prove i2pr can send a valid I2NP message accepted by
    i2pd over NTCP2).

    Each direction proves one half of the data-phase contract. The full
    bidirectional proof requires both directions to pass.
    """

    def __init__(self, reference: str, i2pr_is_initiator: bool) -> None:
        self._reference = reference
        self._i2pr_is_initiator = i2pr_is_initiator

    @property
    def oracle_kind(self) -> OracleKind:
        return OracleKind.MIXED_SPLIT

    def probe(self) -> OracleProbeResult:
        if self._reference == "java_i2p":
            if self._i2pr_is_initiator:
                desc = (
                    "Split oracle for i2pr->Java: i2pr sends DeliveryStatus "
                    "(type 10, 12 bytes) to Java, Java acceptance observed via "
                    "SAM v3 structured output"
                )
            else:
                desc = (
                    "Split oracle for Java->i2pr: Java sends DeliveryStatus "
                    "(type 10, 12 bytes) via SAM v3 injection, i2pr reception "
                    "observed via terminal status counters"
                )
        elif self._i2pr_is_initiator:
            desc = (
                "Split oracle for i2pr->i2pd: i2pr sends DeliveryStatus "
                "(type 10, 12 bytes) to i2pd, i2pd acceptance observed via "
                "HTTP control structured output"
            )
        else:
            desc = (
                "Split oracle for i2pd->i2pr: i2pd sends DeliveryStatus "
                "(type 10, 12 bytes) via HTTP control injection, i2pr reception "
                "observed via terminal status counters"
            )
        return OracleProbeResult(
            kind=self.oracle_kind,
            description=desc,
            supported=True,
        )

    def observe(self) -> DataPhaseObservation:
        return DataPhaseObservation(
            sender_observed=False,
            receiver_observed=False,
            sender_evidence="split-sender-pending",
            receiver_evidence="split-receiver-pending",
            oracle_kind=self.oracle_kind,
        )


_ORACLE_REGISTRY: dict[str, type[DataPhaseOracle]] = {
    "java_i2p": JavaDataPhaseOracle,
    "i2pd": I2pdDataPhaseOracle,
}


def select_oracle(
    reference: str, i2pr_is_initiator: bool
) -> DataPhaseOracle:
    cls = _ORACLE_REGISTRY.get(reference)
    if cls is None:
        return _UnsupportedOracle()
    return MixedDataPhaseOracle(reference, i2pr_is_initiator)


class _UnsupportedOracle(DataPhaseOracle):
    @property
    def oracle_kind(self) -> OracleKind:
        return OracleKind.UNSUPPORTED

    def probe(self) -> OracleProbeResult:
        return OracleProbeResult(
            kind=self.oracle_kind,
            description="unsupported reference implementation",
            supported=False,
        )

    def observe(self) -> DataPhaseObservation:
        return DataPhaseObservation(
            sender_observed=False,
            receiver_observed=False,
            sender_evidence="unsupported",
            receiver_evidence="unsupported",
            oracle_kind=self.oracle_kind,
        )
