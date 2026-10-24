#!/usr/bin/env python3
"""Plan 495 -- the offline peer-eligibility preflight and the first-failure-owner gate.

This script is the *enforcement* half of the external qualification gate whose
model is canonical in ``specs/QUALIFICATION-GATES.md``. It answers one
question, offline, before any process starts:

    could the pinned stock peer admit our RouterInfo, and could its own
    selection state choose us?

Every predicate it evaluates is derived from the pinned reference router's own
source and carries that source's file and line. A predicate whose value cannot
be determined offline is reported ``unknown``, and an unknown predicate
*refuses* a lane-ready verdict: the checker never guesses, and it never turns a
peer's silence into a verdict about ``i2pr``.

The checker is pure. It opens no socket, starts no process and touches no
network. ``audit_purity()`` proves that over this file's own AST, and
``--self-test`` carries the inverted probes that prove the proof works.

Modes:

    --self-test                     deterministic in-memory fixtures, no I/O
    --preflight --profile FILE      evaluate a lab RouterInfo against a family
    --evidence FILE                 validate a lane evidence record
    --hygiene FILE                  validate a peer-state hygiene record
    --withholding --profile FILE    run the deliberate-withholding control
    --profile-drift                 compare the checked lab profile with the
                                    documented one in specs/QUALIFICATION-GATES.md

Exit codes: 0 = the verdict is the expected one, 1 = a violation, 2 = the
checker could not run and refuses to report success.
"""

from __future__ import annotations

import argparse
import ast
import hashlib
import json
import re
import sys
from pathlib import Path

# ---------------------------------------------------------------------------
# Pins and vocabulary.
# ---------------------------------------------------------------------------

I2PD_PIN = "635b013a612ff47278ef02acf8580a28e10e26c5"
I2PD_VERSION = "2.61.0"
JAVA_PIN = "9134f808337b401e8e53c73734c81fab04280c9d"
JAVA_VERSION = "2.13.0"

FAMILIES = ("i2pd", "java-i2p")

#: Predicate states.
SATISFIED = "satisfied"
UNSATISFIED = "unsatisfied"
WITHHELD = "deliberately-unsatisfied"
UNKNOWN = "unknown"

#: The outcome taxonomy, exactly as published in the gate model.
OUTCOME_CLASSES = (
    "pass",
    "i2pr-defect",
    "peer-admission-refused",
    "peer-selection-refused",
    "peer-state-poisoned",
    "environment-unavailable",
    "timeout",
)

#: Outcomes that are *not* a verdict about ``i2pr`` in either direction.
NON_VERDICT_OUTCOMES = (
    "peer-admission-refused",
    "peer-selection-refused",
    "peer-state-poisoned",
    "environment-unavailable",
    "timeout",
)

#: The orthogonal cause of an outcome. The taxonomy says *where* a refusal
#: happened; the cause says *whose* artifact was responsible. Without it a
#: lab-profile bug would be filed as a peer-side condition -- the exact
#: misreading Plan 495 exists to prevent.
CAUSES = (
    "i2pr-lab-profile",
    "i2pr-implementation",
    "peer-state",
    "intentional-withholding",
    "unattributed",
)

#: The causes that may accompany a peer-side outcome class. A peer-side
#: outcome may still be *our* fault -- the lab profile is our artifact and
#: the preflight says so -- but it is never the daemon's own code path,
#: which is what `i2pr-defect` means.
PEER_OUTCOME_CAUSES = (
    "i2pr-lab-profile",
    "peer-state",
    "intentional-withholding",
    "unattributed",
)

#: The claim ladder. T0..T8.
TIERS = tuple(range(0, 9))
TIER_NAMES = {
    0: "peer admitted and persisted our signed RouterInfo",
    1: "peer's own selection state admitted us as a dial target",
    2: "an authenticated transport session was established",
    3: "an I2NP message travelled in one direction",
    4: "the peer answered a request we originated",
    5: "the peer initiated a fresh session to us",
    6: "our daemon's own product behaviour changed",
    7: "a tunnel was built",
    8: "external SAM/Streaming acceptance",
}
TIER_STATES = ("pass", "fail", "not-attempted")

#: The lab profile, checked here and documented in specs/QUALIFICATION-GATES.md.
#: A drift between the two fails the ``--profile-drift`` mode.
LAB_PROFILE_ROWS = (
    (
        "netId",
        "2",
        "Peer ingest gate; a missing netId marks the RouterInfo unreachable "
        "without logging and the peer's NetDB refuses to persist it.",
        "i2pd, java-i2p",
    ),
    (
        "router.version",
        "0.9.62",
        "Clears the peer's dial floor (0.9.58), its floodfill-eligibility "
        "floor (0.9.62) and its peer-test floor (0.9.62).",
        "i2pd, java-i2p",
    ),
    (
        "caps",
        "fOR",
        "'f' makes us a floodfill exploration target, 'R' is required by "
        "Java's profile-creation predicate, 'O' declares high bandwidth so "
        "the dial floor is met even below 0.9.58.",
        "i2pd, java-i2p",
    ),
    (
        "address caps",
        "46",
        "Publishes the IPv4 (and IPv6) SSU2 address rather than an "
        "introducer-only entry.",
        "i2pd, java-i2p",
    ),
    (
        "signing key",
        "ed25519",
        "Java excludes DSA peers from selection; i2pd excludes DSA from "
        "floodfill eligibility.",
        "i2pd, java-i2p",
    ),
    (
        "encryption",
        "ECIES_X25519",
        "Java's peer-exclusion rule requires ECIES_X25519.",
        "java-i2p",
    ),
    (
        "published address",
        "the bound RFC1918 endpoint",
        "An honest statement about the lab: the socket really is bound and "
        "really is reachable there.",
        "i2pd, java-i2p",
    ),
)

LAB_PROFILE_SPEC = "specs/QUALIFICATION-GATES.md"


class GateError(Exception):
    """A named failure. Every failure carries a reason code."""

    def __init__(self, code: str) -> None:
        super().__init__(code)
        self.code = code


# ---------------------------------------------------------------------------
# Version helpers. The two references do not parse versions the same way, so
# neither may be assumed.
# ---------------------------------------------------------------------------


def i2pd_version_number(value: object) -> int | None:
    """The digit-only number i2pd derives from a ``router.version`` property.

    ``RouterInfo::ReadFromBuffer`` keeps every digit and drops everything
    else, so ``"0.9.62"`` becomes ``962`` -- which is also what
    ``MAKE_VERSION_NUMBER(0, 9, 62)`` evaluates to.
    """
    if not isinstance(value, str):
        return None
    digits = "".join(ch for ch in value if ch.isdigit())
    if not digits:
        return None
    return int(digits)


def dotted_version(value: object) -> tuple[int, ...] | None:
    """A dotted version as a comparable tuple, for Java's ``VersionComparator``."""
    if not isinstance(value, str):
        return None
    parts = value.strip().split(".")
    if not parts or not all(part.isdigit() for part in parts):
        return None
    return tuple(int(part) for part in parts)


def version_at_least(value: object, floor: object) -> bool | None:
    """Dotted comparison; ``None`` when either side is unparseable."""
    left = dotted_version(value)
    right = dotted_version(floor)
    if left is None or right is None:
        return None
    width = max(len(left), len(right))
    left += (0,) * (width - len(left))
    right += (0,) * (width - len(right))
    return left >= right


def caps_contain(caps: object, letter: str) -> bool | None:
    """Whether a caps string carries a capability letter."""
    if not isinstance(caps, str):
        return None
    return letter in caps


