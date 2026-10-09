#!/usr/bin/env python3
"""Plan 400 evidence-integrity check for the live ELS2 corrective lane.

The shell wrapper ``scripts/check-els2-live-lane-evidence.sh`` documents the
boundary and execs this file.

This is a *static* check. It does not run the lane — the lane needs three
stock i2pd processes at a frozen pin and is environment-gated — and it
deliberately does not need to, because what it guards is the integrity of the
evidence rather than its content. A lane whose rows were deleted, ignored, or
weakened passes ``cargo test`` happily; that is the failure mode this file
exists to make loud.

How this differs from the outproxy checker it is modeled on: that lane is an
ordinary in-tree test, so an environment-gated row there is a silent skip and
is forbidden. This lane is an *external* lane, so its one driver test MUST be
``#[ignore]``-gated (Plan 162 rule) with missing environment failing, never
skipping — and that is what is checked here. The unit rows underneath it are
ordinary tests and must NOT be gated.

Run ``--mutation-table`` to reproduce the negative test.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

DRIVER = "crates/i2pr-daemon/tests/els2_i2pd_external.rs"
RUNNER = "tests/integration/els2/run-i2pd-els2.sh"
CONF_CONTRACT = "tests/integration/els2/test-tunnels-conf.sh"
EXTRACTOR = "tests/integration/els2/clients/parse_i2pd_els2_destination.py"

# The one driver test. There is exactly one on purpose: every WP4 mode runs
# the same binary with different lane inputs, so a second test fn would be a
# second lane, not a second row. The mode matrix lives in the evidence keys
# below and in the runner's auth-mode parametrization.
DRIVER_TEST = "els2_i2pr_consumes_reference_published_els2"
DRIVER_IGNORE_REASON = "requires the Plan 381 exact-pinned i2pd 2.61.0 ELS2 lane environment"

# Unit rows that must exist as ordinary (never gated) tests. Each pins a defect
# the live lane found; deleting one restores a green unit floor over code that
# fails against the reference.
REQUIRED_UNIT_ROWS: list[tuple[str, str]] = [
    (
        "crates/i2pr-daemon/tests/i2pcontrol_els2_black_box.rs",
        "plan334_els2_create_get_rawconfig_round_trip_over_jsonrpc",
    ),
    (
        "crates/i2pr-netdb/tests/els2_client_authorization.rs",
        "psk_derivation_matches_the_pinned_reference_algorithm",
    ),
    (
        "crates/i2pr-netdb/tests/els2_foundation.rs",
        "freshness_and_storage_key_checks_are_enforced",
    ),
    (
        "crates/i2pr-netdb/src/lease_set2.rs",
        "blinded_on_publication_rejected_by_default_and_allowed_by_policy",
    ),
    (
        "crates/i2pr-i2pcontrol/tests/contract.rs",
        "plan289_tunnel_request_envelope_rules",
    ),
]

# Evidence keys the driver must record. Presence in source is checked, not the
# values: the values come from the live run. `credential-present` is
# presence-only by construction — the value must never reach evidence, and the
# runner's key-material scrub fails the lane if it does.
REQUIRED_DRIVER_KEYS = [
    "auth-mode",
    "credential-present",
    "application-payload-returned",
    "negative-expected",
    "negative-observed",
    "authority-b32",
    "reverse-direction",
    "wrong-credential-scrub",
    "authority-create",
    "authority-generation-advanced",
    "authority-committed-generation",
    "authority-remote-target-projection",
    "authority-remote-counters",
    "authority-activation-pending",
    "authority-activation-failure-present",
    "authority-failed-connects",
    "authority-failure-stage",
    "authority-active-connections-peak",
    "b33-active-connections-peak",
    "consumer-credential-sealed",
    "pre-failure-stage",
    "post-failure-stage",
    "pre-activation-pending",
    "pre-activation-failure-present",
    "post-activation-pending",
    "post-activation-failure-present",
    "authority-b32-payload-returned",
    "reverse-server-create",
    "reverse-publication-handoff",
    "reverse-publication-failure-stage",
    "reverse-publication-counts",
    "reverse-destination-provisioning",
    "reverse-payload-returned",
]

# Runner rows: every `record` label the lane may emit, including the
# documented control skip. A renamed label breaks the evidence surface the
# closure record attributes; an added label is fine.
REQUIRED_RUNNER_ROWS = [
    "tunnels-conf-generated",
    "tunnels-conf-authority-section",
    "mesh-identity-generation",
    "mesh-peer-session",
    "reference-els2-published",
    "destination-derived",
    "blinded-address-cross-check",
    "client-tunnel-pool-ready",
    "control-reference-els2-roundtrip",
    "reference-b32-published",
    "destination-b32-derived",
    "control-reference-b32-roundtrip",
    "reference-families-published",
    "gossip-convergence-gate",
    "gossip-selection-audit",
    "reference-process-health",
    "reference-process-health-after",
    "i2pr-rows",
    "authority-b32-payload-returned",
    "reverse-payload-returned",
    "key-material-scrub",
    "key-material-scrub-dh",
]

# Frozen lane constants, checked literally. A drifted pin, version, or attempt
# budget is a different lane wearing this one's evidence. The attempt budget
# is anchored with its newlines so the header comment mentioning it does not
# satisfy the check by itself.
REQUIRED_RUNNER_LITERALS = [
    '\nMAX_ATTEMPTS=1\n',
    'I2PD_PIN="635b013a612ff47278ef02acf8580a28e10e26c5"',
    'I2PD_VERSION="2.61.0"',
    '"plan": 400',
    '"predecessor_plan": 399',
    '"reference_destination_signature_type": 7',
    'test -s "${EVIDENCE_DIR}/evidence.json"',
    'test -s "${EVIDENCE_DIR}/evidence.md"',
    "control-skip:",
]

# Shell-contract markers: the cheap gate's rows that pin the two reference
# defects this lane found by executing (the colon trap and the base64
# alphabet). The scripts assert them at runtime; these markers assert the
# assertions still exist.
REQUIRED_CONTRACT_MARKERS = [
    "validator refuses a standard-alphabet key value",
    "writer emits the I2P alphabet for a +/ key",
    "value carries the ':' separator i2pd requires",
]

# Extractor markers: the per-client-auth flag and the b32-only derivation.
REQUIRED_EXTRACTOR_MARKERS = [
    "the per-client-auth flag is in the body, not beside it",
    "--b32-only",
    "--per-client-auth",
]

# Forbidden in the driver: the property is "missing environment fails, no
# silent pass, no secret in evidence, no out-of-lane capability".
FORBIDDEN_IN_DRIVER = [
    ("#[should_panic", "a should_panic row asserts failure, not the property"),
    ("std::process::Command", "command execution"),
    ("Command::new", "command execution"),
    ("ToSocketAddrs", "the lane resolves a name"),
    ("lookup_host", "the lane resolves a name"),
    ("0.0.0.0", "the lane binds a non-loopback address"),
    ("continue-on-error", "a swallowed failure"),
    ("libloading", "dynamic library loading"),
    ("dlopen", "dynamic library loading"),
]

REQUIRED_DRIVER_ATTRIBUTES = ["#![forbid(unsafe_code)]"]

# The production properties the lane depends on. Each was a real defect found
# by executing the lane; each is invisible to the unit floor without the
# lane, so a silent regression would restore green units and a broken lane.
REQUIRED_IN_PRODUCTION: list[tuple[str, str, str]] = [
    (
        "crates/i2pr-daemon/src/service_tunnels.rs",
        "admitted_blinded = lease_set2.header().flags().is_blinded_on_publication()",
        "Plan 381 WP3 defect 8: the delivery sweep must preserve the admitted "
        "shape per record (opt in iff the stored record itself carries the "
        "flag). Re-validating the cached LS2 strict kills every SYN for an "
        "ELS2-inner record with BlindedPublicationDeferred. The needle is "
        "the derivation expression, not the variable name, so neutering the "
        "value while keeping the name still fails",
    ),
    (
        "crates/i2pr-daemon/src/exploratory_build.rs",
        "terminal hop",
        "Plan 381 WP3 defect 2: inbound replies arrive from the terminal hop, "
        "which forwards to the creator. Correlating on the first hop orphans "
        "every inbound reply",
    ),
    (
        "crates/i2pr-daemon/src/service_product.rs",
        "ids[9]",
        "Plan 381 WP3 defect 3: the inbound gateway route resolves the "
        "endpoint id, not the creator id. The material is keyed by the "
        "endpoint, so the creator id misses every installed build",
    ),
    (
        "crates/i2pr-daemon/src/service_product.rs",
        "destination_replenishment_order(inbound_deficit, outbound_deficit, submit_limit)",
        "Plan 388: interleave inbound and outbound submissions so an outbound "
        "concurrency window cannot starve the inbound leases needed for LS2",
    ),
    (
        "crates/i2pr-daemon/src/service_product.rs",
        "submitted_inbound_builds: progress.submitted_inbound",
        "Plan 389: the live provisioning snapshot must report the selected "
        "Destination's bounded submitted-build count",
    ),
    (
        "crates/i2pr-daemon/src/service_product.rs",
        ".retain(|destination_id, _| current_destination_ids.contains(destination_id))",
        "Plan 389: retired-generation stage counters must be pruned with their "
        "Destination owner to keep diagnostic memory bounded",
    ),
    (
        "crates/i2pr-daemon/src/service_product.rs",
        "registration_pool_admission_failures: progress.registration_pool_admission_failures",
        "Plan 389: preserve the bounded stage that distinguishes pool admission "
        "from build completion and role activation",
    ),
    (
        "crates/i2pr-daemon/src/exploratory_build.rs",
        "new_with_service_destination_capacity",
        "Plan 390: product composition must allocate the explicit service-role registry bound",
    ),
    (
        "crates/i2pr-daemon/src/exploratory_build.rs",
        "checked_mul(u16::from(",
        "Plan 390: retained inbound role capacity must be checked against the finite service group maximum",
    ),
    (
        "crates/i2pr-daemon/src/exploratory_build.rs",
        "i2pr_service_tunnels::MAX_EFFECTIVE_DIRECTION_TUNNELS",
        "Plan 390: aggregate role capacity must use the effective per-direction service ceiling",
    ),
    (
        "crates/i2pr-daemon/src/exploratory_build.rs",
        "i2pr_service_tunnels::MAX_SERVICE_TUNNELS",
        "Plan 390: aggregate role capacity must use the service tunnel count ceiling",
    ),
    (
        "crates/i2pr-daemon/src/service_product.rs",
        "ExploratoryBuildCoordinator::new_with_service_destination_capacity(",
        "Plan 390: both product composition paths must select the aggregate role bound",
    ),
    (
        "crates/i2pr-tunnel/src/data_plane_registry.rs",
        "pub inbound: u16",
        "Plan 390: the bounded role registry must represent all supported service inbound roles",
    ),
    (
        "crates/i2pr-daemon/src/service_product.rs",
        "post_activation_owner_failures: progress.post_activation_owner_failures",
        "Plan 391: retain bounded attribution for receive-owner rejection",
    ),
    (
        "crates/i2pr-daemon/src/service_product.rs",
        "DestinationPostActivationStage::InboundInstalled",
        "Plan 391: count successful inbound owner and bridge installation",
    ),
    (
        "crates/i2pr-daemon/src/sam/streams.rs",
        "pub(crate) enum InboundReceiveProjectionError",
        "Plan 392: keep bridge state absence and duplicate receive projection as distinct typed errors",
    ),
    (
        "crates/i2pr-daemon/src/service_product.rs",
        "DestinationPostActivationStage::BridgeDuplicateReceive",
        "Plan 392: count duplicate bridge projection separately without recording an identifier",
    ),
    (
        "crates/i2pr-daemon/src/sam/streams.rs",
        "pub(crate) fn new_staged(destination_id: DestinationId)",
        "Plan 392: support a bounded bridge role projection before its first LS2",
    ),
    (
        "crates/i2pr-daemon/src/sam/streams.rs",
        "pub(crate) fn router_ls2_for_publication(&self) -> Option<LeaseSet2>",
        "Plan 392: staged state must not fabricate a publishable LS2",
    ),
    (
        "crates/i2pr-daemon/src/service_product.rs",
        "RouterDestinationNetworkState::new_staged(",
        "Plan 392: committed router-backed generations must initialize staged bridge state",
    ),
    (
        "crates/i2pr-daemon/src/service_product.rs",
        ".cancel_ls2_publication(request_id)",
        "Plan 393: failed publication composition and delivery must release coordinator capacity",
    ),
    (
        "crates/i2pr-daemon/tests/destination_tunnel_unit.rs",
        "failed_publication_cancellation_releases_bounded_capacity",
        "Plan 393: cancellation must restore publication capacity for a replacement attempt",
    ),
    (
        "crates/i2pr-daemon/src/service_product.rs",
        "DestinationTunnelError::TooManyPublications => {\n                        PublicationBeginRejection::Capacity",
        "Plan 396: coordinator-capacity rejection must remain separately attributable",
    ),
    (
        "crates/i2pr-daemon/tests/els2_i2pd_external.rs",
        "begin_capacity={}",
        "Plan 396: the pinned evidence must retain coarse begin-rejection counts",
    ),
    (
        "crates/i2pr-daemon/tests/els2_i2pd_external.rs",
        '"reverse-service-connections-post-read"',
        "Plan 400: post-read evidence must retain bounded service connection counts",
    ),
    (
        "crates/i2pr-daemon/tests/els2_i2pd_external.rs",
        '"reverse-routing-counters-post-read"',
        "Plan 400: post-read evidence must retain bounded routing counters",
    ),
    (
        "crates/i2pr-daemon/tests/els2_i2pd_external.rs",
        '"reverse-inbound-orphan-receives-post-read"',
        "Plan 400: attribute post-read drops to a missing service receive owner",
    ),
    (
        "crates/i2pr-daemon/tests/els2_i2pd_external.rs",
        '"reverse-inbound-orphan-receives-delta"',
        "Plan 400: distinguish reverse receive-owner misses from earlier lane traffic",
    ),
    (
        "crates/i2pr-daemon/src/destination_tunnels.rs",
        "blinded_storage_key(&key))\n                    == Some(store_message.key)",
        "Plan 395: encrypted DatabaseStore key must match its blinded public key",
    ),
    (
        "crates/i2pr-daemon/src/service_tunnels.rs",
        "server_syns_observed_total",
        "Plan 397: expose a bounded cumulative count of inbound server SYNs observed",
    ),
    (
        "crates/i2pr-daemon/src/service_tunnels.rs",
        "server_target_dials_succeeded_total",
        "Plan 397: distinguish accepted server streams from successful loopback target dials",
    ),
    (
        "crates/i2pr-daemon/src/service_product.rs",
        "BuildDirection::Inbound => Some(tunnel.local_inbound_receive())",
        "Plan 397: retain the established tunnel's local endpoint receive id",
    ),
    (
        "crates/i2pr-daemon/src/service_product.rs",
        "let receive_id = binding.local_receive_tunnel.map(TunnelId::get)",
        "Plan 397: post-start owner and bridge projection must use endpoint receive id",
    ),
    (
        "crates/i2pr-daemon/src/service_product.rs",
        "pub fn delta_since(self, earlier: Self) -> Self",
        "Plan 400: inbound diagnostics must support transaction-scoped saturating deltas",
    ),
    (
        "crates/i2pr-daemon/tests/els2_i2pd_external.rs",
        '"reverse-router-inbound-traffic-delta"',
        "Plan 400: exact-pinned evidence must retain reverse inbound transition deltas",
    ),
    (
        "crates/i2pr-daemon/src/service_product.rs",
        "service_garlic_authenticated",
        "Plan 400: distinguish authenticated Garlic from inbound owner attribution",
    ),
    (
        "crates/i2pr-daemon/src/service_product.rs",
        "service_payloads_dequeued",
        "Plan 400: expose whether authenticated Garlic yielded destination payloads",
    ),
    (
        "crates/i2pr-daemon/src/service_product.rs",
        "service_sender_ls2_freshness_rejected",
        "Plan 400: retain bounded sender-LeaseSet2 freshness rejection attribution",
    ),
    (
        "crates/i2pr-daemon/src/service_product.rs",
        "service_sender_ls2_crypto_protocol_rejected",
        "Plan 400: retain bounded LeaseSet2 protocol-crypto rejection attribution",
    ),
    (
        "crates/i2pr-daemon/src/service_product.rs",
        "service_last_unsupported_signature_type",
        "Plan 400: retain only the bounded public signature algorithm code for diagnosis",
    ),
    (
        "tests/integration/els2/clients/sam_b33_connect.py",
        '"SIGNATURE_TYPE=7"',
        "Plan 400: the stock reference SAM destination must select the supported type-7 profile",
    ),
    (
        DRIVER,
        "DESTINATION=TRANSIENT SIGNATURE_TYPE=7",
        "Plan 400: the reverse ELS2 requester must use the supported type-7 profile",
    ),
    (
        DRIVER,
        'create_session.push_str(" i2cp.leaseSetPrivKey=")',
        "Plan 403: authorized reverse requester must provide the consumer secret",
    ),
    (
        DRIVER,
        '"reverse-reference-signature-type"',
        "Plan 400: the reverse requester profile must be recorded in sanitized evidence",
    ),
    (
        "crates/i2pr-daemon/src/sam/streams.rs",
        "RouterGarlicRejection::UnknownDestination",
        "Plan 400: distinguish unknown local destination from sender binding failures",
    ),
    (
        "tests/integration/els2/els2-tunnels-conf.sh",
        "tr -- '+/' '-~'",
        "Plan 381 WP4: the client key must be emitted in i2pd's base64 "
        "alphabet. Standard base64 decodes to wrong-but-well-formed bytes "
        "under i2pd and publishes under a key the lane never held",
    ),
    (
        "crates/i2pr-i2pcontrol/src/tunnel_request.rs",
        "consumer_shape",
        "Plan 381 WP4: the decoder must admit the ELS2 consumer shape "
        "(lookup secret plus target, no publisher mode). Without the "
        "carve-out no control consumer can configure a lookup secret and "
        "the wrong-secret row is unwritable",
    ),
    (
        "crates/i2pr-service-tunnels/src/config.rs",
        "is_ephemeral",
        "Plan 381 WP4: two port-0 listeners must validate. Treating "
        "ephemerals as colliding refuses every second default client",
    ),
    (
        "crates/i2pr-client/src/encrypted_leaseset.rs",
        "allow_blinded_on_publication",
        "Plan 381 WP3 defect 6: the ELS2 inner routinely carries "
        "BLINDED_ON_PUBLICATION. Strict validation rejects every live "
        "reference record; the opt-in is ELS2-inner-only",
    ),
    (
        "crates/i2pr-netdb/src/els2.rs",
        "ELS2_INNER_PUBLISHED_MAX_STALENESS_SECONDS",
        "Plan 381 WP3 defect 5: i2pd wraps the inner as-is and stamps the "
        "outer with now. Strict inner==outer equality rejects every live "
        "reference record; the same-day window preserves the intent",
    ),
    (
        "crates/i2pr-daemon/tests/els2_i2pd_external.rs",
        'starts_with("success")',
        "Plan 381 WP4: create assertions must read the result status, not "
        "just the envelope. A refused candidate returns result-status "
        "error with a null JSON-RPC error, and an envelope-only assert "
        "passes a service that was never built",
    ),
    (
        "crates/i2pr-daemon/src/service_tunnels.rs",
        "Err(DestinationFailure::LookupRequired { .. }) if runtime.delay_open",
        "Plan 385 defect 1: an uncached ordinary Base32 target with DelayOpen "
        "must accept a connection and request product-owned activation instead "
        "of parking its already-bound supervisor",
    ),
    (
        "crates/i2pr-daemon/src/service_tunnels.rs",
        "self.resolve_remote_client_target(runtime.destination_id, &hash)",
        "Plan 385 defect 2: after successful provisioning, ordinary client "
        "resolution must use only that requesting service's validated remote "
        "LeaseSet mirror",
    ),
]


class Report:
    def __init__(self) -> None:
        self.failures: list[str] = []

    def require(self, label: str, haystack: str, needle: str, where: str) -> None:
        if needle not in haystack:
            self.failures.append(f"{label}: {where} must contain {needle!r}")

    def forbid(self, label: str, haystack: str, needle: str, where: str) -> None:
        if needle in haystack:
            self.failures.append(f"{label}: {where} must NOT contain {needle!r}")

    def failed(self) -> bool:
        return bool(self.failures)


def read(rel: str) -> str:
    return (ROOT / rel).read_text(encoding="utf-8")


def strip_comments(source: str) -> str:
    """Return `source` with `//` and `/* */` comments blanked out.

    The driver's module header explains at length *why* it avoids certain
    constructs, and a raw substring scan over the file would trip on its own
    documentation. Presence checks still read the unstripped text.
    """
    out: list[str] = []
    index = 0
    length = len(source)
    while index < length:
        char = source[index]
        nxt = source[index + 1] if index + 1 < length else ""
        if char == "/" and nxt == "/":
            while index < length and source[index] != "\n":
                index += 1
        elif char == "/" and nxt == "*":
            index += 2
            while index < length and not (source[index] == "*" and source[index + 1] == "/"):
                index += 1
            index += 2
        else:
            out.append(char)
            index += 1
    return "".join(out)


def strip_shell_comments(source: str) -> str:
    """Return shell/Python `source` with `#` comments blanked out."""
    out: list[str] = []
    for line in source.splitlines(keepends=True):
        stripped = line.lstrip()
        if stripped.startswith("#"):
            out.append("\n")
        else:
            out.append(line)
    return "".join(out)


def fn_span(source: str, name: str) -> tuple[int, int]:
    """Byte span of one named `fn`, attributes excluded."""
    match = re.search(rf"\bfn\s+{re.escape(name)}\s*\(", source)
    if match is None:
        raise LookupError(name)
    start = match.start()
    brace = source.index("{", match.end())
    depth = 0
    index = brace
    while index < len(source):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return start, index + 1
        index += 1
    raise LookupError(f"{name}: unbalanced braces")


def attribute_window(source: str, fn_start: int) -> str:
    r"""The attribute lines immediately above a `fn`, or '' when there are none."""
    line_start = source.rfind("\n", 0, fn_start) + 1
    head = source[:line_start].rstrip()
    lines: list[str] = []
    for raw in reversed(head.splitlines()):
        stripped = raw.strip()
        if stripped.startswith("#["):
            lines.append(stripped)
            continue
        if stripped.startswith("//") or stripped == "":
            lines.append(stripped)
            continue
        break
    return "\n".join(reversed(lines))


def check(report: Report) -> None:
    driver = read(DRIVER)
    driver_code = strip_comments(driver)
    product = read("crates/i2pr-daemon/src/service_product.rs")
    service_coordinator = "ExploratoryBuildCoordinator::new_with_service_destination_capacity("
    if product.count(service_coordinator) != 2:
        report.failures.append(
            "Plan 390: both product composition paths must select the aggregate role bound"
        )
    coordinator_source = read("crates/i2pr-daemon/src/exploratory_build.rs")
    report.require(
        "role capacity derivation",
        coordinator_source,
        "checked_mul(u16::from(\n                i2pr_service_tunnels::MAX_EFFECTIVE_DIRECTION_TUNNELS,",
        "ExploratoryBuildCoordinator::new_with_service_destination_capacity",
    )
    publication_cancel = ".cancel_ls2_publication(request_id)"
    if product.count(publication_cancel) < 3:
        report.failures.append(
            "Plan 393: compose, request-construction, and delivery failures must cancel publication intent"
        )

    # ------------------------------------------------------------------
    # 1. The driver test exists and is ignore-gated for the exact
    #    environment (missing env fails; ordinary runs skip).
    # ------------------------------------------------------------------
    try:
        start, _ = fn_span(driver_code, DRIVER_TEST)
    except LookupError:
        report.failures.append(f"driver test missing or renamed: {DRIVER_TEST}")
        start = -1
    if start >= 0:
        window = attribute_window(driver_code, start)
        if "#[ignore" not in window:
            report.failures.append(
                f"driver test {DRIVER_TEST}: missing #[ignore] — an environment-gated "
                "lane that runs un-ignored breaks the ordinary floor"
            )
        elif DRIVER_IGNORE_REASON not in window:
            report.failures.append(
                f"driver test {DRIVER_TEST}: #[ignore] reason changed — "
                f"expected {DRIVER_IGNORE_REASON!r}"
            )
        for forbidden, why in (
            ("#[should_panic", "a should_panic row asserts failure, not the property"),
        ):
            if forbidden in window:
                report.failures.append(f"driver test {DRIVER_TEST}: {forbidden} — {why}")
        if "cfg(" in window:
            report.failures.append(
                f"driver test {DRIVER_TEST}: cfg-gated — a row that does not compile "
                "in an ordinary run is not evidence"
            )

    report.require(
        "post-start server publication scheduling",
        product,
        ".filter(|destination_id| !previously_known_servers.contains(destination_id))",
        "service_product.rs",
    )

    # ------------------------------------------------------------------
    # 2. The evidence keys the matrix rows record.
    # ------------------------------------------------------------------
    for key in REQUIRED_DRIVER_KEYS:
        report.require("driver evidence", driver, f'"{key}"', DRIVER)
    report.require(
        "reverse authorized consumer credential",
        driver,
        'create_session.push_str(" i2cp.leaseSetPrivKey=")',
        DRIVER,
    )
    for forbidden in (
        "i2cp.leaseSetType=5",
        "i2cp.leaseSetAuthType",
        "i2cp.leaseSetClient.",
    ):
        if forbidden in driver:
            report.failures.append(
                f"reverse requester carries publisher-only ELS2 option {forbidden!r}"
            )

    # ------------------------------------------------------------------
    # 3. The unit rows exist and are NOT gated.
    # ------------------------------------------------------------------
    for rel, row in REQUIRED_UNIT_ROWS:
        source = strip_comments(read(rel))
        try:
            row_start, _ = fn_span(source, row)
        except LookupError:
            report.failures.append(f"required unit row missing or renamed: {row} in {rel}")
            continue
        row_window = attribute_window(source, row_start)
        for forbidden, why in (
            ("#[ignore", "an ignored unit row silently skips in an ordinary run"),
            ("#[should_panic", "a should_panic row asserts failure, not the property"),
        ):
            if forbidden in row_window:
                report.failures.append(f"unit row {row} in {rel}: {forbidden} — {why}")
        if "cfg(" in row_window:
            report.failures.append(
                f"unit row {row} in {rel}: cfg-gated — a row that does not compile "
                "in an ordinary run is not evidence"
            )

    # ------------------------------------------------------------------
    # 4. The shell/Python cheap-gate markers.
    # ------------------------------------------------------------------
    contract = read(CONF_CONTRACT)
    for marker in REQUIRED_CONTRACT_MARKERS:
        report.require("tunnels.conf contract", contract, marker, CONF_CONTRACT)
    extractor = read(EXTRACTOR)
    for marker in REQUIRED_EXTRACTOR_MARKERS:
        report.require("b33 extractor", extractor, marker, EXTRACTOR)

    # ------------------------------------------------------------------
    # 5. The runner: frozen constants and the full row surface.
    # ------------------------------------------------------------------
    runner = read(RUNNER)
    for literal in REQUIRED_RUNNER_LITERALS:
        report.require("lane runner", runner, literal, RUNNER)
    for row in REQUIRED_RUNNER_ROWS:
        if not re.search(rf"(record_guarded?|record)\s+{re.escape(row)}\b|\"{re.escape(row)}\"", runner):
            # Labels reach `record` positionally or via variables; accept
            # either the literal call or a quoted mention.
            report.failures.append(f"runner row missing or renamed: {row} in {RUNNER}")

    # ------------------------------------------------------------------
    # 6. The driver holds no out-of-lane capability and stays loopback.
    # ------------------------------------------------------------------
    for needle, why in FORBIDDEN_IN_DRIVER:
        report.forbid("driver", driver_code, needle, f"{DRIVER} ({why})")
    for attribute in REQUIRED_DRIVER_ATTRIBUTES:
        report.require("driver", driver, attribute, DRIVER)
    report.forbid("driver", driver_code.replace("unsafe_code", ""), "unsafe", DRIVER)
    for needle in ("127.0.0.1",):
        report.require("driver", driver_code, needle, DRIVER)

    # ------------------------------------------------------------------
    # ------------------------------------------------------------------
    # 8. The production properties the lane depends on.
    # ------------------------------------------------------------------
    for rel, needle, why in REQUIRED_IN_PRODUCTION:
        report.require("Plan 381", strip_comments(read(rel)), needle, f"{rel} ({why})")

    # ------------------------------------------------------------------
    # 8b. The final gate reports failure as failure. The awk exits 0 when
    #     a non-passed row exists; the then-branch must say failed and exit
    #     non-zero, and the fall-through must say passed on stdout. An
    #     inversion here passes failing lanes and fails passing ones — it
    #     happened once during WP5 assembly and was caught by the next lane
    #     run, which is exactly one time too many for a hand check.
    # ------------------------------------------------------------------
    gate = re.search(
        r"if awk -F'\\t' '[^']*exit found \? 0 : 1[^']*' \"\$\{RESULTS_FILE\}\"[^;]*; then\n(.*?)fi\n(.*)",
        runner,
        re.DOTALL,
    )
    if gate is None:
        report.failures.append("runner: final gate shape unrecognized — inspect by hand")
    else:
        then_branch, after = gate.group(1), gate.group(2)
        if "lane failed" not in then_branch or "exit 1" not in then_branch:
            report.failures.append(
                "runner: gate then-branch must report failure and exit non-zero"
            )
        after_first = after.splitlines()[0] if after.splitlines() else ""
        if "lane passed" not in after_first or ">&2" in after_first:
            report.failures.append(
                "runner: gate fall-through must report the pass on stdout"
            )

    # ------------------------------------------------------------------
    # 9. The consumer-path guard must still pass: a green lane over a
    #    mutated request path proves nothing.
    # ------------------------------------------------------------------
    result = subprocess.run(
        ["bash", str(ROOT / "scripts" / "check-encrypted-service-consumer-caller.sh")],
        capture_output=True,
        text=True,
        cwd=ROOT,
        check=False,
    )
    if result.returncode != 0:
        report.failures.append(
            "guard: scripts/check-encrypted-service-consumer-caller.sh failed; a green lane "
            f"over a mutated request path proves nothing. Output: {result.stdout.strip()}"
        )


# ---------------------------------------------------------------------------
# Negative test.
# ---------------------------------------------------------------------------

MUTATIONS: list[tuple[str, str, str, str]] = [
    (DRIVER, f"async fn {DRIVER_TEST}(", f"async fn {DRIVER_TEST}_RENAMED(", "a required row is renamed"),
    (DRIVER, DRIVER_IGNORE_REASON, "some other reason", "the ignore-gate reason is changed"),
    (DRIVER, '"application-payload-returned"', '"application-payload-renamed"', "an evidence key is renamed"),
    (DRIVER, '"authority-b32-payload-returned"', '"authority-b32-row-renamed"', "the post-start authority row is removed"),
    (DRIVER, '"reverse-destination-provisioning"', '"reverse-destination-provisioning-renamed"', "the reverse provisioning evidence is removed"),
    (DRIVER, '"reverse-service-connections-post-read"', '"reverse-service-connections-post-read-renamed"', "post-read service connection evidence is removed"),
    (DRIVER, '"reverse-routing-counters-post-read"', '"reverse-routing-counters-post-read-renamed"', "post-read routing counter evidence is removed"),
    (DRIVER, '"reverse-inbound-orphan-receives-post-read"', '"reverse-inbound-orphan-receives-post-read-renamed"', "post-read inbound orphan evidence is removed"),
    (DRIVER, '"reverse-inbound-orphan-receives-delta"', '"reverse-inbound-orphan-receives-delta-renamed"', "reverse receive-owner delta evidence is removed"),
    ("crates/i2pr-daemon/src/service_tunnels.rs", "pub server_syns_observed_total: u64,", "pub server_syns_seen_total: u64,", "the bounded inbound server SYN observation is removed"),
    ("crates/i2pr-daemon/src/service_product.rs", "let receive_id = binding.local_receive_tunnel.map(TunnelId::get)", "let receive_id = self.manager.with_destination_runtime(destination_id, |runtime| runtime.tunnel_registration(binding.pool_slot).map(|registration| registration.tunnel_id().get())).flatten()", "post-start owner registration regresses to the pool creator tunnel id"),
    ("crates/i2pr-daemon/tests/els2_i2pd_external.rs", '"reverse-router-inbound-traffic-delta"', '"reverse-router-inbound-traffic-delta-renamed"', "transaction-scoped inbound route diagnostics are removed"),
    ("crates/i2pr-daemon/src/service_product.rs", "pub service_garlic_authenticated: u64,", "pub service_garlic_valid: u64,", "authenticated inbound Garlic attribution is removed"),
    ("crates/i2pr-daemon/src/service_product.rs", "pub service_sender_ls2_freshness_rejected: u64,", "pub service_sender_ls2_fresh_rejected: u64,", "fine-grained sender LeaseSet2 validation attribution is removed"),
    ("crates/i2pr-daemon/src/sam/streams.rs", "i2pr_crypto::CryptoError::Protocol(_),\n        )) => RouterGarlicRejection::LeaseSet2CryptoProtocol", "i2pr_crypto::CryptoError::Protocol(_),\n        )) => RouterGarlicRejection::LeaseSet2Crypto", "LeaseSet2 validation crypto failures lose protocol subtype attribution"),
    ("crates/i2pr-daemon/src/service_product.rs", "ExploratoryBuildCoordinator::new_with_service_destination_capacity(", "ExploratoryBuildCoordinator::new(", "product composition falls back to the exploratory-only role bound"),
    ("crates/i2pr-daemon/src/exploratory_build.rs", "i2pr_service_tunnels::MAX_EFFECTIVE_DIRECTION_TUNNELS,", "i2pr_service_tunnels::MAX_SERVICE_TUNNELS,", "aggregate inbound capacity stops using the per-direction ceiling"),
    ("crates/i2pr-daemon/src/service_product.rs", "RouterDestinationNetworkState::new_staged(", "RouterDestinationNetworkState::new_with_outbound_roles(", "post-start generation stops initializing staged bridge state"),
    ("crates/i2pr-daemon/src/service_product.rs", ".cancel_ls2_publication(request_id)", ".retry_ls2_publication(request_id)", "a publication failure path stops releasing its coordinator intent"),
    ("crates/i2pr-daemon/src/service_product.rs", "DestinationTunnelError::TooManyPublications => {\n                        PublicationBeginRejection::Capacity", "DestinationTunnelError::TooManyPublications => {\n                        PublicationBeginRejection::Other", "publication capacity rejection loses its distinct counter"),
    (DRIVER, "use std::sync::Arc;", "use std::sync::Arc;\n    unsafe { }", "the driver grows unsafe"),
    (RUNNER, '\nMAX_ATTEMPTS=1\n', '\nMAX_ATTEMPTS=2\n', "the attempt budget is raised"),
    (RUNNER, 'I2PD_PIN="635b013a612ff47278ef02acf8580a28e10e26c5"', 'I2PD_PIN="0000000000000000000000000000000000000000"', "the reference pin drifts"),
    (RUNNER, '"reference_destination_signature_type": 7', '"reference_destination_signature_type": 0', "evidence no longer records the explicit supported signature profile"),
    ("tests/integration/els2/clients/sam_b33_connect.py", '"SIGNATURE_TYPE=7"', '"SIGNATURE_TYPE=0"', "reference SAM requester silently returns to legacy type 0"),
    (DRIVER, "DESTINATION=TRANSIENT SIGNATURE_TYPE=7", "DESTINATION=TRANSIENT SIGNATURE_TYPE=0", "reverse requester silently returns to legacy type 0"),
    (DRIVER, 'create_session.push_str(" i2cp.leaseSetPrivKey=")', 'create_session.push_str(" i2cp.leaseSetPrivateKey=")', "authorized reverse requester stops supplying the consumer secret"),
    (DRIVER, '"consumer-credential-sealed"', '"consumer-credential-stored"', "DH/PSK consumer credential presence evidence is removed"),
    (DRIVER, '"post-failure-stage"', '"post-failure-phase"', "deferred failure stage evidence is removed"),
    (DRIVER, '"authority-active-connections-peak"', '"authority-active-connections"', "authority local-accept evidence is removed"),
    (RUNNER, 'record_guarded "reference-process-health"', 'record_guarded "reference-process-status"', "the pre-driver reference health gate is removed"),
    (DRIVER, '"post-activation-failure-present"', '"post-activation-failure"', "deferred provisioning failure presence evidence is removed"),
    (DRIVER, 'create_session.push_str(" i2cp.leaseSetPrivKey=");', 'create_session.push_str(" i2cp.leaseSetType=5");\n        create_session.push_str(" i2cp.leaseSetPrivKey=");', "reverse requester regresses to publisher-only authorization options"),
    (RUNNER, 'echo "Plan 400 ELS2 corrective lane failed; sanitized evidence: ${EVIDENCE_DIR}" >&2\n  exit 1', 'echo "Plan 400 ELS2 corrective lane passed (auth ${AUTH_MODE}); sanitized evidence: ${EVIDENCE_DIR}" >&2\n  exit 1', "the final gate is inverted to pass failing lanes"),
    (RUNNER, 'test -s "${EVIDENCE_DIR}/evidence.json"', 'record evidence-packaged passed "evidence files exist"', "the evidence package is not verified before final TSV copy"),
    ("crates/i2pr-daemon/src/service_tunnels.rs", "admitted_blinded = lease_set2.header().flags().is_blinded_on_publication()", "admitted_blinded = false", "the sweep stops preserving the admitted shape"),
    ("crates/i2pr-daemon/src/service_tunnels.rs", "Err(DestinationFailure::LookupRequired { .. }) if runtime.delay_open", "Err(DestinationFailure::LookupRequired { .. }) if false", "ordinary delay-open clients stop reaching product activation"),
    ("crates/i2pr-daemon/src/service_tunnels.rs", "self.resolve_remote_client_target(runtime.destination_id, &hash)", "self.resolve_remote_client_target(runtime.destination_id, &[0; 32])", "ordinary resolution stops using the validated cached target"),
    ("crates/i2pr-daemon/src/service_product.rs", ".filter(|destination_id| !previously_known_servers.contains(destination_id))", ".filter(|_| false)", "post-start control generations stop scheduling server publication"),
    ("crates/i2pr-daemon/src/service_product.rs", "destination_replenishment_order(inbound_deficit, outbound_deficit, submit_limit)", "destination_replenishment_order(outbound_deficit, inbound_deficit, submit_limit)", "inbound build scheduling is displaced behind outbound work"),
    ("crates/i2pr-daemon/src/service_product.rs", ".retain(|destination_id, _| current_destination_ids.contains(destination_id))", ".retain(|_, _| true)", "retired Destination progress is no longer bounded by the committed generation"),
    ("crates/i2pr-daemon/src/service_product.rs", "registration_pool_admission_failures: progress.registration_pool_admission_failures", "registration_pool_admission_failures: 0", "pool admission failure stage is no longer reported"),
    ("tests/integration/els2/els2-tunnels-conf.sh", "tr -- '+/' '-~'", "tr -- '+/' '+/'", "the alphabet translation is neutered"),
]

# Mutations that must NOT be detected: deliberate design choices, so a
# regression in the checker itself is visible.
CONTROLS: list[tuple[str, str, str, str]] = [
    (DRIVER, "//! - `I2PR_ELS2_SSU2_BIND` — a fixed loopback bind for R",
     "//! - `I2PR_ELS2_SSU2_BIND` — a fixed loopback bind for R (std::process::Command is never used here).",
     "a forbidden construct named only in a comment is not a violation"),
    (DRIVER, "    let _ = product.shutdown().await;", "    let _ = product.shutdown().await; // shutdown", "a trailing comment added is not a violation"),
]


def mutation_table() -> int:
    detected = 0
    missed = 0
    needed = {rel for rel, *_ in MUTATIONS} | {rel for rel, *_ in CONTROLS}
    original = {rel: (ROOT / rel).read_text(encoding="utf-8") for rel in needed}
    for rel, old, new, label in MUTATIONS:
        path = ROOT / rel
        source = original[rel]
        if old not in source:
            print(f"SKIP (anchor absent): {label}")
            missed += 1
            continue
        path.write_text(source.replace(old, new, 1), encoding="utf-8")
        try:
            report = Report()
            check(report)
            failed = report.failed()
        finally:
            path.write_text(source, encoding="utf-8")
        if failed:
            print(f"detected: {label}")
            detected += 1
        else:
            print(f"NOT DETECTED: {label}")
            missed += 1
    controls_ok = 0
    controls_bad = 0
    for rel, old, new, label in CONTROLS:
        path = ROOT / rel
        source = original.get(rel) or (ROOT / rel).read_text(encoding="utf-8")
        if old not in source:
            print(f"CONTROL SKIP (anchor absent): {label}")
            controls_bad += 1
            continue
        path.write_text(source.replace(old, new, 1), encoding="utf-8")
        try:
            report = Report()
            check(report)
            failed = report.failed()
        finally:
            path.write_text(source, encoding="utf-8")
        if failed:
            print(f"CONTROL FAILED (checker is too strict): {label}")
            controls_bad += 1
        else:
            print(f"control ok (correctly not flagged): {label}")
            controls_ok += 1
    print()
    print(
        f"{detected} mutation(s) detected, {missed} missed, "
        f"{controls_ok} control(s) ok, {controls_bad} control(s) broken"
    )
    return 1 if (missed or controls_bad) else 0


def main() -> int:
    if len(sys.argv) > 1 and sys.argv[1] == "--mutation-table":
        return mutation_table()
    if len(sys.argv) > 1 and sys.argv[1] == "--self-test":
        # The cheap gate for the checker itself: run the check and require
        # silence. A checker that fails on the tree it guards is broken, not
        # strict. Mutation coverage is the `--mutation-table` run.
        report = Report()
        check(report)
        if report.failed():
            for failure in report.failures:
                print(f"FAIL: {failure}")
            return 1
        print("check-els2-live-lane-evidence: self-test holds")
        return 0
    report = Report()
    check(report)
    if report.failed():
        for failure in report.failures:
            print(f"FAIL: {failure}")
        print(f"check-els2-live-lane-evidence: {len(report.failures)} failure(s)")
        return 1
    print("check-els2-live-lane-evidence: ok (driver + 5 unit rows + runner surface)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
