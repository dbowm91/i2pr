"""Plan 093 source-classification metadata.

This module is the canonical reference for the Plan 093 source
classification tests. The metadata binds the pinned i2pd 2.60.0
revision to the exact file:line locations that produce the
diagnostic strings the launcher and the runner observe.

The metadata is consumed by
``tests/integration/ntcp2/harness/test_plan093.py``. A future source
revision change fails the test matrix until the metadata is updated
and the implementation review records the new revision.
"""

from __future__ import annotations

import re
from typing import Dict


# Plan 093 pinned source identifiers.

PINNED_I2PD_REVISION = "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e"
SCHEMA_MARKER = "i2pr-ntcp2-source-classification-v1"
REFERENCE = "i2pd"
REFERENCE_VERSION = "2.60.0"

# Diagnostic string -> canonical location name. The Plan 092
# misclassification labelled `Receive length read error` as a
# handshake SessionRequest read; Plan 093 corrects the
# classification to the data-phase length reader.
DIAGNOSTIC_LOCATIONS: Dict[str, str] = {
    "NTCP2: receive length read error: ": "data-phase length reader",
    "NTCP2: Receive length read error: ": "data-phase length reader",
    "NTCP2: receive read error: ": "data-phase body reader",
    "NTCP2: SessionRequest read error: ": "handshake SessionRequest reader",
    "NTCP2: SessionCreated read error: ": "handshake SessionCreated reader",
    "NTCP2: SessionConfirmed read error: ": "handshake SessionConfirmed reader",
}

# Canonical call-graph binding for the inbound session RouterInfo
# send. The pinned i2pd 2.60.0 source flow is:
#   NTCP2Session::Established -> transports.PeerConnected(session)
#   Transports::PeerConnected(incoming session) -> session->SendLocalRouterInfo()
CALL_GRAPH = (
    "NTCP2Session::Established",
    "transports.PeerConnected",
    "session->SendLocalRouterInfo",
    "NTCP2Session::HandleI2NPMsgsSent",
)


def diagnostic_location(diagnostic: str) -> str:
    """Return the canonical location name for the supplied diagnostic.

    Unknown diagnostics raise ``KeyError`` so the test matrix
    refuses to silently accept a new diagnostic.
    """
    if diagnostic not in DIAGNOSTIC_LOCATIONS:
        raise KeyError(f"unknown diagnostic: {diagnostic!r}")
    return DIAGNOSTIC_LOCATIONS[diagnostic]


def is_locked_revision(revision: str) -> bool:
    """Return ``True`` when ``revision`` matches the locked pinned SHA."""
    return bool(re.fullmatch(r"[0-9a-f]{40}", revision)) and revision == PINNED_I2PD_REVISION


def is_data_phase_diagnostic(diagnostic: str) -> bool:
    """Return ``True`` when the diagnostic originates in the data phase."""
    return DIAGNOSTIC_LOCATIONS.get(diagnostic) == "data-phase length reader" or \
        DIAGNOSTIC_LOCATIONS.get(diagnostic) == "data-phase body reader"


def is_handshake_diagnostic(diagnostic: str) -> bool:
    """Return ``True`` when the diagnostic originates in the handshake."""
    return DIAGNOSTIC_LOCATIONS.get(diagnostic) == "handshake SessionRequest reader" or \
        DIAGNOSTIC_LOCATIONS.get(diagnostic) == "handshake SessionCreated reader" or \
        DIAGNOSTIC_LOCATIONS.get(diagnostic) == "handshake SessionConfirmed reader"