# ---------------------------------------------------------------------------
# The predicate catalogue.
#
# Each predicate names the pinned source it was derived from. `owner` says
# whose artifact the predicate is about: `i2pr-profile` for a property of the
# RouterInfo we publish, `peer-state` for a property of the peer's own local
# state, and `peer-policy` for a rule the peer applies to us.
# ---------------------------------------------------------------------------


class Predicate:
    """One source-derived condition the peer applies to us.

    `kind` says *when* the peer applies it:

    ``admission``
        The peer's ingest gate -- whether it will accept and persist our
        RouterInfo at all. Every admission predicate is required.
    ``dial``
        The conditions under which the peer will establish a transport
        session to us. Every dial predicate is required.
    ``explore``
        The conditions under which the peer's own selection state chooses
        us as a *target*. These are grouped into named routes: the peer
        may reach us by more than one route, and at least one route must
        be available.

    `owner` says whose artifact the predicate is about: ``i2pr-profile``
    for a property of the RouterInfo we publish, ``peer-state`` for a
    property of the peer's own local state, ``peer-policy`` for a rule
    the peer applies to us.
    """

    __slots__ = (
        "identifier", "family", "kind", "route", "requirement", "source",
        "owner", "evaluate",
    )

    def __init__(
        self,
        identifier: str,
        family: str,
        kind: str,
        requirement: str,
        source: str,
        owner: str,
        evaluate,
        route: str | None = None,
    ) -> None:
        self.identifier = identifier
        self.family = family
        self.kind = kind
        self.route = route
        self.requirement = requirement
        self.source = source
        self.owner = owner
        self.evaluate = evaluate


class Context:
    """The candidate lab RouterInfo plus the pinned peer's configuration."""

    __slots__ = ("profile", "peer")

    def __init__(self, profile: dict, peer: dict) -> None:
        self.profile = profile
        self.peer = peer

    def published_addresses(self) -> list[dict]:
        value = self.profile.get("published_addresses")
        if not isinstance(value, list):
            return []
        return [row for row in value if isinstance(row, dict)]

    def address_for(self, transport: str) -> dict | None:
        for row in self.published_addresses():
            if row.get("transport") == transport:
                return row
        return None


# -- i2pd admission --------------------------------------------------------


def _i2pd_signature(ctx: Context) -> str:
    value = ctx.profile.get("signature_valid")
    if value is None:
        return UNKNOWN
    return SATISFIED if value is True else UNSATISFIED


def _i2pd_netid_present(ctx: Context) -> str:
    value = ctx.profile.get("net_id")
    if value is None:
        return UNSATISFIED
    if isinstance(value, bool) or not isinstance(value, int):
        return UNKNOWN
    return SATISFIED


def _i2pd_netid_matches(ctx: Context) -> str:
    value = ctx.profile.get("net_id")
    peer_net = ctx.peer.get("network_id")
    if value is None:
        return UNSATISFIED
    if isinstance(value, bool) or not isinstance(value, int):
        return UNKNOWN
    if isinstance(peer_net, bool) or not isinstance(peer_net, int):
        return UNKNOWN
    return SATISFIED if value == peer_net else UNSATISFIED


def _i2pd_version_present(ctx: Context) -> str:
    value = ctx.profile.get("router_version")
    if value is None:
        return UNSATISFIED
    number = i2pd_version_number(value)
    if number is None:
        return UNKNOWN
    return SATISFIED if number > 0 else UNSATISFIED


def _i2pd_transport_present(ctx: Context) -> str:
    supported = ctx.peer.get("supported_transports")
    if not isinstance(supported, list):
        return UNKNOWN
    for row in ctx.published_addresses():
        if row.get("transport") in supported:
            return SATISFIED
    return UNSATISFIED if ctx.published_addresses() else UNSATISFIED


def _i2pd_family_signature(ctx: Context) -> str:
    value = ctx.profile.get("family_signature_valid")
    if value is None:
        # No `family` property is published, so there is no family signature to
        # verify. i2pd only verifies a family when the property is present.
        return SATISFIED
    return SATISFIED if value is True else UNSATISFIED


# -- i2pd selection --------------------------------------------------------


def _i2pd_not_unreachable(ctx: Context) -> str:
    """The aggregate the peer's dial path checks.

    ``Transports::PostMessages`` refuses a router that ``IsUnreachable()``,
    and i2pd marks a RouterInfo unreachable when the netId is absent or
    mismatched, when the version is absent, when no transport is published,
    or when a published family signature fails.
    """
    states = (
        _i2pd_netid_matches(ctx),
        _i2pd_version_present(ctx),
        _i2pd_transport_present(ctx),
        _i2pd_signature(ctx),
    )
    if UNKNOWN in states:
        return UNKNOWN
    return UNSATISFIED if UNSATISFIED in states else SATISFIED


def _i2pd_dial_version_floor(ctx: Context) -> str:
    """``version >= min_allowed_version`` or a high-bandwidth declaration."""
    number = i2pd_version_number(ctx.profile.get("router_version"))
    floor = i2pd_version_number(ctx.peer.get("min_allowed_version"))
    if number is None or floor is None:
        return UNKNOWN
    if number >= floor:
        return SATISFIED
    caps = ctx.profile.get("caps")
    if not isinstance(caps, str):
        return UNKNOWN
    # 'O' high bandwidth, 'P'/'X' extra bandwidth all set eHighBandwidth.
    if any(letter in caps for letter in ("O", "P", "X")):
        return SATISFIED
    return UNSATISFIED


def _i2pd_reachable_from_peer(ctx: Context) -> str:
    """A published address whose transport the peer supports.

    ``IsReachableFrom`` is a transport-set intersection; an introducer-only
    address is reachable but not published, and i2pd will not dial it.
    """
    supported = ctx.peer.get("supported_transports")
    if not isinstance(supported, list):
        return UNKNOWN
    addresses = ctx.published_addresses()
    if not addresses:
        return UNSATISFIED
    for row in addresses:
        if row.get("transport") not in supported:
            continue
        address_caps = row.get("address_caps")
        if address_caps is None:
            return UNKNOWN
        if not isinstance(address_caps, str):
            return UNKNOWN
        # 'C' marks an introducer-only address: reachable, but never dialled.
        if "C" in address_caps:
            continue
        return SATISFIED
    return UNSATISFIED


def _i2pd_floodfill_declared(ctx: Context) -> str:
    present = caps_contain(ctx.profile.get("caps"), "f")
    if present is None:
        return UNKNOWN
    return SATISFIED if present else UNSATISFIED


def _i2pd_floodfill_eligible(ctx: Context) -> str:
    """The conditions under which i2pd files us as a floodfill.

    ``RouterInfo::IsEligibleFloodfill`` requires the version floor, a
    published (or reachable-then-published) IPv4 address and a non-DSA
    signing key. Below the floor the peer never files us as a floodfill,
    so an exploratory request can never target us.
    """
    number = i2pd_version_number(ctx.profile.get("router_version"))
    floor = i2pd_version_number(ctx.peer.get("min_floodfill_version"))
    if number is None or floor is None:
        return UNKNOWN
    if number < floor:
        return UNSATISFIED
    signing = ctx.profile.get("signing_key_type")
    if signing is None:
        return UNKNOWN
    if str(signing).lower() in ("dsa", "dsa_sha1", "dsa-sha1"):
        return UNSATISFIED
    addresses = ctx.published_addresses()
    if not addresses:
        return UNSATISFIED
    published_v4 = False
    reachable_v4 = False
    published_v6 = False
    for row in addresses:
        if row.get("transport") != "SSU2":
            continue
        address_caps = row.get("address_caps")
        if not isinstance(address_caps, str):
            return UNKNOWN
        if "C" in address_caps:
            continue
        if "4" in address_caps:
            reachable_v4 = True
            published_v4 = True
        if "6" in address_caps:
            published_v6 = True
    if published_v4 or (reachable_v4 and published_v6):
        return SATISFIED
    return UNSATISFIED


