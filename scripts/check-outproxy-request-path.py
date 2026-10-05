#!/usr/bin/env python3
"""Plan 342 static guard: the I2P-routed outproxy request-path boundary.

The shell wrapper ``scripts/check-outproxy-request-path.sh`` documents the
boundary and execs this file. The checks live here because they need to
resolve a *named function body* by brace matching; grepping a whole file
cannot tell a live call from a mention, and a guard that cannot distinguish
those is not a boundary check.

Run ``--mutation-table`` to reproduce the negative test: it applies fourteen
deliberate mutations to the sources and asserts that this guard rejects each
one. A guard that has never been shown to fail is a comment.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

HTTP = "crates/i2pr-daemon/src/service_tunnels_http.rs"
SOCKS = "crates/i2pr-daemon/src/service_tunnels_socks5.rs"
ROUTE = "crates/i2pr-daemon/src/outproxy_route.rs"
CONTROL = "crates/i2pr-daemon/src/i2pcontrol_tunnels.rs"
PUMP = "crates/i2pr-daemon/src/service_tunnels.rs"
OPTIONS = "crates/i2pr-daemon/src/outproxy_options.rs"
SERVICE_OUTPROXY = "crates/i2pr-service-tunnels/src/outproxy.rs"


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


def fn_body(source: str, name: str) -> str:
    """Return the body of `fn name(...)` by brace matching.

    Finds the *first* definition. A caller that shadows the name with a local
    binding is not a definition and is not matched, which is what makes this
    a definition lookup rather than a substring lookup.
    """
    match = re.search(rf"\bfn\s+{re.escape(name)}\s*\(", source)
    if match is None:
        raise LookupError(name)
    # Walk back to the start of the signature so attributes and the return
    # type are inside the extracted span.
    start = source.rfind("\n", 0, match.start()) + 1
    brace = source.find("{", match.end())
    if brace == -1:
        raise LookupError(f"{name}: no body")
    depth = 0
    index = brace
    in_string: str | None = None
    in_line_comment = False
    in_block_comment = False
    while index < len(source):
        char = source[index]
        nxt = source[index + 1] if index + 1 < len(source) else ""
        if in_line_comment:
            in_line_comment = char != "\n"
        elif in_block_comment:
            in_block_comment = not (char == "*" and nxt == "/")
            if not in_block_comment:
                index += 1
        elif in_string is not None:
            if char == "\\":
                index += 1
            elif char == in_string:
                in_string = None
        elif char == "/" and nxt == "/":
            in_line_comment = True
        elif char == "/" and nxt == "*":
            in_block_comment = True
            index += 1
        elif char in ('"', "'"):
            # Raw strings and char literals are not used in the spans these
            # checks read, and a lone quote inside a doc comment is already
            # skipped by the comment branches above.
            in_string = char
        elif char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return source[start : index + 1]
        index += 1
    raise LookupError(f"{name}: unbalanced braces")


def braced_span(source: str, brace: int) -> str:
    """Return `source[brace..]` through the matching closing brace."""
    depth = 0
    for index in range(brace, len(source)):
        char = source[index]
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return source[brace : index + 1]
    raise LookupError("unbalanced braces")


def check(report: Report) -> None:
    http = read(HTTP)
    socks = read(SOCKS)
    route = read(ROUTE)
    control = read(CONTROL)
    pump = read(PUMP)
    options = read(OPTIONS)
    service = read(SERVICE_OUTPROXY)

    # ------------------------------------------------------------------
    # 1. Each request path classifies, inside the function that requests.
    # ------------------------------------------------------------------
    connect = fn_body(http, "handle_connect")
    report.require("CONNECT classification", connect, "classify_client_target(", "handle_connect")
    # The classification must be *bound* and its result consumed. A body that
    # classifies and then discards the value passes a bare `require` on the
    # symbol, which is the whole reason these rows are stated as the
    # assignment form rather than a substring.
    report.require(
        "CONNECT binding",
        connect,
        "let class = match classify_client_target(",
        "handle_connect",
    )
    report.require("CONNECT binding", connect, "match class {", "handle_connect")
    for variant in ("Direct", "ViaOutproxy", "Refused"):
        report.require(
            "CONNECT classification",
            connect,
            f"ClientTargetClass::{variant}",
            "handle_connect",
        )
    # The classification must happen before anything is opened. Index of the
    # first classification vs the first open on the direct path.
    classified_at = connect.index("classify_client_target(")
    opened_at = connect.index("open_streaming(")
    if classified_at > opened_at:
        report.failures.append(
            "CONNECT order: handle_connect opens Streaming before classifying the target"
        )

    socks_conn = fn_body(socks, "run_socks5_connection")
    report.require(
        "SOCKS5 classification", socks_conn, "classify_client_target(", "run_socks5_connection"
    )
    report.require(
        "SOCKS5 binding",
        socks_conn,
        "let class = match classify_client_target(",
        "run_socks5_connection",
    )
    report.require("SOCKS5 binding", socks_conn, "match class {", "run_socks5_connection")
    for variant in ("Direct", "ViaOutproxy", "Refused"):
        report.require(
            "SOCKS5 classification",
            socks_conn,
            f"ClientTargetClass::{variant}",
            "run_socks5_connection",
        )
    classified_at = socks_conn.index("classify_client_target(")
    opened_at = socks_conn.index("open_streaming(")
    if classified_at > opened_at:
        report.failures.append(
            "SOCKS5 order: run_socks5_connection opens Streaming before classifying the target"
        )

    opener = fn_body(route, "open_client_route")
    report.require("shared opener", opener, "classify_client_target(", "open_client_route")
    for variant in ("Direct", "ViaOutproxy", "Refused"):
        report.require("shared opener", opener, f"ClientTargetClass::{variant}", "open_client_route")
    report.require(
        "shared opener", opener, "open_via_outproxy(", "open_client_route"
    )

    # ------------------------------------------------------------------
    # 2. A refusal is answered, not fallen through.
    # ------------------------------------------------------------------
    report.require(
        "CONNECT refusal",
        connect,
        "HttpConnectionOutcome::BadGateway",
        "handle_connect",
    )
    report.require(
        "SOCKS5 refusal",
        socks_conn,
        "Socks5ConnectionOutcome::BadGateway",
        "run_socks5_connection",
    )
    report.require(
        "SOCKS5 refusal",
        socks_conn,
        "Socks5ReplyCode::HostUnreachable",
        "run_socks5_connection",
    )
    # The `Refused` arm must return; an arm that falls out of the match would
    # run the direct path afterwards.
    for label, body, needle in (
        ("CONNECT refusal", connect, "ClientTargetClass::Refused(failure) => {"),
        ("SOCKS5 refusal", socks_conn, "ClientTargetClass::Refused(failure) => {"),
    ):
        index = body.find(needle)
        if index == -1:
            report.failures.append(f"{label}: the Refused arm must bind the failure by name")
            continue
        # Span the arm by brace matching. A fixed-width window would run past
        # the arm into the next one and find a `return` belonging to the
        # direct path — which is exactly the defect being tested for.
        arm = braced_span(body, body.index("{", index))
        if "return " not in arm:
            report.failures.append(
                f"{label}: the Refused arm must return, or control falls into the direct path"
            )

    # ------------------------------------------------------------------
    # 3. The handshake prefix reaches the pump's inbound direction.
    # ------------------------------------------------------------------
    connect_pump = fn_body(http, "connect_via_outproxy")
    report.require(
        "CONNECT pump", connect_pump, "new_client_with_prefix(", "connect_via_outproxy"
    )
    report.require("CONNECT pump", connect_pump, "session.tunnel_prefix", "connect_via_outproxy")
    socks_pump = fn_body(socks, "pump_via_outproxy")
    report.require("SOCKS5 pump", socks_pump, "new_client_with_prefix(", "pump_via_outproxy")
    report.require("SOCKS5 pump", socks_pump, "session.tunnel_prefix", "pump_via_outproxy")

    prefix_ctor = fn_body(pump, "new_client_with_prefix")
    report.require("pump endpoint", prefix_ctor, "tunnel_prefix", "new_client_with_prefix")
    drain = fn_body(pump, "drain_delivered")
    report.require("pump endpoint", drain, "tunnel_prefix", "ServicePumpEndpoint::drain_delivered")
    # Consumed exactly once: a replay would re-inject handshake bytes into
    # every later drain and corrupt the stream.
    report.require(
        "pump endpoint", drain, "std::mem::take(&mut *prefix)", "ServicePumpEndpoint::drain_delivered"
    )
    if "prefix.clone()" in drain or "*prefix = prefix" in drain:
        report.failures.append(
            "pump endpoint: drain_delivered must consume the prefix, never replay it"
        )

    open_one = fn_body(route, "open_one")
    report.require("outproxy session", open_one, "tunnel_prefix,", "open_one")
    # The pump addresses the Streaming bridge by `(connection_id, remote)`,
    # so the session must carry the remote it actually opened. Without it a
    # request path would have to re-resolve the outproxy and could disagree.
    report.require(
        "outproxy session", open_one, "remote: client.remote.clone()", "open_one"
    )

    # The strict-CONNECT executor is one of the request paths: it adapts a
    # CONNECT-only tunnel's options into the shared handler, so it must carry
    # the route policy along or that tunnel silently loses the block it
    # parsed.
    strict = fn_body(http, "run_connect_only_connection")
    report.require(
        "strict-CONNECT adapter", strict, "outproxy: options.outproxy.clone()", "handle_strict_connect"
    )

    # ------------------------------------------------------------------
    # 4. No request path can name-resolve or connect directly.
    # ------------------------------------------------------------------
    for label, rel, body in (
        ("CONNECT request path", HTTP, http),
        ("SOCKS5 request path", SOCKS, socks),
        ("shared opener", ROUTE, route),
    ):
        for primitive in ("lookup_host", "TcpStream::connect", "ToSocketAddrs"):
            report.forbid(label, body, primitive, rel)
    # The outproxy itself is only ever reached over I2P Streaming.
    report.require(
        "outproxy opener", fn_body(route, "open_one"), "open_outproxy_streaming(", "open_one"
    )

    # ------------------------------------------------------------------
    # 5. The credential is sealed once and opened once.
    # ------------------------------------------------------------------
    normalize = fn_body(control, "normalize_definition_with_filter_root")
    report.require(
        "seal step", normalize, "parse_outproxy_block(", "normalize_definition_with_filter_root"
    )
    report.require(
        "seal step",
        normalize,
        "seal_outproxy_credential(",
        "normalize_definition_with_filter_root",
    )
    seal = fn_body(control, "seal_outproxy_credential")
    report.require("seal step", seal, "store.seal(", "seal_outproxy_credential")
    report.require("seal step", seal, "store.is_available()", "seal_outproxy_credential")
    report.require(
        "seal step", seal, "no outbound secret owner is installed", "seal_outproxy_credential"
    )
    report.require(
        "seal step",
        seal,
        "OutboundSecret::new(",
        "seal_outproxy_credential",
    )
    # It must wrap the plaintext rather than store the string form.
    if "persisted.insert(" in seal and "sealed" not in seal:
        report.failures.append("seal step: the persisted value must be the sealed form")

    auth = fn_body(route, "auth_header")
    report.require(
        "credential reader", auth, "self.store.open(", "RouterOutproxyProvider::auth_header"
    )

    # The block rule lives in one module and admits seven keys or none.
    classify = fn_body(options, "parse_outproxy_block")
    report.require("block rule", classify, "all-or-none", "parse_outproxy_block")
    report.require(
        "block rule",
        classify,
        "OUTPROXY_BLOCK_KEYS.len()",
        "parse_outproxy_block",
    )
    report.require(
        "block rule", classify, "kind_accepts_outproxy_block(kind)", "parse_outproxy_block"
    )
    # `UseOutproxyPlugin:false` must stay a refusal: accepting it would store a
    # ProxyList that no request path consults, which is the inert acceptance
    # the all-or-none rule exists to prevent.
    report.require("block rule", classify, "if !use_plugin", "parse_outproxy_block")
    report.require(
        "block rule",
        classify,
        "UseOutproxyPlugin:false declares no provider",
        "parse_outproxy_block",
    )

    # A direct-clearnet fallback must not exist in either the route enum or
    # the classifier. This is invariant 1 as a type-level property.
    for rel, source in ((ROUTE, route), (OPTIONS, options), (SERVICE_OUTPROXY, service)):
        for forbidden in ("DirectClearnet", "FallbackToDirect", "DirectClearnetSocket"):
            report.forbid("invariant 1", source, forbidden, rel)

    # The classifier's `Direct` arm must be reachable only for an `.i2p`
    # target, so the guard checks the order inside its own body.
    service_classify = fn_body(service, "classify_client_target")
    i2p_at = service_classify.find("target.is_i2p()")
    direct_at = service_classify.find("ClientTargetClass::Direct(target)")
    if i2p_at == -1 or direct_at == -1:
        report.failures.append(
            "classifier: ClientTargetClass::Direct must be produced from the .i2p bypass"
        )
    elif i2p_at > direct_at:
        report.failures.append(
            "classifier: the .i2p check must precede the Direct arm, or a clearnet host could "
            "reach it"
        )


# ---------------------------------------------------------------------------
# Negative test.
# ---------------------------------------------------------------------------

MUTATIONS: list[tuple[str, str, str, str]] = [
    # -- 1. a request path stops classifying, or discards the classification --
    (HTTP, "    let class = match classify_client_target(", "    let _swallowed = match classify_client_target(", "CONNECT classifies and throws the result away"),
    (HTTP, "    let class = match classify_client_target(", "    let class = match Ok::<ClientTargetClass, _>(ClientTargetClass::Direct(OutproxyTarget::parse_authority(\"x.b32.i2p:80\").unwrap())).and_then(|_| classify_client_target(", "CONNECT stops consulting the classifier"),
    (SOCKS, "    let class = match classify_client_target(", "    let _swallowed = match classify_client_target(", "SOCKS5 classifies and throws the result away"),
    (SOCKS, "    let class = match classify_client_target(", "    let class = match Ok::<ClientTargetClass, _>(ClientTargetClass::Direct(OutproxyTarget::parse_authority(\"x.b32.i2p:80\").unwrap())).and_then(|_| classify_client_target(", "SOCKS5 stops consulting the classifier"),
    (HTTP, "    match class {\n", "    if false {\n", "CONNECT stops matching the classification"),
    (SOCKS, "    match class {\n", "    if false {\n", "SOCKS5 stops matching the classification"),
    # -- 2. a refusal is not answered --
    (HTTP, "ClientTargetClass::Refused(failure) => {", "ClientTargetClass::Refused(_) => {", "CONNECT discards the refusal reason"),
    (SOCKS, "ClientTargetClass::Refused(failure) => {", "ClientTargetClass::Refused(_) => {", "SOCKS5 discards the refusal reason"),
    (SOCKS, "            .await;\n            return Socks5ConnectionOutcome::BadGateway;\n        }\n        ClientTargetClass::Direct(_) => {}", "            .await;\n        }\n        ClientTargetClass::Direct(_) => {}", "the SOCKS5 refusal arm stops returning"),
    (HTTP, "            .await;\n            return HttpConnectionOutcome::BadGateway;\n        }\n        ClientTargetClass::Direct(_) => {}", "            .await;\n        }\n        ClientTargetClass::Direct(_) => {}", "the CONNECT refusal arm stops returning"),
    # -- 3. the handshake prefix is lost or replayed --
    (HTTP, "ServicePumpEndpoint::new_client_with_prefix(", "ServicePumpEndpoint::new_client(", "CONNECT drops the handshake prefix"),
    (SOCKS, "ServicePumpEndpoint::new_client_with_prefix(", "ServicePumpEndpoint::new_client(", "SOCKS5 drops the handshake prefix"),
    (PUMP, "std::mem::take(&mut *prefix)", "prefix.clone()", "the pump replays the prefix on every drain"),
    (ROUTE, "        tunnel_prefix,\n    })", "        tunnel_prefix: Vec::new(),\n    })", "the session discards the handshake prefix"),
    (ROUTE, "        remote: client.remote.clone(),\n", "", "the session stops carrying the outproxy remote"),
    # -- 4. the outproxy stops being reached over I2P --
    (ROUTE, "match open_outproxy_streaming(manager, destination_id, &client.remote, connect_timeout_ms)", "match open_clearnet_directly(manager, destination_id, &client.remote, connect_timeout_ms)", "the outproxy opener stops using Streaming"),
    # -- 5. the credential is no longer sealed, owned, or gated --
    (CONTROL, "        seal_outproxy_credential(name, store, block.credential, &mut persisted)?;", "        let _ = block.credential;", "the seal step is skipped"),
    (CONTROL, '"{name}: no outbound secret owner is installed, so OutproxyPassword cannot be \\', '"{name}: no owner, so OutproxyPassword cannot be \\', "the no-owner refusal loses its message"),
    (ROUTE, "let secret: OutboundSecret = self.store.open(sealed)", "let secret: OutboundSecret = OutboundSecret::new(sealed)", "the provider stops going through the owner"),
    # -- 6. the block rule loosens --
    (OPTIONS, "    if !use_plugin {", "    if false {", "UseOutproxyPlugin:false is accepted"),
    (OPTIONS, "    if !kind_accepts_outproxy_block(kind) {", "    if false {", "the block is accepted on any tunnel kind"),
    (OPTIONS, "    if present.len() != OUTPROXY_BLOCK_KEYS.len() {", "    if present.is_empty() {", "the all-or-none rule becomes all-or-something"),
    (SERVICE_OUTPROXY, "    if target.is_i2p() {\n        return Ok(ClientTargetClass::Direct(target));\n    }", "    if true {\n        return Ok(ClientTargetClass::Direct(target));\n    }", "the classifier sends every target direct"),
    # -- 7. the strict-CONNECT path loses the policy --
    (HTTP, "        outproxy: options.outproxy.clone(),", "        outproxy: None,", "the strict-CONNECT adapter drops the route policy"),
]


def mutation_table() -> int:
    failures = 0
    for rel, find, replace, description in MUTATIONS:
        path = ROOT / rel
        original = path.read_text(encoding="utf-8")
        if find not in original:
            print(f"SKIP (anchor absent): {description}")
            failures += 1
            continue
        path.write_text(original.replace(find, replace, 1), encoding="utf-8")
        try:
            proc = subprocess.run(
                [
                    "bash",
                    str(ROOT / "scripts" / "check-outproxy-request-path.sh"),
                ],
                capture_output=True,
                text=True,
                cwd=ROOT,
            )
        finally:
            path.write_text(original, encoding="utf-8")
        if proc.returncode == 0:
            print(f"NOT DETECTED: {description}")
            failures += 1
        else:
            print(f"detected: {description}")
    print()
    if failures:
        print(f"{failures} mutation(s) not detected")
        return 1
    print(f"all {len(MUTATIONS)} mutations detected")
    return 0


def main() -> int:
    if "--mutation-table" in sys.argv[1:]:
        return mutation_table()
    report = Report()
    try:
        check(report)
    except LookupError as error:
        report.failures.append(f"could not resolve a required function: {error}")
    if report.failed():
        for failure in report.failures:
            print(f"FAIL: {failure}", file=sys.stderr)
        print(f"check-outproxy-request-path: {len(report.failures)} failure(s)", file=sys.stderr)
        return 1
    print("check-outproxy-request-path: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())