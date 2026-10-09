#!/usr/bin/env python3
"""Static boundary checks for the Plan 368 trusted AppManager bridge.

Every rule here exists because a plausible implementation would otherwise look
identical to the correct one. The checker fails closed: a rule that cannot find
what it asserts is a failure, not a pass.

Rules
  1. Only the daemon consumes the manager protocol as a router-side
     implementation; the contract crate has no production reverse dependency.
  2. The contract crate cannot reach a router, runtime, or OS owner.
  3. The daemon bridge's only host listener is the explicitly authorized
     127.0.0.1 local-service listener; no other bind/connect path is allowed.
  4. The control vocabulary carries no package, grant, configuration, process,
     or policy operation, and no administrator variant.
  5. `control_scoped` is unrepresentable in the service vocabulary.
  6. The application-facing `hello` is never referenced as authorization input,
     and no `RequestedCapability` reaches the bridge's production path.
  7. The bridge routes every session through the existing administrator-grant
     path rather than trusting the manager's assertion.
  8. Manager authority never reaches a second manager session's service stream.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path


def fail(rule: str, detail: str) -> None:
    raise SystemExit(f"check-managed-app-manager-boundary: FAIL [{rule}]: {detail}")


def require(rule: str, condition: bool, detail: str) -> None:
    if not condition:
        fail(rule, detail)


def normalise_use_groups(source: str) -> list[str]:
    """Flattens `use a::{b, c as d, ...};` into individual leaf strings.

    A naive scan for `std::net` misses `use std::{fs, net};`. This is the same
    use-tree parser strategy as `check-runtime-boundaries.sh`.
    """
    leaves: list[str] = []
    index = 0
    length = len(source)
    while index < length:
        if not source.startswith("use", index):
            index += 1
            continue
        after = index + 3
        if after < length and (source[after].isalnum() or source[after] == "_"):
            index += 1
            continue
        # A commented-out `use` is documentation, not an import.
        line_start = source.rfind("\n", 0, index) + 1
        if source[line_start:index].strip().startswith("//"):
            index += 3
            continue
        # Walk to the terminating `;` at brace depth zero.
        depth = 0
        cursor = after
        while cursor < length:
            char = source[cursor]
            if char == "{":
                depth += 1
            elif char == "}":
                depth -= 1
            elif char == ";" and depth == 0:
                break
            cursor += 1
        if cursor >= length:
            break
        tree = source[after:cursor].strip()
        leaves.extend(_expand_use_tree(tree))
        index = cursor + 1
    return leaves


def _split_group(rest: str) -> tuple[str, str] | None:
    """Splits `rest` into (group body, remainder after the matching `}`).

    The caller has already consumed the opening `{`, so the matching `}` is the
    point where depth returns to zero.
    """
    depth = 1
    for position, char in enumerate(rest):
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return rest[:position], rest[position + 1 :]
    return None


def _expand_use_tree(tree: str, bases: list[str] | None = None) -> list[str]:
    """Flattens one use-tree into fully-qualified leaf paths.

    Handles sibling groups (`use a::{b, c}::{d, e};`) as well as nested ones.
    Every branch contributes at least one leaf: dropping a path here would be
    fail-open, because the dropped path is exactly the one a smuggled forbidden
    import would hide in.
    """
    bases = list(bases) if bases is not None else [""]
    tree = tree.strip()
    if "{" not in tree:
        # Strip the alias before collapsing whitespace, or `TcpStream as S` would
        # first collapse to `TcpStreamasS` and the split would never match.
        leaf = re.sub(r"\s+as\s+[A-Za-z_][A-Za-z0-9_]*$", "", tree.strip())
        leaf = re.sub(r"\s+", "", leaf)
        if leaf == "self":
            # `self` names the enclosing module: the base path minus its last
            # segment.
            return [_parent_path(base) for base in bases]
        return [base + leaf for base in bases]

    head, rest = tree.split("{", 1)
    head = re.sub(r"\s+", "", head)
    if "{" in head:
        # A brace group before the first `{` (`a::{b,c}::{d}` has the group after
        # the head, but `a::b::{c}::{d}` style heads resolve to several bases).
        new_bases: list[str] = []
        for base in bases:
            new_bases.extend(_expand_use_tree(head, [base]))
        head_bases = new_bases
    else:
        head_bases = [base + head for base in bases]

    results: list[str] = []
    current = rest
    while True:
        split = _split_group(current)
        if split is None:
            # Unbalanced braces: keep the raw text as a leaf rather than dropping it.
            tail = re.sub(r"\s+", "", current)
            if tail:
                results.extend(_expand_use_tree(tail, head_bases))
            return results
        body, remainder = split
        for entry in _split_top_level(body):
            entry = entry.strip()
            if entry:
                results.extend(_expand_use_tree(entry, head_bases))
        remainder = remainder.strip()
        # A trailing `::{...}` sibling group resolves against the same head.
        if remainder.startswith("::"):
            current = remainder[2:]
            continue
        if remainder:
            results.extend(_expand_use_tree(remainder, head_bases))
        return results


def _parent_path(path: str) -> str:
    return path.rsplit("::", 1)[0] if "::" in path else path


def _split_top_level(body: str) -> list[str]:
    parts: list[str] = []
    depth = 0
    current = []
    for char in body:
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
        if char == "," and depth == 0:
            parts.append("".join(current))
            current = []
            continue
        current.append(char)
    parts.append("".join(current))
    return parts


def strip_comments(source: str) -> str:
    """Removes `//` line comments so identifier rules test code, not prose.

    Documentation is allowed to *name* a forbidden construct in order to forbid
    it. Only executable code may be scanned, or the rule could not be written
    honestly in the module's own docs.
    """
    lines = []
    for line in source.splitlines():
        stripped = line.strip()
        if stripped.startswith("//"):
            continue
        # Keep `str` literals intact but drop a trailing `//` comment.
        index = 0
        in_string = False
        escaped = False
        while index < len(line):
            char = line[index]
            if escaped:
                escaped = False
            elif char == "\\" and in_string:
                escaped = True
            elif char == '"':
                in_string = not in_string
            elif char == "/" and not in_string and line.startswith("//", index):
                break
            index += 1
        lines.append(line[:index])
    return "\n".join(lines)


ROOT = Path(".")
CONTRACT_DIR = ROOT / "crates/i2pr-app-manager-proto"
CONTRACT_LIB = CONTRACT_DIR / "src/lib.rs"
BRIDGE = ROOT / "crates/i2pr-daemon/src/app_manager_bridge.rs"
WORKSPACE_MANIFEST = (ROOT / "Cargo.toml").read_text()

require(
    "1-members",
    (CONTRACT_DIR / "Cargo.toml").is_file(),
    "the i2pr-app-manager-proto crate is missing from the workspace",
)
require(
    "1-members",
    '"crates/i2pr-app-manager-proto"' in WORKSPACE_MANIFEST,
    "i2pr-app-manager-proto must be an explicit workspace member",
)
require("1-contract-source", CONTRACT_LIB.is_file(), "contract crate lib.rs is missing")

contract_source = CONTRACT_LIB.read_text()
contract_code = strip_comments(contract_source)
bridge_source = BRIDGE.read_text()
bridge_production = strip_comments(bridge_source.split("#[cfg(test)]", 1)[0])
contract_manifest = (CONTRACT_DIR / "Cargo.toml").read_text()

# -- rule 1: one-way dependency -------------------------------------------
contract_dependencies = re.findall(r'^(\S+)\s*=', contract_manifest, flags=re.MULTILINE)
for forbidden in ("i2pr-daemon", "i2pr-runtime", "i2pr-client", "i2pr-daemon"):
    require(
        "1-one-way",
        forbidden not in contract_dependencies,
        f"the contract crate must not depend on {forbidden}",
    )
require(
    "1-one-way",
    'i2pr-app-proto = { path = "../i2pr-app-proto" }' in contract_manifest,
    "the contract crate must keep exactly the app-protto dependency",
)
# Exactly two implementations may consume the protocol, and no others.
#
# Plan 368 closed with a single consumer: `i2pr-daemon`, which owns the bridge.
# Plan 369 WP2 adds the second and only other one, `i2pr-appd`, because the
# manager process is the protocol's *client* — it is the party that speaks
# manager-protocol to the daemon over the inherited transport. That is the
# whole point of the protocol, so admitting it is a design fact, not a
# relaxation: the list is an exact allow-set, so a third consumer (a tool, a
# test fixture, an SDK) still fails here.
# An exact allow-set, not a pattern: the manager protocol may be consumed by the
# daemon bridge, the appd manager client, and the apphost host side of the
# bootstrap, and by nothing else. A fourth consumer is a hard failure.
#
# Plan 369 WP3 added `i2pr-apphost`. It is the other end of the bootstrap
# handshake defined by this protocol, so it is a legitimate consumer; it is in a
# separate trust zone and is forbidden by the dependency-direction script from
# depending on `i2pr-appd`, which is what keeps the zone split real.
#
# Plan 369 WP5 added `i2pr-app-fixture`. It is **evidence tooling**, and this is
# the one place that says so in an executable way: the fixture application is
# exec'd by `i2pr-apphost` as a real process, so it must be able to *decode* the
# bootstrap contract and speak app v1 without linking the manager that produces
# it. That is a decoder capability, not a manager implementation -- the fixture
# has no `LaunchAuthority`, no catalog, and no way to originate a session.
# `scripts/check-managed-app-process-boundary.py` is what keeps the difference
# honest: it asserts no production module can name this crate at all.
# Sorted: the comparison below is against `sorted(consumers)`.
ALLOWED_PROTOCOL_CONSUMERS = [
    "i2pr-app-fixture",
    "i2pr-appd",
    "i2pr-apphost",
    "i2pr-daemon",
]
consumers = []
for cargo_manifest in (ROOT / "crates").glob("*/Cargo.toml"):
    if "i2pr-app-manager-proto = { path =" in cargo_manifest.read_text():
        consumers.append(cargo_manifest.parent.name)
require(
    "1-one-way",
    sorted(consumers) == ALLOWED_PROTOCOL_CONSUMERS,
    "the protocol may be consumed only as an implementation by the daemon bridge, "
    f"the appd manager client, and the apphost bootstrap host; found {sorted(consumers)}",
)

# -- rule 2: the contract crate reaches no owner ----------------------------
contract_leaves = normalise_use_groups(contract_code)
for leaf in contract_leaves:
    require(
        "2-contract-pure",
        not leaf.startswith(("std::net", "tokio::net", "std::fs", "std::process",
                             "std::thread", "std::env", "tokio::process", "std::time",
                             "tokio::time", "tokio::signal")),
        f"the contract crate must not name a runtime/OS owner: {leaf}",
    )
    require(
        "2-contract-pure",
        not leaf.startswith(("i2pr-daemon", "i2pr-runtime", "i2pr-client", "i2pr-core",
                             "i2pr-api", "i2pr-console", "i2pr-netdb")),
        f"the contract crate must not depend on a router/runtime crate: {leaf}",
    )
for forbidden in ("async fn", ".await", "spawn", "std::io::", "Mutex"):
    require(
        "2-contract-pure",
        forbidden not in contract_code,
        f"the contract crate must stay runtime-neutral; found {forbidden!r}",
    )
require(
    "2-contract-pure",
    "#![forbid(unsafe_code)]" in contract_source,
    "the contract crate must forbid unsafe code",
)
require(
    "2-contract-pure",
    "pub trait ManagerTransport" not in contract_source,
    "transport binding belongs to the daemon bridge, not the contract crate",
)

# -- rule 3: only the authorized loopback listener is allowed -------------
bridge_leaves = normalise_use_groups(bridge_production)
for leaf in bridge_leaves:
    require(
        "3-loopback-only",
        not leaf.startswith(("std::net", "std::os::unix::net", "hyper::",
                             "axum::", "tokio::process", "std::process", "std::fs")),
        f"the bridge must not name an unapproved socket/process API: {leaf}",
    )
    if leaf.startswith("tokio::net"):
        require(
            "3-loopback-only",
            leaf in {"tokio::net::TcpListener", "tokio::net::TcpStream"},
            f"only the fixed loopback listener and its accepted TCP streams are allowed: {leaf}",
        )
for forbidden in (
    "UdpSocket", "TcpSocket", "UnixListener", "UnixStream", "socket2::",
    "connect(", "0.0.0.0", "[::]",
):
    require(
        "3-loopback-only",
        forbidden not in bridge_production,
        f"the bridge must not contain an alternate socket path: {forbidden!r}",
    )
require(
    "3-loopback-only",
    "tokio::net::TcpListener" in bridge_leaves and "tokio::net::TcpStream" in bridge_leaves,
    "the daemon bridge may use only Tokio's TCP listener and accepted stream types",
)
require(
    "3-loopback-only",
    len(re.findall(r"TcpListener::bind\(\(std::net::Ipv4Addr::LOCALHOST, port\)\)", bridge_production)) == 1
    and len(re.findall(r"\bbind\(", bridge_production)) == 1,
    "the sole listener must bind Ipv4Addr::LOCALHOST and no caller-selected address",
)
require(
    "3-loopback-only",
    "trait ManagerTransport: AsyncRead + AsyncWrite" in bridge_production,
    "the private manager transport must remain injected",
)
require(
    "3-loopback-only",
    not any(token in strip_comments((ROOT / "crates/i2pr-appd/src/session.rs").read_text())
            for token in ("tokio::net", "std::net::Tcp", "TcpListener", "TcpStream")),
    "appd must remain free of listener and socket ownership",
)

# -- rule 4: the control vocabulary has no admin authority ------------------
for forbidden in (
    "package", "install", "uninstall", "grant", "revoke", "permission_persist",
    "launch_profile", "network_policy", "config_", "process_", "admin",
    "i2pcontrol", "I2cpMessage::", "set_date", "update",
):
    require(
        "4-no-admin",
        f'"{forbidden}' not in contract_source and f'"{forbidden}"' not in contract_source,
        f"the control vocabulary must not carry an admin operation: {forbidden!r}",
    )
require(
    "4-no-admin",
    "Administrator" not in contract_source,
    "the contract crate must not define or accept an administrator role",
)
# The manager protocol must not import the administrator Proposal 170 surface.
require(
    "4-no-admin",
    "i2pr-i2pcontrol" not in contract_manifest,
    "the contract crate must not depend on the Proposal 170 crate",
)

# -- rule 5: control_scoped is unrepresentable ------------------------------
service_block = contract_source.split("pub enum ManagerService {", 1)[1].split("}", 1)[0]
require(
    "5-control-scoped",
    "Sam" in service_block and "I2cp" in service_block,
    "the service vocabulary must contain sam and i2cp",
)
require(
    "5-control-scoped",
    "ControlScoped" not in service_block,
    "control_scoped must be unrepresentable in the manager service vocabulary",
)
require(
    "5-control-scoped",
    '"control_scoped" => Err(ManagerProtocolError::UnsupportedService)' in contract_source,
    "the control_scoped spelling must be recognised and refused by type",
)
require(
    "5-control-scoped",
    "ControlScoped" not in bridge_production,
    "the bridge must not name control_scoped as an openable service",
)

# -- rule 6: application declarations never construct authority ------------
for forbidden in ("AppToHostMessage", "AppFromHostMessage", "RequestedCapability", "hello"):
    require(
        "6-no-app-authority",
        forbidden not in bridge_production,
        f"the bridge must not build authority from an application declaration: {forbidden}",
    )
require(
    "6-no-app-authority",
    "GrantedCapability::from_administrator_policy" in bridge_production,
    "manager grants must be re-derived through the administrator-grant path",
)
require(
    "6-no-app-authority",
    "EffectiveCapabilities::from_grants" in bridge_production,
    "effective capabilities must be rebuilt from validated grants",
)
require(
    "6-no-app-authority",
    "EffectiveCapabilities::from_requests" not in bridge_production,
    "effective capabilities must never be built from requested capabilities",
)

# -- rule 7: bounded authority ceiling in the bridge ------------------------
# The session and stream ceilings live in the contract crate (pinned by
# `declared_ceilings_match_the_plan_368_contract`); the bridge must *use* that
# bounded accounting rather than counting on its own, and must bound its own
# queues locally.
for required in (
    "ManagerScopeLimits",
    "ServiceStreamLedger",
    "MAX_QUEUED_INBOUND_CHUNKS",
    "MAX_QUEUED_OUTBOUND_FRAMES",
):
    require(
        "7-bounded",
        required in bridge_production,
        f"the bridge must apply the bounded accounting {required}",
    )
require(
    "7-bounded",
    "try_send" in bridge_production,
    "service forwarding must use a bounded queue",
)
require(
    "7-bounded",
    "tokio::io::duplex(" in bridge_production,
    "each service stream must use a bounded in-memory duplex",
)
# The bridge must not keep its own unbounded counter that could drift from the
# contract ceilings.
require(
    "7-bounded",
    not re.search(r"\bBTreeSet<usize>", bridge_production),
    "the bridge must not maintain a local unbounded-limit counter",
)

# -- rule 8: session isolation ---------------------------------------------
require(
    "8-isolation",
    "sessions: Mutex<BTreeMap<ManagerSessionId, Arc<Session>>>" in bridge_production,
    "sessions must be tracked per manager session handle",
)
require(
    "8-isolation",
    "inbound: BTreeMap<ManagerServiceStreamId, mpsc::Sender<Vec<u8>>>" in bridge_production,
    "service streams must be keyed inside the owning session",
)
require(
    "8-isolation",
    "ManagerSessionId::new(" in bridge_production and "AtomicU64" in bridge_production,
    "handles must be daemon-assigned and monotonically allocated",
)

print("managed app manager boundary: ok")
sys.exit(0)