def _i2pd_peer_test_cap(ctx: Context) -> str:
    """The published SSU2 testing cap the peer's peer-test selector needs.

    i2pd strips the ``eSSUTesting`` address cap from a RouterInfo whose
    version is below ``NETDB_MIN_PEER_TEST_VERSION``, so a profile below
    that floor can never be picked for a peer-test session.
    """
    number = i2pd_version_number(ctx.profile.get("router_version"))
    floor = i2pd_version_number(ctx.peer.get("min_peer_test_version"))
    if number is None or floor is None:
        return UNKNOWN
    address = ctx.address_for("SSU2")
    if address is None:
        return UNSATISFIED
    address_caps = address.get("address_caps")
    if not isinstance(address_caps, str):
        return UNKNOWN
    if "B" not in address_caps:
        return UNSATISFIED
    if number < floor:
        # i2pd strips the cap before the peer ever sees it.
        return UNSATISFIED
    return SATISFIED


# -- Java I2P admission ----------------------------------------------------


def _java_signature(ctx: Context) -> str:
    value = ctx.profile.get("signature_valid")
    if value is None:
        return UNKNOWN
    return SATISFIED if value is True else UNSATISFIED


def _java_netid(ctx: Context) -> str:
    value = ctx.profile.get("net_id")
    peer_net = ctx.peer.get("network_id")
    if value is None:
        return UNSATISFIED
    if isinstance(value, bool) or not isinstance(value, int):
        return UNKNOWN
    if isinstance(peer_net, bool) or not isinstance(peer_net, int):
        return UNKNOWN
    return SATISFIED if value == peer_net else UNSATISFIED


def _java_version_present(ctx: Context) -> str:
    value = ctx.profile.get("router_version")
    if value is None:
        return UNSATISFIED
    if dotted_version(value) is None:
        return UNKNOWN
    return SATISFIED


def _java_transport_present(ctx: Context) -> str:
    supported = ctx.peer.get("supported_transports")
    if not isinstance(supported, list):
        return UNKNOWN
    for row in ctx.published_addresses():
        if row.get("transport") in supported:
            return SATISFIED
    return UNSATISFIED


def _java_ban_surface(ctx: Context) -> str:
    """The profile must not land on one of the peer's ban triggers.

    Java's NTCP2 inbound-establish path bans a peer whose RouterInfo
    carries an exact (caps, version) pair it considers abusive. The
    cooldown is 30 to 120 minutes, which is long enough to make every
    later attempt in the same lane fail. This predicate is checked
    offline precisely because the cost of getting it wrong is the whole
    attempt budget.
    """
    caps = ctx.profile.get("caps")
    version = ctx.profile.get("router_version")
    if not isinstance(caps, str) or not isinstance(version, str):
        return UNKNOWN
    triggers = ctx.peer.get("ban_triggers")
    if not isinstance(triggers, list):
        return UNKNOWN
    for trigger in triggers:
        if not isinstance(trigger, dict):
            return UNKNOWN
        trigger_caps = trigger.get("caps")
        trigger_version = trigger.get("version")
        if not isinstance(trigger_caps, str) or not isinstance(trigger_version, str):
            return UNKNOWN
        if caps == trigger_caps and version == trigger_version:
            return UNSATISFIED
        if "L" in trigger_caps and "L" in caps and version == trigger_version:
            return UNSATISFIED
    return SATISFIED


# -- Java I2P selection ----------------------------------------------------


def _java_profile_creation(ctx: Context) -> str:
    """``ProfileManagerImpl.shouldCreate(caps)`` -- the only route to a profile.

    A Java router creates a profile for a peer only when the peer's
    RouterInfo carries ``R`` and either ``f`` or a capacity/congestion
    combination the local router does not exclude. Without a profile the
    peer is never selected, which is the exact state Plan 230 recorded as
    ``P230-D-ELIGIBLE-BUT-NO-PROFILE``.
    """
    caps = ctx.profile.get("caps")
    if not isinstance(caps, str):
        return UNKNOWN
    if "R" not in caps:
        return UNSATISFIED
    if "f" in caps:
        return SATISFIED
    floodfill_enabled = ctx.peer.get("floodfill_enabled")
    share_bandwidth = ctx.peer.get("max_share_bandwidth_kbps")
    if not isinstance(floodfill_enabled, bool):
        return UNKNOWN
    if share_bandwidth is None:
        return UNKNOWN
    if isinstance(share_bandwidth, bool) or not isinstance(share_bandwidth, int):
        return UNKNOWN
    low_bandwidth_allowed = "L" not in caps or (
        not floodfill_enabled and share_bandwidth < 128
    )
    if not low_bandwidth_allowed:
        return UNSATISFIED
    if "E" in caps or "G" in caps:
        return UNSATISFIED
    return SATISFIED


def _java_not_excluded(ctx: Context) -> str:
    """``TunnelPeerSelector.shouldExclude`` -- the peer's exclusion rules.

    A peer is excluded when its caps carry an excluded letter, when its
    caps say nothing but reachability, when it signs with DSA, when it is
    not ECIES_X25519, or when it is below the peer's minimum version.
    """
    caps = ctx.profile.get("caps")
    version = ctx.profile.get("router_version")
    if not isinstance(caps, str) or not isinstance(version, str):
        return UNKNOWN
    excluded_caps = ctx.peer.get("exclude_caps")
    if not isinstance(excluded_caps, list):
        return UNKNOWN
    for letter in excluded_caps:
        if not isinstance(letter, str):
            return UNKNOWN
        if letter in caps:
            return UNSATISFIED
    reachability_letters = sum(
        1 for letter in ("f", "R", "U") if letter in caps
    )
    if len(caps) <= reachability_letters:
        # The caps say only "floodfill" and/or "reachable": no capacity
        # information at all, so the peer will not use us.
        return UNSATISFIED
    signing = ctx.profile.get("signing_key_type")
    if signing is None:
        return UNKNOWN
    if str(signing).lower() in ("dsa", "dsa_sha1", "dsa-sha1"):
        return UNSATISFIED
    encryption = ctx.profile.get("encryption_type")
    if encryption is None:
        return UNKNOWN
    if str(encryption).upper() != "ECIES_X25519":
        return UNSATISFIED
    floor = ctx.peer.get("min_selection_version")
    if floor is None:
        return UNKNOWN
    at_least = version_at_least(version, floor)
    if at_least is None:
        return UNKNOWN
    return SATISFIED if at_least else UNSATISFIED


