#!/usr/bin/env python3
"""Plan 381 evidence-integrity check for the live ELS2 external driver lane.

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
    "i2pr-rows",
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

# Rows the lane deliberately does not have. If one is ever written, remove it
# from this list and update the closure record's limitation section: the list
# firing is the mechanism that keeps a landed row from silently inheriting a
# "parked" narrative. Both name the successor plan that owns them.
DOCUMENTED_ABSENCES: list[str] = [
    # The i2pr-side `.b32` authority payload row: parked after three
    # compositions exposed real product boundaries (post-start ordinary
    # provisioning gap; product-spec invisibility to the shared manager;
    # startup lookup vs floodfill gossip timing).
    "authority-b32-payload-returned",
    # The i2pd-consumes-i2pr-published reverse row: parked after four live
    # attempts answered LeaseSet-not-found (publication/gossip path
    # unproven against the mesh).
    "reverse-payload-returned",
]

# Forbidden in the driver: the property is "missing environment fails, no
# silent pass, no secret in evidence, no out-of-lane capability".
FORBIDDEN_IN_DRIVER = [
    ("#[should_panic", "a should_panic row asserts failure, not the property"),
    ("std::process::Command", "command execution (the reverse SAM call was parked with the reverse row)"),
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

    # ------------------------------------------------------------------
    # 2. The evidence keys the matrix rows record.
    # ------------------------------------------------------------------
    for key in REQUIRED_DRIVER_KEYS:
        report.require("driver evidence", driver, f'"{key}"', DRIVER)

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
    # 7. A parked row that got written must update this list.
    # ------------------------------------------------------------------
    for absent in DOCUMENTED_ABSENCES:
        if absent in driver_code:
            report.failures.append(
                f"documented absence {absent} now exists in {DRIVER} — remove it from "
                "DOCUMENTED_ABSENCES and update the closure record's limitation list"
            )

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
    (DRIVER, "use std::sync::Arc;", "use std::sync::Arc;\n    unsafe { }", "the driver grows unsafe"),
    (RUNNER, '\nMAX_ATTEMPTS=1\n', '\nMAX_ATTEMPTS=2\n', "the attempt budget is raised"),
    (RUNNER, 'I2PD_PIN="635b013a612ff47278ef02acf8580a28e10e26c5"', 'I2PD_PIN="0000000000000000000000000000000000000000"', "the reference pin drifts"),
    (RUNNER, 'echo "Plan 381 ELS2 lane failed; sanitized evidence: ${EVIDENCE_DIR}" >&2\n  exit 1', 'echo "Plan 381 ELS2 lane passed (auth ${AUTH_MODE}); sanitized evidence: ${EVIDENCE_DIR}" >&2\n  exit 1', "the final gate is inverted to pass failing lanes"),
    ("crates/i2pr-daemon/src/service_tunnels.rs", "admitted_blinded = lease_set2.header().flags().is_blinded_on_publication()", "admitted_blinded = false", "the sweep stops preserving the admitted shape"),
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
    print("check-els2-live-lane-evidence: ok (driver + 4 unit rows + runner surface)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
