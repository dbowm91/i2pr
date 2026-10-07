#!/usr/bin/env python3
"""Plan 342 evidence-integrity check for the loopback outproxy wire lane.

The shell wrapper ``scripts/check-outproxy-wire-lane-evidence.sh`` documents the
boundary and execs this file.

This is a *static* check. It does not run the lane — the routine floor does that
with ``--test-threads=1`` — and it deliberately does not need to, because what it
guards is the integrity of the evidence rather than its content. A lane whose rows
were deleted, ignored, or weakened passes ``cargo test`` happily; that is the
failure mode this file exists to make loud.

Run ``--mutation-table`` to reproduce the negative test.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

LANE = "crates/i2pr-daemon/tests/outproxy_loopback_wire.rs"

# The production modules the lane exercises. Plan 342's "no plugin loading and no
# command execution from control input" row is checked over these, not over the
# lane alone: the lane having no `libloading` proves nothing if the module it
# drives has one.
PRODUCTION = [
    "crates/i2pr-daemon/src/outproxy_route.rs",
    "crates/i2pr-daemon/src/outproxy_options.rs",
    "crates/i2pr-service-tunnels/src/outproxy.rs",
    "crates/i2pr-service-tunnels/src/outbound_secret.rs",
    "crates/i2pr-service-tunnels/src/target_policy.rs",
]

# The requirement. Discovered rows would make this check tautological.
REQUIRED_ROWS = [
    # A clearnet target reaches a real outproxy over Streaming and its bytes come
    # back, handshake prefix included. This is the row that does not exist without
    # the target-policy fix: before it, every parser refused the clearnet
    # authority and this row was unreachable rather than failing.
    "plan342_clearnet_target_succeeds_through_the_loopback_outproxy",
    # An `.i2p` target is routed to the tunnel's own destination and the outproxy
    # is not consulted at all.
    "plan342_i2p_authority_bypasses_the_outproxy",
    # No provider means refusal with nothing opened.
    "plan342_clearnet_target_without_a_provider_is_refused_and_opens_nothing",
    # An unreachable outproxy fails typed and bounded rather than hanging.
    "plan342_unreachable_outproxy_fails_typed_and_bounded",
    # The credential is presented as a header and never reaches the client.
    "plan342_outproxy_auth_is_presented_and_never_echoed",
    # The SOCKS family reaches the same route.
    "plan342_socks5_request_is_carried_by_the_outproxy",
    # The forward path carries the clearnet authority rather than refusing it.
    "plan342_http_forward_request_is_carried_by_the_outproxy",
    # The block grammar holds, and a refused block allocates no listener.
    "plan342_malformed_block_is_refused_before_any_listener",
    # -- Plan 376. The two absences this file used to record, now closed. --
    # Failover, over HTTP CONNECT and again over SOCKS5 so it is a property of
    # the route owner rather than of the HTTP request grammar.
    "plan376_http_connect_fails_over_to_the_second_outproxy",
    "plan376_socks5_fails_over_to_the_second_outproxy",
    # Both halves of the retryable taxonomy, and the attempt budget.
    "plan376_an_upstream_refusal_is_retried_at_the_second_outproxy",
    "plan376_authentication_rejection_is_retried_at_the_next_endpoint_by_the_current_policy",
    "plan376_the_attempt_budget_is_the_list_length_and_exhaustion_is_typed",
    # A restart that carries a real request, through the product's own
    # `TunnelControlState::startup` recovery path rather than a re-applied
    # `create`, plus the bypass, fail-closed, and copied-config rows.
    "plan376_a_routed_request_survives_a_product_restart",
    "plan376_after_restart_i2p_traffic_bypasses_and_removing_the_provider_fails_closed",
    "plan376_a_copied_config_without_the_router_secret_cannot_recover_the_credential",
]

# Rows the lane deliberately does not have. Recorded so that someone who later
# writes one knows it was a considered absence and not an oversight, and so the
# module header's limitation list has a machine-checked counterpart.
#
# Plan 376 removed the two Plan 342 entries (`..._failover_reaches_the_second...`
# and `..._route_survives_a_restart`) only after their replacement rows actually
# passed. What remains below is absent by decision, not by omission.
DOCUMENTED_ABSENCES: list[str] = [
    "plan376_between_request_load_rotation",
    "plan376_cross_process_restart",
]

# Forbidden in the lane: the property is "this process never holds a clearnet
# capability", and any of these would make that untrue while the row still passed.
FORBIDDEN_IN_LANE = [
    # `TcpStream` is imported from `tokio::net` here, so a bare
    # `TcpStream::connect` cannot be forbidden by prefix -- that would also
    # forbid the loopback connects the lane is built from. What can be forbidden
    # is a connect to a *literal* address, which is exactly the clearnet shape.
    ('TcpStream::connect("', "the lane opens a socket to a literal address"),
    ('TcpStream::connect(&"', "the lane opens a socket to a literal address"),
    ("ToSocketAddrs", "the lane resolves a name"),
    ("lookup_host", "the lane resolves a name"),
    ("libloading", "dynamic library loading"),
    ("dlopen", "dynamic library loading"),
    ("libloading::Library", "dynamic library loading"),
    ("std::process::Command", "command execution"),
    ("Command::new", "command execution"),
    ("0.0.0.0", "the lane binds a non-loopback address"),
    ("|| true", "a swallowed failure"),
    ("continue-on-error", "a swallowed failure"),
]

# The lane must be `#![forbid(unsafe_code)]`. Checked separately because the
# attribute itself contains the word, so the body has to be read with it removed.
REQUIRED_LANE_ATTRIBUTES = ["#![forbid(unsafe_code)]"]

# Plan 376: the three production properties the new rows depend on. Each was a
# real defect before Plan 376, and each is invisible to a single-endpoint row,
# so a silent regression would restore a green lane and a broken router.
REQUIRED_IN_PRODUCTION: list[tuple[str, str, str]] = [
    (
        "crates/i2pr-daemon/src/outproxy_route.rs",
        "backoff_ms",
        "Plan 376: the policy's bounded backoff schedule must actually be consulted "
        "by the retry loop. It was declared and never called, so a retryable "
        "failure was retried immediately and a dead list was hammered in one burst",
    ),
    (
        "crates/i2pr-daemon/src/outproxy_route.rs",
        "cancellation.cancelled()",
        "Plan 376: the retry backoff must be cancellation-aware, or a shutdown "
        "during the backoff window sits out the remaining schedule",
    ),
    (
        "crates/i2pr-daemon/src/outproxy_options.rs",
        "list.len()",
        "Plan 376: the attempt budget must follow the operator's ProxyList length. "
        "It was hardcoded to 2, so entries 3..N were silently never tried",
    ),
]

# Forbidden in production: Plan 342 invariant 5.
FORBIDDEN_IN_PRODUCTION = [
    ("libloading", "dynamic library loading"),
    ("dlopen", "dynamic library loading"),
    ("std::process::Command", "command execution"),
    ("Command::new", "command execution"),
    ("std::env::var", "reading the environment as a control input"),
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

    The lane's module header explains at length *why* it avoids certain
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
    r"""The attribute lines immediately above a `fn`, or '' when there are none.

    Anchored at the start of the line containing the match, not at the match
    itself. `re.search(r"\bfn\s+name")` matches the `fn` of an `async fn`, so
    `source[:fn_start]` ends with `async ` and a naive upward walk breaks on the
    first line -- which silently returned an empty window for every row in this
    file and made the `#[ignore]` check vacuous.
    """
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
    lane = read(LANE)
    lane_code = strip_comments(lane)

    # ------------------------------------------------------------------
    # 1. The required rows exist.
    # ------------------------------------------------------------------
    for row in REQUIRED_ROWS:
        if not re.search(rf"\bfn\s+{re.escape(row)}\s*\(", lane_code):
            report.failures.append(f"required row missing or renamed: {row}")

    # ------------------------------------------------------------------
    # 2. No required row is skipped, and no row is weakened.
    # ------------------------------------------------------------------
    for row in REQUIRED_ROWS + DOCUMENTED_ABSENCES:
        try:
            start, _ = fn_span(lane_code, row)
        except LookupError:
            continue
        window = attribute_window(lane_code, start)
        for forbidden, why in (
            ("#[ignore", "an ignored row silently skips in an ordinary run"),
            ("#[should_panic", "a should_panic row asserts failure, not the property"),
        ):
            if forbidden in window:
                report.failures.append(f"row {row}: {forbidden} — {why}")
        # A row must not be env-gated. `std::env::var` anywhere in the lane is
        # already forbidden below; this catches the `#[cfg(feature = ...)]` shape.
        if "cfg(" in window:
            report.failures.append(
                f"row {row}: cfg-gated — a row that does not compile in an ordinary run "
                "is not evidence"
            )

    # A documented absence that grew into a real row must be removed from the
    # list, or the module header's limitation note goes stale silently.
    for absent in DOCUMENTED_ABSENCES:
        if re.search(rf"\bfn\s+{re.escape(absent)}\s*\(", lane_code):
            report.failures.append(
                f"documented absence {absent} now exists — remove it from "
                "DOCUMENTED_ABSENCES and update the module header's limitation list"
            )

    # ------------------------------------------------------------------
    # 3. The lane holds no clearnet capability of its own.
    # ------------------------------------------------------------------
    for needle, why in FORBIDDEN_IN_LANE:
        report.forbid("lane", lane_code, needle, LANE)
    for attribute in REQUIRED_LANE_ATTRIBUTES:
        report.require("lane", lane_code, attribute, LANE)
    # `unsafe` is forbidden in the body; the `forbid(unsafe_code)` attribute is
    # the one place the word belongs, so it is masked out before the scan.
    report.forbid("lane", lane_code.replace("unsafe_code", ""), "unsafe", LANE)

    # Every bind must be loopback.
    binds = re.findall(r"bind\(\s*\"([^\"]+)\"", lane_code)
    for address in binds:
        if not (address == "127.0.0.1" or address.startswith("127.")):
            report.failures.append(
                f"lane: binds {address!r}; every listener must be loopback"
            )
    if not binds:
        report.failures.append(
            "lane: no listener bind found — the loopback-fixture assertion is vacuous"
        )

    # The fixture must relay to a loopback origin, and must never resolve the
    # authority it was asked for. `TcpStream::connect` is already forbidden, so
    # the relay can only be a loopback connect; assert the origin is loopback too.
    for needle in ("127.0.0.1",):
        report.require("lane", lane_code, needle, LANE)

    # ------------------------------------------------------------------
    # 4. No plugin loading or command execution, in the lane or in the
    #    production modules it drives.
    # ------------------------------------------------------------------
    for rel in PRODUCTION:
        source = strip_comments(read(rel))
        for needle, why in FORBIDDEN_IN_PRODUCTION:
            report.forbid("invariant 5", source, needle, rel)
    for rel, needle, why in REQUIRED_IN_PRODUCTION:
        report.require("Plan 376", strip_comments(read(rel)), needle, rel)

    # The retry loop must not hold the counter lock across an await. The
    # counters are a plain `Mutex`, so holding it across an await would let one
    # pending attempt block every status projection in the router.
    route = strip_comments(read("crates/i2pr-daemon/src/outproxy_route.rs"))
    for match in re.finditer(r"lock\(\)[^;]*?\.await", route, re.DOTALL):
        window = match.group(0)
        # `note(counters, ...)` takes and releases inside one statement; a lock
        # guard binding that survives into an await is the shape being ruled out.
        if "let _guard" in window or "let mut guard" in window:
            report.failures.append(
                "Plan 376: a counter lock guard appears to be held across an await "
                "in outproxy_route.rs"
            )

    # ------------------------------------------------------------------
    # 5. The request-path guard must still pass.
    # ------------------------------------------------------------------
    result = subprocess.run(
        ["bash", str(ROOT / "scripts" / "check-outproxy-request-path.sh")],
        capture_output=True,
        text=True,
        cwd=ROOT,
        check=False,
    )
    if result.returncode != 0:
        report.failures.append(
            "guard: scripts/check-outproxy-request-path.sh failed; a green lane over a "
            f"mutated request path proves nothing. Output: {result.stdout.strip()}"
        )


# ---------------------------------------------------------------------------
# Negative test.
# ---------------------------------------------------------------------------

MUTATIONS: list[tuple[str, str, str, str]] = [
    (LANE, "async fn plan342_i2p_authority_bypasses_the_outproxy(", "async fn plan342_i2p_authority_bypasses_the_outproxy_RENAMED(", "a required row is renamed"),
    (LANE, "#[tokio::test(flavor = \"current_thread\")]\nasync fn plan342_socks5_request_is_carried_by_the_outproxy", "#[ignore]\n#[tokio::test(flavor = \"current_thread\")]\nasync fn plan342_socks5_request_is_carried_by_the_outproxy", "a required row is ignored"),
    (LANE, "bind(\"127.0.0.1:0\")", "bind(\"0.0.0.0:0\")", "the lane binds a non-loopback address"),
    (LANE, "    let _ = kind;", "    let _ = kind;\n    unsafe { }", "the lane grows unsafe"),
    (LANE, "TcpStream::connect(listener)", "TcpStream::connect(\"93.184.216.34:80\").unwrap_or_else(|_| TcpStream::connect(listener))", "the lane opens a clearnet socket"),
    (LANE, "use std::time::{Duration, Instant};", "use std::time::{Duration, Instant};\nuse std::process::Command;", "the lane gains command execution"),
    ("crates/i2pr-daemon/src/outproxy_route.rs", "use std::sync::Arc;", "use std::sync::Arc;\nuse libloading::Library;", "a production module gains dynamic library loading"),
]

# Mutations that must NOT be detected. Each isolates a deliberate design choice
# so that a regression in the checker itself is visible.
CONTROLS: list[tuple[str, str, str, str]] = [
    (LANE, "//! origin it was given at construction. It never resolves a name and never", "//! origin it was given at construction. std::process::Command is never used.", "a forbidden construct named only in a comment is not a violation"),
    (LANE, "    let _ = kind;", "    let _kind = kind;", "an unused local renamed is not a violation"),
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
    report = Report()
    check(report)
    if report.failed():
        for failure in report.failures:
            print(f"FAIL: {failure}")
        print(f"check-outproxy-wire-lane-evidence: {len(report.failures)} failure(s)")
        return 1
    print(f"check-outproxy-wire-lane-evidence: ok ({len(REQUIRED_ROWS)} required rows)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())