I2PD_PREDICATES = (
    Predicate(
        "i2pd-ri-signature", "i2pd", "admission",
        "The RouterInfo signature verifies.",
        "libi2pd/RouterInfo.cpp RouterInfo::VerifySignature",
        "i2pr-profile", _i2pd_signature,
    ),
    Predicate(
        "i2pd-ri-netid-present", "i2pd", "admission",
        "A netId property is present in the RouterInfo.",
        "libi2pd/RouterInfo.cpp:479-486,502 (isNetId, SetUnreachable)",
        "i2pr-profile", _i2pd_netid_present,
    ),
    Predicate(
        "i2pd-ri-netid-matches", "i2pd", "admission",
        "The netId property equals the peer's network id.",
        "libi2pd/RouterInfo.cpp:482-487 (netID != i2p::context.GetNetID())",
        "i2pr-profile", _i2pd_netid_matches,
    ),
    Predicate(
        "i2pd-ri-version-present", "i2pd", "admission",
        "A router.version property is present and non-zero.",
        "libi2pd/RouterInfo.cpp:464-476,502 (m_Version)",
        "i2pr-profile", _i2pd_version_present,
    ),
    Predicate(
        "i2pd-ri-transport-present", "i2pd", "admission",
        "At least one transport the peer supports is published.",
        "libi2pd/RouterInfo.cpp:502 (!m_SupportedTransports)",
        "i2pr-profile", _i2pd_transport_present,
    ),
    Predicate(
        "i2pd-ri-family-signature", "i2pd", "admission",
        "A published family signature verifies.",
        "libi2pd/RouterInfo.cpp:495-504 (VerifyFamily)",
        "i2pr-profile", _i2pd_family_signature,
    ),
    Predicate(
        "i2pd-not-unreachable", "i2pd", "dial",
        "The peer does not consider our RouterInfo unreachable.",
        "libi2pd/Transports.cpp:535 (r->IsUnreachable())",
        "i2pr-profile", _i2pd_not_unreachable,
    ),
    Predicate(
        "i2pd-dial-version-floor", "i2pd", "dial",
        "router.version clears the peer's minimum-allowed floor, or we "
        "declare high bandwidth.",
        "libi2pd/Transports.cpp:536, libi2pd/NetDb.hpp:58 "
        "(NETDB_MIN_ALLOWED_VERSION = 0.9.58)",
        "i2pr-profile", _i2pd_dial_version_floor,
    ),
    Predicate(
        "i2pd-reachable-from-peer", "i2pd", "dial",
        "We publish a non-introducer address on a transport the peer "
        "supports.",
        "libi2pd/RouterInfo.h:275 (IsReachableFrom), libi2pd/Transports.cpp:535",
        "i2pr-profile", _i2pd_reachable_from_peer,
    ),
    Predicate(
        "i2pd-floodfill-declared", "i2pd", "explore",
        "Our caps declare the floodfill capability.",
        "libi2pd/RouterInfo.cpp:451-455 (ExtractCaps, CAPS_FLAG_FLOODFILL)",
        "i2pr-profile", _i2pd_floodfill_declared,
        route="floodfill-exploration",
    ),
    Predicate(
        "i2pd-floodfill-eligible", "i2pd", "explore",
        "We are eligible to be filed as a floodfill, so an exploratory "
        "request can target us.",
        "libi2pd/RouterInfo.cpp IsEligibleFloodfill, libi2pd/NetDb.hpp:55 "
        "(NETDB_MIN_FLOODFILL_VERSION = 0.9.62), libi2pd/NetDb.cpp:333-344",
        "i2pr-profile", _i2pd_floodfill_eligible,
        route="floodfill-exploration",
    ),
    Predicate(
        "i2pd-peer-test-cap", "i2pd", "explore",
        "We publish the SSU2 testing address cap the peer's peer-test "
        "selector requires.",
        "libi2pd/RouterInfo.cpp:468-475, libi2pd/NetDb.hpp:57 "
        "(NETDB_MIN_PEER_TEST_VERSION = 0.9.62), libi2pd/NetDb.cpp:1179-1188",
        "i2pr-profile", _i2pd_peer_test_cap,
        route="ssu2-peer-test",
    ),
)

JAVA_PREDICATES = (
    Predicate(
        "java-ri-signature", "java-i2p", "admission",
        "The RouterInfo signature verifies.",
        "router/java/src/net/i2p/data/router/RouterInfo.java",
        "i2pr-profile", _java_signature,
    ),
    Predicate(
        "java-ri-netid", "java-i2p", "admission",
        "The netId property is present and equals the peer's network id.",
        "router/java/src/net/i2p/data/router/RouterInfo.java:85 "
        "(PROP_NETWORK_ID)",
        "i2pr-profile", _java_netid,
    ),
    Predicate(
        "java-ri-version-present", "java-i2p", "admission",
        "A router.version property is present.",
        "router/java/src/net/i2p/data/router/RouterInfo.java",
        "i2pr-profile", _java_version_present,
    ),
    Predicate(
        "java-ri-transport-present", "java-i2p", "admission",
        "At least one transport the peer supports is published.",
        "router/java/src/net/i2p/router/transport/",
        "i2pr-profile", _java_transport_present,
    ),
    Predicate(
        "java-ban-surface", "java-i2p", "admission",
        "Our (caps, version) pair does not land on one of the peer's ban "
        "triggers.",
        "router/java/src/net/i2p/router/transport/ntcp/InboundEstablishState.java"
        ":797-822",
        "i2pr-profile", _java_ban_surface,
    ),
    Predicate(
        "java-profile-creation", "java-i2p", "explore",
        "The peer's profile-creation predicate accepts our caps, so a "
        "profile can exist for us at all.",
        "router/java/src/net/i2p/router/peermanager/ProfileManagerImpl.java"
        ":431-441 (shouldCreate)",
        "i2pr-profile", _java_profile_creation,
        route="java-selection",
    ),
    Predicate(
        "java-not-excluded", "java-i2p", "explore",
        "The peer's exclusion rules do not exclude us from selection.",
        "router/java/src/net/i2p/router/tunnel/pool/TunnelPeerSelector.java"
        ":347-403 (shouldExclude), :45-50 (exclude caps)",
        "i2pr-profile", _java_not_excluded,
        route="java-selection",
    ),
)

#: The named routes by which a peer's selection state can reach us. At least
#: one route must be available for the profile to be lane-ready; the others
#: are alternatives the peer may take.
ROUTES = {
    "i2pd": ("floodfill-exploration", "ssu2-peer-test"),
    "java-i2p": ("java-selection",),
}

CATALOGUE = {
    "i2pd": I2PD_PREDICATES,
    "java-i2p": JAVA_PREDICATES,
}


# ---------------------------------------------------------------------------
# Fixture loading.
# ---------------------------------------------------------------------------


def load_json(path: Path) -> dict:
    try:
        text = path.read_text(encoding="utf-8")
    except OSError:
        raise GateError("fixture-unavailable") from None
    try:
        value = json.loads(text)
    except json.JSONDecodeError:
        raise GateError("fixture-malformed") from None
    if not isinstance(value, dict):
        raise GateError("fixture-malformed")
    return value


def validate_profile(value: dict) -> dict:
    if value.get("schema") != "i2pr-plan495-lab-routerinfo-v1":
        raise GateError("profile-schema-invalid")
    family = value.get("family")
    if family not in FAMILIES:
        raise GateError("profile-family-unknown")
    withheld = value.get("withheld", [])
    if not isinstance(withheld, list) or not all(
        isinstance(item, str) for item in withheld
    ):
        raise GateError("profile-schema-invalid")
    known = {predicate.identifier for predicate in CATALOGUE[family]}
    for item in withheld:
        if item not in known:
            raise GateError("withheld-predicate-unknown")
    return value


def validate_peer(value: dict) -> dict:
    if value.get("schema") != "i2pr-plan495-peer-config-v1":
        raise GateError("peer-config-schema-invalid")
    family = value.get("family")
    if family not in FAMILIES:
        raise GateError("peer-config-family-unknown")
    return value


# ---------------------------------------------------------------------------
# The preflight.
# ---------------------------------------------------------------------------


def evaluate(profile: dict, peer: dict) -> dict:
    """Evaluate every predicate of the profile's family."""
    family = profile["family"]
    ctx = Context(profile, peer)
    withheld = set(profile.get("withheld", []))
    results = []
    for predicate in CATALOGUE[family]:
        if predicate.identifier in withheld:
            state = WITHHELD
        else:
            try:
                state = predicate.evaluate(ctx)
            except Exception:  # a broken predicate is never a pass
                state = UNKNOWN
        results.append(
            {
                "id": predicate.identifier,
                "kind": predicate.kind,
                "route": predicate.route,
                "owner": predicate.owner,
                "state": state,
                "requirement": predicate.requirement,
                "source": predicate.source,
            }
        )
    return {"family": family, "predicates": results}


def route_states(report: dict) -> dict[str, str]:
    """The state of each named selection route.

    A route is ``satisfied`` only when every one of its predicates is
    satisfied. A route holding a deliberately withheld predicate is
    ``withheld``: the peer was never given the reason to pick us, which is
    a control, not a defect.
    """
    by_route: dict[str, list[dict]] = {}
    for row in report["predicates"]:
        if row["kind"] != "explore" or not row.get("route"):
            continue
        by_route.setdefault(row["route"], []).append(row)
    states: dict[str, str] = {}
    for route, rows in by_route.items():
        row_states = {row["state"] for row in rows}
        if UNKNOWN in row_states:
            states[route] = UNKNOWN
        elif WITHHELD in row_states:
            states[route] = WITHHELD
        elif UNSATISFIED in row_states:
            states[route] = UNSATISFIED
        else:
            states[route] = SATISFIED
    return states


def classify(report: dict) -> dict:
    """Turn a predicate report into a precondition verdict.

    The verdict speaks the gate's own vocabulary, so a lane never has to
    invent a wording for "the peer would not have picked us".
    """
    predicates = report["predicates"]
    unknown = [row for row in predicates if row["state"] == UNKNOWN]
    withheld = [row for row in predicates if row["state"] == WITHHELD]
    admission_bad = [
        row for row in predicates
        if row["kind"] == "admission" and row["state"] == UNSATISFIED
    ]
    dial_bad = [
        row for row in predicates
        if row["kind"] == "dial" and row["state"] == UNSATISFIED
    ]
    routes = route_states(report)

    if unknown:
        return {
            "verdict": "not-lane-ready",
            "outcome_class": "environment-unavailable",
            "cause": "unattributed",
            "reason": "predicate-unknown",
            "reason_predicates": [row["id"] for row in unknown],
            "route_states": routes,
        }
    if admission_bad:
        return {
            "verdict": "not-lane-ready",
            "outcome_class": "peer-admission-refused",
            "cause": "i2pr-lab-profile",
            "reason": "admission-predicate-unsatisfied",
            "reason_predicates": [row["id"] for row in admission_bad],
            "route_states": routes,
        }
    if dial_bad:
        return {
            "verdict": "not-lane-ready",
            "outcome_class": "peer-selection-refused",
            "cause": "i2pr-lab-profile",
            "reason": "dial-predicate-unsatisfied",
            "reason_predicates": [row["id"] for row in dial_bad],
            "route_states": routes,
        }
    if routes and not any(state == SATISFIED for state in routes.values()):
        # No route by which the peer's selection state could reach us.
        cause = (
            "intentional-withholding"
            if withheld
            else "i2pr-lab-profile"
        )
        failing = [
            row["id"] for row in predicates
            if row["kind"] == "explore"
            and row["state"] in (UNSATISFIED, WITHHELD)
        ]
        return {
            "verdict": "not-lane-ready",
            "outcome_class": "peer-selection-refused",
            "cause": cause,
            "reason": "no-selection-route-available",
            "reason_predicates": failing,
            "route_states": routes,
        }
    return {
        "verdict": "lane-ready",
        "outcome_class": None,
        "cause": None,
        "reason": "all-required-predicates-satisfied",
        "reason_predicates": [],
        "route_states": routes,
    }


def digest(value: object) -> str:
    payload = json.dumps(value, sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(payload.encode("utf-8")).hexdigest()[:16]


# ---------------------------------------------------------------------------
# The evidence validator: the tier ladder and the outcome taxonomy.
# ---------------------------------------------------------------------------


def validate_evidence(record: dict) -> list[str]:
    """Return the violations in a lane evidence record; empty means valid."""
    violations: list[str] = []
    if record.get("schema") != "i2pr-plan495-lane-evidence-v1":
        return ["evidence-schema-invalid"]

    outcome = record.get("outcome")
    if outcome not in OUTCOME_CLASSES:
        violations.append("evidence-outcome-class-invalid")

    cause = record.get("cause")
    if cause is not None and cause not in CAUSES:
        violations.append("evidence-cause-invalid")

    target = record.get("target_tier")
    if not isinstance(target, int) or isinstance(target, bool):
        violations.append("evidence-target-tier-invalid")
        target = None
    elif target not in TIERS:
        violations.append("evidence-target-tier-invalid")
        target = None

    tiers = record.get("tiers")
    if not isinstance(tiers, list):
        return violations + ["evidence-tiers-invalid"]

    seen: dict[int, dict] = {}
    for row in tiers:
        if not isinstance(row, dict):
            violations.append("evidence-tier-row-invalid")
            continue
        number = row.get("tier")
        state = row.get("state")
        reason = row.get("reason")
        if not isinstance(number, int) or isinstance(number, bool) or number not in TIERS:
            violations.append("evidence-tier-number-invalid")
            continue
        if number in seen:
            violations.append("evidence-tier-duplicate")
            continue
        if state not in TIER_STATES:
            violations.append("evidence-tier-state-invalid")
        if not isinstance(reason, str) or not reason.strip():
            violations.append("evidence-tier-reason-missing")
        seen[number] = row

    # A tier that was reached implies every lower tier was recorded: a gap
    # would let a lane claim T4 without ever stating what happened at T2.
    for number in sorted(seen):
        if seen[number]["state"] == "not-attempted":
            continue
        for lower in range(number):
            if lower not in seen:
                violations.append(f"evidence-tier-gap:{number}")

    # The outcome and the ladder must agree.
    if outcome == "pass":
        if target is None:
            violations.append("evidence-pass-without-target-tier")
        elif seen.get(target, {}).get("state") != "pass":
            violations.append("evidence-pass-without-earning-tier")
    if outcome == "i2pr-defect":
        if cause != "i2pr-implementation":
            violations.append("evidence-i2pr-defect-cause-invalid")
        failing = [
            row["tier"] for row in seen.values() if row.get("state") == "fail"
        ]
        if not failing:
            violations.append("evidence-i2pr-defect-without-failing-tier")
    if outcome in ("peer-admission-refused", "peer-selection-refused"):
        if cause not in PEER_OUTCOME_CAUSES:
            violations.append("evidence-peer-outcome-cause-invalid")
    if outcome == "peer-state-poisoned":
        if cause != "peer-state":
            violations.append("evidence-peer-state-cause-invalid")

    # Attempts: exactly one outcome class per attempt, and a peer-state
    # outcome must not consume the attempt budget.
    attempts = record.get("attempts", [])
    if not isinstance(attempts, list):
        return violations + ["evidence-attempts-invalid"]
    consumed = 0
    peer_state_attempts = 0
    for index, attempt in enumerate(attempts):
        if not isinstance(attempt, dict):
            violations.append("evidence-attempt-row-invalid")
            continue
        attempt_outcome = attempt.get("outcome")
        if attempt_outcome not in OUTCOME_CLASSES:
            violations.append("evidence-attempt-outcome-invalid")
            continue
        attempt_cause = attempt.get("cause")
        if attempt_cause is not None and attempt_cause not in CAUSES:
            violations.append("evidence-attempt-cause-invalid")
        if attempt_outcome == "peer-state-poisoned":
            peer_state_attempts += 1
            if attempt_cause != "peer-state":
                violations.append("evidence-attempt-peer-state-cause-invalid")
            if attempt.get("consumes_budget") is not False:
                violations.append("evidence-peer-state-consumed-budget")
            if not attempt.get("cooldown_seconds"):
                violations.append("evidence-peer-state-without-cooldown")
        else:
            if attempt.get("consumes_budget") is not True:
                violations.append("evidence-attempt-budget-flag-invalid")
            consumed += 1
    if outcome == "peer-state-poisoned" and not peer_state_attempts:
        violations.append("evidence-peer-state-without-cooldown")
    if record.get("attempts_consumed") is not None and record["attempts_consumed"] != consumed:
        violations.append("evidence-attempts-consumed-mismatch")

    return violations


# ---------------------------------------------------------------------------
# The peer-state hygiene precondition.
# ---------------------------------------------------------------------------

HYGIENE_CONDITIONS = (
    ("peer_data_dir_fresh", "stale peer data directory"),
    ("peer_process_fresh", "stale peer process"),
    ("no_stale_netdb_files", "stale netDb/r*/*.dat peer profiles"),
    ("static_key_matches_bound_port", "static-key mismatch on the live port"),
    ("advertised_endpoint_matches_bound_socket", "advertised endpoint differs from the bound socket"),
)


def validate_hygiene(record: dict) -> list[str]:
    violations: list[str] = []
    if record.get("schema") != "i2pr-plan495-peer-state-v1":
        return ["hygiene-schema-invalid"]
    for key, label in HYGIENE_CONDITIONS:
        value = record.get(key)
        if value is None:
            violations.append(f"hygiene-condition-missing:{key}")
            continue
        if value is not True:
            violations.append(f"hygiene-condition-failed:{key}:{label}")
    stale = record.get("stale_netdb_file_count")
    if stale is None:
        violations.append("hygiene-condition-missing:stale_netdb_file_count")
    elif isinstance(stale, bool) or not isinstance(stale, int) or stale < 0:
        violations.append("hygiene-stale-count-invalid")
    elif stale > 0 and record.get("no_stale_netdb_files") is not False:
        # The boolean already named this condition; report the count only
        # when the record contradicts itself.
        violations.append(
            "hygiene-condition-failed:no_stale_netdb_files:"
            "stale netDb/r*/*.dat peer profiles"
        )
    return violations


# ---------------------------------------------------------------------------
# The source-purity audit.
#
# The checker must not be able to reach the network: a gate that could talk
# to the peer could not be falsified by an offline fixture. The audit walks
# this file's own AST, so the deny-list's own literals -- which are strings,
# not imports -- cannot trip it.
# ---------------------------------------------------------------------------

FORBIDDEN_MODULES = (
    "socket",
    "socketserver",
    "ssl",
    "subprocess",
    "multiprocessing",
    "asyncio",
    "ctypes",
    "http",
    "urllib",
    "requests",
    "ftplib",
    "smtplib",
    "telnetlib",
    "xmlrpc",
    "webbrowser",
    "selectors",
    "grpcurl",
)

FORBIDDEN_CALLS = (
    "os.system",
    "os.popen",
    "os.fork",
    "os.forkpty",
    "os.execv",
    "os.execve",
    "os.execvp",
    "os.spawnv",
    "os.spawnl",
    "os.posix_spawn",
    "eval",
    "exec",
    "compile",
    "__import__",
    "importlib.import_module",
)


def _dotted(node: ast.AST) -> str | None:
    if isinstance(node, ast.Name):
        return node.id
    if isinstance(node, ast.Attribute):
        base = _dotted(node.value)
        if base is None:
            return None
        return f"{base}.{node.attr}"
    return None


def audit_purity(source: str) -> list[str]:
    """Return the purity violations in a Python source; empty means pure."""
    try:
        tree = ast.parse(source)
    except SyntaxError:
        return ["source-unparseable"]
    violations: list[str] = []
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            for alias in node.names:
                root = alias.name.split(".")[0]
                if root in FORBIDDEN_MODULES:
                    violations.append(f"forbidden-import:{root}")
        elif isinstance(node, ast.ImportFrom):
            if node.level:
                continue
            root = (node.module or "").split(".")[0]
            if root in FORBIDDEN_MODULES:
                violations.append(f"forbidden-import:{root}")
        elif isinstance(node, ast.Call):
            name = _dotted(node.func)
            if name is None:
                continue
            if name in FORBIDDEN_CALLS:
                violations.append(f"forbidden-call:{name}")
    return sorted(set(violations))


def self_source() -> str:
    path = Path(__file__).resolve()
    try:
        return path.read_text(encoding="utf-8")
    except OSError:
        raise GateError("checker-source-unavailable") from None


# ---------------------------------------------------------------------------
# The checked lab profile against the documented one.
# ---------------------------------------------------------------------------


def documented_profile_rows(spec_text: str) -> list[tuple[str, ...]]:
    """The lab-profile table rows of the canonical gate model.

    Emphasis markers are stripped so the documented table can be written in
    ordinary markdown; the comparison is about the values, not the styling.
    """
    rows: list[tuple[str, ...]] = []
    header = None
    for line in spec_text.splitlines():
        if not line.startswith("|"):
            if header is not None:
                break
            continue
        cells = [
            cell.strip().strip("`").strip("*")
            for cell in line.strip().strip("|").split("|")
        ]
        if header is None:
            if cells and cells[0] == "Field":
                header = cells
            continue
        if set("".join(cells)) <= set("-: "):
            continue
        if len(cells) != len(header):
            continue
        rows.append(tuple(cells))
    return rows


def profile_drift(spec_text: str) -> list[str]:
    """Compare the documented lab profile with the checked one."""
    documented = documented_profile_rows(spec_text)
    if not documented:
        return ["lab-profile-table-missing"]
    violations: list[str] = []
    if len(documented) != len(LAB_PROFILE_ROWS):
        violations.append("lab-profile-row-count-mismatch")
    for index, expected in enumerate(LAB_PROFILE_ROWS):
        if index >= len(documented):
            violations.append(f"lab-profile-row-missing:{expected[0]}")
            continue
        actual = documented[index]
        for field, want, got in zip(expected, actual, expected):
            if want != got:
                violations.append(
                    f"lab-profile-drift:{expected[0]}:{field}"
                )
    return violations


# ---------------------------------------------------------------------------
# The self-test.
# ---------------------------------------------------------------------------


def _profile(**overrides) -> dict:
    base = {
        "schema": "i2pr-plan495-lab-routerinfo-v1",
        "family": "i2pd",
        "label": "unit-fixture",
        "net_id": 2,
        "router_version": "0.9.62",
        "caps": "fOR",
        "signing_key_type": "ed25519",
        "encryption_type": "ECIES_X25519",
        "signature_valid": True,
        "family_signature_valid": None,
        "published_addresses": [
            {
                "transport": "SSU2",
                "version": 2,
                "host": "192.168.44.10",
                "port": 19001,
                "address_caps": "46",
            }
        ],
        "withheld": [],
    }
    base.update(overrides)
    return base


def _peer(family: str = "i2pd", **overrides) -> dict:
    base = {
        "schema": "i2pr-plan495-peer-config-v1",
        "family": family,
        "network_id": 2,
        "min_allowed_version": "0.9.58",
        "min_floodfill_version": "0.9.62",
        "min_peer_test_version": "0.9.62",
        "min_selection_version": "0.9.62",
        "floodfill_enabled": False,
        "max_share_bandwidth_kbps": 64,
        "supported_transports": ["SSU2", "NTCP2"],
        "exclude_caps": ["K", "E", "G"],
        "ban_triggers": [
            {"caps": "LU", "version": "0.9.56"},
            {"caps": "L", "version": "0.9.57"},
            {"caps": "GXf", "version": "0.9.67"},
        ],
    }
    base.update(overrides)
    return base


def run_self_test() -> int:
    failures: list[str] = []

    def check(condition: bool, label: str) -> None:
        if not condition:
            failures.append(label)

    # -- the baseline profile is lane-ready for both families ---------------
    for family in FAMILIES:
        profile = _profile(family=family)
        peer = _peer(family)
        report = evaluate(profile, peer)
        verdict = classify(report)
        check(
            verdict["verdict"] == "lane-ready",
            f"baseline-{family}-not-lane-ready:{verdict['reason']}",
        )

    # -- one negative case per predicate -----------------------------------
    negatives = [
        ("i2pd-ri-signature", _profile(signature_valid=False), _peer()),
        ("i2pd-ri-netid-present", _profile(net_id=None), _peer()),
        ("i2pd-ri-netid-matches", _profile(net_id=7), _peer()),
        ("i2pd-ri-version-present", _profile(router_version=None), _peer()),
        (
            "i2pd-ri-transport-present",
            _profile(published_addresses=[]),
            _peer(),
        ),
        (
            "i2pd-ri-family-signature",
            _profile(family_signature_valid=False),
            _peer(),
        ),
        (
            "i2pd-not-unreachable",
            _profile(net_id=7),
            _peer(),
        ),
        (
            "i2pd-dial-version-floor",
            _profile(router_version="0.9.57", caps="f"),
            _peer(),
        ),
        (
            "i2pd-reachable-from-peer",
            _profile(
                published_addresses=[
                    {
                        "transport": "SSU2",
                        "version": 2,
                        "host": "192.168.44.10",
                        "port": 19001,
                        "address_caps": "46C",
                    }
                ]
            ),
            _peer(),
        ),
        ("i2pd-floodfill-declared", _profile(caps="OR"), _peer()),
        (
            "i2pd-floodfill-eligible",
            _profile(router_version="0.9.61", caps="fOR"),
            _peer(),
        ),
        (
            "i2pd-peer-test-cap",
            _profile(caps="OR"),
            _peer(),
        ),
        (
            "java-ban-surface",
            _profile(
                family="java-i2p", caps="LU", router_version="0.9.56"
            ),
            _peer("java-i2p"),
        ),
        (
            "java-profile-creation",
            _profile(family="java-i2p", caps="fO"),
            _peer("java-i2p"),
        ),
        (
            "java-not-excluded",
            _profile(family="java-i2p", caps="fR"),
            _peer("java-i2p"),
        ),
    ]
    for identifier, profile, peer in negatives:
        report = evaluate(profile, peer)
        states = {row["id"]: row["state"] for row in report["predicates"]}
        check(
            states.get(identifier) == UNSATISFIED,
            f"negative-case-did-not-fire:{identifier}:{states.get(identifier)}",
        )
        verdict = classify(report)
        check(
            verdict["verdict"] == "not-lane-ready",
            f"negative-case-still-lane-ready:{identifier}",
        )
        check(
            verdict["outcome_class"] != "pass",
            f"negative-case-reported-pass:{identifier}",
        )

    # -- an unknown predicate refuses a lane-ready verdict ------------------
    report = evaluate(_profile(signature_valid=None), _peer())
    verdict = classify(report)
    check(
        verdict["verdict"] == "not-lane-ready"
        and verdict["reason"] == "predicate-unknown",
        "unknown-predicate-did-not-refuse",
    )

    # -- the deliberate-withholding control (WP6) --------------------------
    report = evaluate(
        _profile(withheld=["i2pd-floodfill-declared"]), _peer()
    )
    verdict = classify(report)
    check(
        verdict["outcome_class"] == "peer-selection-refused",
        "withholding-not-classified-as-peer-selection-refused",
    )
    check(
        verdict["outcome_class"] != "i2pr-defect",
        "withholding-classified-as-i2pr-defect",
    )
    check(
        verdict["verdict"] != "lane-ready",
        "withholding-reported-lane-ready",
    )
    check(
        verdict["cause"] == "intentional-withholding",
        "withholding-cause-not-intentional",
    )
    check(
        "i2pd-floodfill-declared" in verdict["reason_predicates"],
        "withholding-did-not-name-the-peers-own-reason",
    )

    # -- the evidence validator --------------------------------------------
    good_evidence = {
        "schema": "i2pr-plan495-lane-evidence-v1",
        "family": "i2pd",
        "target_tier": 4,
        "outcome": "pass",
        "cause": None,
        "tiers": [
            {"tier": 0, "state": "pass", "reason": "ri-persisted"},
            {"tier": 1, "state": "pass", "reason": "selected"},
            {"tier": 2, "state": "pass", "reason": "session-established"},
            {"tier": 3, "state": "pass", "reason": "one-way-i2np"},
            {"tier": 4, "state": "pass", "reason": "two-way-reply"},
        ],
        "attempts": [
            {
                "outcome": "peer-state-poisoned",
                "cause": "peer-state",
                "consumes_budget": False,
                "cooldown_seconds": 2100,
            },
            {"outcome": "pass", "cause": None, "consumes_budget": True},
        ],
        "attempts_consumed": 1,
    }
    check(
        validate_evidence(good_evidence) == [],
        f"valid-evidence-rejected:{validate_evidence(good_evidence)}",
    )

    def evidence_violation(mutate, label: str) -> None:
        record = json.loads(json.dumps(good_evidence))
        mutate(record)
        violations = validate_evidence(record)
        check(bool(violations), f"evidence-validator-missed:{label}")

    evidence_violation(
        lambda r: r.__setitem__("outcome", "peer-selection-refused"),
        "peer-outcome-without-cause",
    )
    evidence_violation(
        lambda r: r["tiers"].pop(2),
        "tier-gap",
    )
    evidence_violation(
        lambda r: r["tiers"][4].__setitem__("state", "fail"),
        "pass-without-earning-tier",
    )
    evidence_violation(
        lambda r: (
            r.__setitem__("outcome", "i2pr-defect"),
            r.__setitem__("cause", "i2pr-lab-profile"),
            r["tiers"][4].__setitem__("state", "fail"),
        ),
        "i2pr-defect-must-not-name-the-lab-profile",
    )
    evidence_violation(
        lambda r: (
            r.__setitem__("outcome", "i2pr-defect"),
            r.__setitem__("cause", "i2pr-implementation"),
        ),
        "i2pr-defect-without-a-failing-tier",
    )
    evidence_violation(
        lambda r: (
            r.__setitem__("outcome", "peer-admission-refused"),
            r.__setitem__("cause", "i2pr-implementation"),
        ),
        "peer-outcome-must-not-name-the-implementation",
    )
    evidence_violation(
        lambda r: r["attempts"][0].__setitem__("consumes_budget", True),
        "peer-state-consumed-budget",
    )
    evidence_violation(
        lambda r: r["attempts"][0].pop("cooldown_seconds"),
        "peer-state-without-cooldown",
    )
    evidence_violation(
        lambda r: r["tiers"][0].__setitem__("reason", "  "),
        "tier-without-reason",
    )
    evidence_violation(
        lambda r: r.__setitem__("outcome", "not-a-class"),
        "invalid-outcome-class",
    )

    # -- the peer-state hygiene precondition -------------------------------
    clean_hygiene = {
        "schema": "i2pr-plan495-peer-state-v1",
        "peer_data_dir_fresh": True,
        "peer_process_fresh": True,
        "no_stale_netdb_files": True,
        "stale_netdb_file_count": 0,
        "static_key_matches_bound_port": True,
        "advertised_endpoint_matches_bound_socket": True,
    }
    check(
        validate_hygiene(clean_hygiene) == [],
        f"clean-hygiene-rejected:{validate_hygiene(clean_hygiene)}",
    )
    poisoned = json.loads(json.dumps(clean_hygiene))
    poisoned["static_key_matches_bound_port"] = False
    violations = validate_hygiene(poisoned)
    check(
        any("static-key mismatch" in item for item in violations),
        f"hygiene-missed-static-key:{violations}",
    )
    stale = json.loads(json.dumps(clean_hygiene))
    stale["stale_netdb_file_count"] = 4
    violations = validate_hygiene(stale)
    check(
        any("stale netDb" in item for item in violations),
        f"hygiene-missed-stale-netdb:{violations}",
    )

    # -- the source-purity audit, with its inverted probes ------------------
    source = self_source()
    violations = audit_purity(source)
    check(
        violations == [],
        f"checker-is-not-pure:{violations}",
    )
    for probe, expected in (
        ("import socket\n", "forbidden-import:socket"),
        ("import subprocess\n", "forbidden-import:subprocess"),
        ("import urllib.request\n", "forbidden-import:urllib"),
        ("import os\nos.system('id')\n", "forbidden-call:os.system"),
        ("eval('1')\n", "forbidden-call:eval"),
        ("__import__('os')\n", "forbidden-call:__import__"),
    ):
        found = audit_purity(source + "\n" + probe)
        check(
            any(expected in item for item in found),
            f"purity-probe-missed:{expected}:{found}",
        )

    # -- the checked lab profile against the documented one ----------------
    spec_path = Path(__file__).resolve().parent.parent / LAB_PROFILE_SPEC
    if spec_path.is_file():
        spec_text = spec_path.read_text(encoding="utf-8")
        violations = profile_drift(spec_text)
        check(
            violations == [],
            f"lab-profile-drift:{violations}",
        )
        drifted = spec_text.replace("| `netId` | `2` |", "| `netId` | `9` |", 1)
        check(
            bool(profile_drift(drifted)),
            "profile-drift-guard-did-not-fire",
        )
    else:
        failures.append(f"lab-profile-spec-missing:{LAB_PROFILE_SPEC}")

    if failures:
        for failure in failures:
            print(f"plan495-peer-eligibility: FAIL: {failure}", file=sys.stderr)
        print(
            f"plan495-peer-eligibility: {len(failures)} self-test failure(s)",
            file=sys.stderr,
        )
        return 1
    print(
        "plan495-peer-eligibility: self-test ok "
        f"({len(negatives)} predicate negatives, purity probes, evidence and "
        "hygiene controls, profile-drift control)"
    )
    return 0


# ---------------------------------------------------------------------------
# Entry point.
# ---------------------------------------------------------------------------


def repository_root() -> Path:
    return Path(__file__).resolve().parent.parent


def emit(payload: dict) -> None:
    print(json.dumps(payload, sort_keys=True, indent=2))


def run_preflight(profile_path: Path, peer_path: Path | None) -> int:
    profile = validate_profile(load_json(profile_path))
    family = profile["family"]
    if peer_path is None:
        peer = _peer(family)
    else:
        peer = validate_peer(load_json(peer_path))
        if peer["family"] != family:
            raise GateError("peer-config-family-mismatch")
    report = evaluate(profile, peer)
    verdict = classify(report)
    emit(
        {
            "schema": "i2pr-plan495-preflight-result-v1",
            "profile_label": profile.get("label"),
            "family": family,
            "reference": {
                "i2pd": {"version": I2PD_VERSION, "pin": I2PD_PIN},
                "java-i2p": {"version": JAVA_VERSION, "pin": JAVA_PIN},
            }[family],
            "packet_attempt": "none",
            "live_qualification": "not-run",
            "verdict": verdict["verdict"],
            "outcome_class": verdict["outcome_class"],
            "cause": verdict["cause"],
            "reason": verdict["reason"],
            "reason_predicates": verdict["reason_predicates"],
            "predicate_digest": digest(report["predicates"]),
            "predicates": report["predicates"],
        }
    )
    return 0 if verdict["verdict"] == "lane-ready" else 1


def run_evidence(path: Path) -> int:
    record = load_json(path)
    violations = validate_evidence(record)
    emit(
        {
            "schema": "i2pr-plan495-evidence-result-v1",
            "valid": not violations,
            "violations": violations,
        }
    )
    return 0 if not violations else 1


def run_hygiene(path: Path) -> int:
    record = load_json(path)
    violations = validate_hygiene(record)
    emit(
        {
            "schema": "i2pr-plan495-hygiene-result-v1",
            "valid": not violations,
            "outcome_class": None if not violations else "peer-state-poisoned",
            "violations": violations,
        }
    )
    return 0 if not violations else 1


def run_withholding(profile_path: Path, peer_path: Path | None) -> int:
    profile = validate_profile(load_json(profile_path))
    family = profile["family"]
    peer = (
        validate_peer(load_json(peer_path))
        if peer_path is not None
        else _peer(family)
    )
    report = evaluate(profile, peer)
    verdict = classify(report)
    ok = (
        verdict["outcome_class"] == "peer-selection-refused"
        and verdict["outcome_class"] != "i2pr-defect"
        and verdict["verdict"] != "lane-ready"
    )
    emit(
        {
            "schema": "i2pr-plan495-withholding-result-v1",
            "control": "deliberate-withholding",
            "passed": ok,
            "outcome_class": verdict["outcome_class"],
            "cause": verdict["cause"],
            "reason": verdict["reason"],
            "reason_predicates": verdict["reason_predicates"],
        }
    )
    return 0 if ok else 1


def run_profile_drift() -> int:
    spec_path = repository_root() / LAB_PROFILE_SPEC
    try:
        spec_text = spec_path.read_text(encoding="utf-8")
    except OSError:
        raise GateError("lab-profile-spec-unavailable") from None
    violations = profile_drift(spec_text)
    emit(
        {
            "schema": "i2pr-plan495-profile-drift-result-v1",
            "spec": LAB_PROFILE_SPEC,
            "valid": not violations,
            "violations": violations,
        }
    )
    return 0 if not violations else 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--preflight", action="store_true")
    parser.add_argument("--withholding", action="store_true")
    parser.add_argument("--profile-drift", action="store_true")
    parser.add_argument("--profile", type=Path)
    parser.add_argument("--peer-config", type=Path)
    parser.add_argument("--evidence", type=Path)
    parser.add_argument("--hygiene", type=Path)
    args = parser.parse_args()

    try:
        if args.self_test:
            return run_self_test()
        if args.profile_drift:
            return run_profile_drift()
        if args.preflight:
            if not args.profile:
                parser.error("--preflight requires --profile")
            return run_preflight(args.profile, args.peer_config)
        if args.withholding:
            if not args.profile:
                parser.error("--withholding requires --profile")
            return run_withholding(args.profile, args.peer_config)
        if args.evidence:
            return run_evidence(args.evidence)
        if args.hygiene:
            return run_hygiene(args.hygiene)
        parser.error(
            "one of --self-test, --preflight, --withholding, --profile-drift, "
            "--evidence or --hygiene is required"
        )
    except GateError as error:
        print(
            json.dumps(
                {
                    "schema": "i2pr-plan495-gate-error-v1",
                    "result": "environment-unavailable",
                    "reason": error.code,
                    "packet_attempt": "none",
                    "live_qualification": "not-run",
                },
                sort_keys=True,
            )
        )
        return 1
    except (OSError, json.JSONDecodeError, KeyError, TypeError, ValueError) as error:
        print(
            "plan495-peer-eligibility: cannot run; refusing to report success "
            f"on an unchecked input: {error}",
            file=sys.stderr,
        )
        return 2
    return 2


if __name__ == "__main__":
    sys.exit(main())
