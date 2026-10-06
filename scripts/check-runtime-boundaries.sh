#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if grep -REn "unbounded_channel|unbounded::<|UnboundedSender|UnboundedReceiver" \
  "$root/crates/i2pr-runtime/src" "$root/crates/i2pr-testkit/src" >/dev/null; then
  echo "unbounded asynchronous channels are forbidden in runtime/testkit source" >&2
  exit 1
fi

if grep -REn 'std::thread::sleep|thread::sleep|std::mem::forget|mem::forget' \
  "$root/crates/i2pr-runtime" "$root/crates/i2pr-testkit" >/dev/null; then
  echo "wall-clock sleeps and handle-forgetting are forbidden in deterministic lanes" >&2
  exit 1
fi

spawn_matches=$(grep -REn 'tokio::spawn\(' "$root/crates/i2pr-runtime" "$root/crates/i2pr-testkit" || true)
if printf '%s\n' "$spawn_matches" | grep -Ev 'let .* =|push\(|JoinSet' | grep -Eq .; then
  echo "every tokio::spawn call must retain an explicit owner" >&2
  exit 1
fi

if grep -REn 'JoinHandle' "$root/crates/i2pr-runtime" "$root/crates/i2pr-testkit" >/dev/null; then
  echo "raw JoinHandle ownership requires a reviewed owner-specific implementation" >&2
  exit 1
fi

for manifest in "$root"/crates/*/Cargo.toml; do
  crate=$(basename "$(dirname "$manifest")")
  if [[ "$crate" != i2pr-runtime && "$crate" != i2pr-testkit ]] \
    && grep -En '^(tokio|tokio-util)[[:space:]]*=' "$manifest" >/dev/null; then
    echo "Tokio dependencies are confined to approved runtime/testkit manifests" >&2
    exit 1
  fi
done

testkit_dependents=$(grep -En 'i2pr-testkit' "$root/crates"/*/Cargo.toml || true)
if printf '%s\n' "$testkit_dependents" | grep -Ev 'crates/i2pr-testkit/Cargo.toml' | grep -Eq .; then
  echo "production crate depends on i2pr-testkit" >&2
  exit 1
fi

if grep -REn 'tokio::|std::net|std::fs|TcpStream|TcpListener|UdpSocket|UnixStream|OpenOptions|File::' \
  "$root/crates/i2pr-transport/src" "$root/crates/i2pr-transport-ntcp2/src" "$root/crates/i2pr-transport-ssu2/src" >/dev/null; then
  echo "transport contract crates must not own Tokio, sockets, or filesystem I/O" >&2
  exit 1
fi

# Plan 286: i2pr-i2pcontrol is a runtime-neutral control-contract crate.
# No Tokio, sockets, filesystem ownership, async runtime, router-state
# imports, or unbounded channels. Only the daemon adapts the contract.
if grep -REn 'tokio::|std::net|std::fs|TcpStream|TcpListener|UdpSocket|UnixStream|UnixListener|OpenOptions|File::|tokio::net|tokio::spawn|tokio::time|tokio::sync' \
  "$root/crates/i2pr-i2pcontrol/src" >/dev/null; then
  echo "i2pr-i2pcontrol must remain runtime-neutral: no Tokio, sockets, listeners, tasks, timers, or filesystem I/O" >&2
  exit 1
fi

if grep -REn 'async[[:space:]]+fn|async_trait' \
  "$root/crates/i2pr-i2pcontrol/src" >/dev/null; then
  echo "i2pr-i2pcontrol contracts must remain synchronous" >&2
  exit 1
fi

if grep -En 'i2pr-daemon|i2pr-runtime|i2pr-testkit|i2pr-netdb|i2pr-client|i2pr-service-tunnels|i2pr-tunnel|i2pr-transport' \
  "$root/crates/i2pr-i2pcontrol/Cargo.toml" >/dev/null; then
  echo "i2pr-i2pcontrol must not depend on daemon/runtime/service implementation owners" >&2
  exit 1
fi

if grep -REn "unbounded_channel|unbounded::<|UnboundedSender|UnboundedReceiver" \
  "$root/crates/i2pr-i2pcontrol/src" >/dev/null; then
  echo "unbounded asynchronous channels are forbidden in i2pr-i2pcontrol source" >&2
  exit 1
fi

# No UI/frontend dependency may enter the control-contract crate.
if grep -REn 'egui|iced|tauri|dioxus|yew|leptos|slint' \
  "$root/crates/i2pr-i2pcontrol/src" "$root/crates/i2pr-i2pcontrol/Cargo.toml" >/dev/null; then
  echo "no UI/frontend dependency may enter i2pr-i2pcontrol" >&2
  exit 1
fi

# Plan 287: the test-only TLS accept-any verifier (`dangerous()`) is
# confined to integration tests. Production daemon source must never
# bypass certificate verification.
if grep -REn '\.dangerous\(\)|DangerousClientConfig|with_custom_certificate_verifier' \
  "$root/crates/i2pr-daemon/src" >/dev/null; then
  echo "dangerous TLS verifiers are forbidden in production daemon source" >&2
  exit 1
fi

if grep -REn 'async[[:space:]]+fn|async_trait|i2pr-(netdb|tunnel|client)' \
  "$root/crates/i2pr-transport" "$root/crates/i2pr-transport-ntcp2" "$root/crates/i2pr-transport-ssu2" >/dev/null; then
  echo "transport contracts must remain synchronous and independent of routing clients" >&2
  exit 1
fi

if grep -En 'i2pr-daemon|i2pr-runtime|i2pr-testkit' \
  "$root/crates/i2pr-transport/Cargo.toml" "$root/crates/i2pr-transport-ntcp2/Cargo.toml" "$root/crates/i2pr-transport-ssu2/Cargo.toml" >/dev/null; then
  echo "transport crates must not depend on runtime, daemon, or testkit" >&2
  exit 1
fi

if grep -REn 'tokio::|TcpListener|TcpStream|UdpSocket|UnixListener|UnixStream|tokio::net|tokio::spawn|tokio::time|tokio::sync' \
  "$root/crates/i2pr-service-tunnels/src" >/dev/null; then
  echo "i2pr-service-tunnels must remain runtime-neutral: no Tokio, sockets, listeners, tasks, or timers" >&2
  exit 1
fi

# Plan 345: application protocol is a data/policy contract, never an OS or
# runtime owner. std::net address *values* are permitted; socket/DNS APIs are not.
#
# Plan 362 closes two coverage gaps in this script, both verified on 2026-10-06:
#
#   1. `i2pr-api` had NO section at all here, so a "passed" result was not
#      evidence for the crate that owns the SAM 3.1 + I2CP wire/state.
#   2. Every scan above greps for *contiguous* module-path strings, so a
#      grouped `use std::{fs, net};` writes the path with a brace, breaks the
#      string, and evades the scan entirely.
#
# Gap 2 is closed by option (a) of the plan: NORMALISE BEFORE SCANNING. The
# alternative -- adding brace-shaped alternatives such as `std::\{[^}]*\bnet\b`
# to each pattern -- is explicitly rejected: it cannot see `std::{env, process}`
# or a nested group such as `std::{fs::{self, File}, net}`, and it would be a
# one-line diff that looks like a fix while leaving the probe passing.
#
# Everything below only ADDS detection. No existing pattern, path, or assertion
# is relaxed, narrowed, or exempted, and the pre-existing raw scans above are
# left byte-for-byte untouched -- the normalised scan runs *in addition*, so
# nothing the raw scan used to catch can be lost.
python3 - "$root" "${BASH_SOURCE[0]}" <<'PY'
from pathlib import Path
import re
import sys

root = Path(sys.argv[1])
script_path = Path(sys.argv[2])

# --- brace normalisation ----------------------------------------------------
#
# `normalise()` blanks comment bodies and literal contents (preserving line
# counts and therefore line numbers) and then rewrites every `use` statement
# into its flat leaves, so `use std::{fs, net};` yields exactly the tokens
# `use std::fs; use std::net;`.
#
# It FAILS CLOSED: an unbalanced brace group, a `use` with no terminating `;`,
# an unterminated string/char literal, or any other malformed shape raises
# ParseError and aborts the script. A scan this parser cannot read stops the
# build; it is never skipped.

class ParseError(Exception):
    pass


IDENT = re.compile(r"(?:r#)?[A-Za-z_][A-Za-z0-9_]*")
USE_TOKEN = re.compile(r"(?<![A-Za-z0-9_#])use(?![A-Za-z0-9_])")
VISIBILITY = re.compile(r"(?<![A-Za-z0-9_])pub(?:\([^()]*\))?\s*$")
AS_KW = re.compile(r"as(?![A-Za-z0-9_])")
RAW_PREFIX = re.compile(r"(?:b|c)?(?:br|cr|r)$")


def _prefix_before(text, i):
    k = i
    while k > 0 and (text[k - 1].isalnum() or text[k - 1] in "_#"):
        k -= 1
    return text[k:i]


def _mask_string(text, out, i):
    n = len(text)
    prefix = _prefix_before(text, i)
    hashes = len(prefix) - len(prefix.rstrip("#"))
    ident = prefix[: len(prefix) - hashes]
    if "r" in ident and RAW_PREFIX.match(ident):
        closer = '"' + "#" * hashes
        end = text.find(closer, i + 1)
        if end < 0:
            raise ParseError("unterminated raw string literal")
        j = end + len(closer)
    else:
        # rustc keeps raw newlines inside a string literal and treats a trailing
        # backslash as a line continuation, so scan for the closer instead of
        # stopping at the end of the line.
        j, closed = i + 1, False
        while j < n:
            if text[j] == "\\":
                j += 2
                continue
            if text[j] == '"':
                j += 1
                closed = True
                break
            j += 1
        if not closed:
            raise ParseError("unterminated string literal")
    for k in range(i, j):
        if out[k] != "\n":
            out[k] = " "
    return j


def _mask_quote(text, out, i):
    n = len(text)
    if i + 1 >= n:
        raise ParseError("unterminated quote")
    nxt = text[i + 1]
    if nxt == "'":
        out[i] = out[i + 1] = " "
        return i + 2
    if nxt == "\\":
        j = i + 1
        while j < n:
            if text[j] == "\\":
                j += 2
                continue
            if text[j] == "'":
                j += 1
                break
            j += 1
        else:
            raise ParseError("unterminated character literal")
        for k in range(i, j):
            out[k] = " "
        return j
    if i + 2 < n and text[i + 2] == "'":
        out[i] = out[i + 1] = out[i + 2] = " "
        return i + 3
    if not IDENT.match(text, i + 1):
        raise ParseError("unterminated quote")
    out[i] = " "  # a lifetime such as 'static -- keep the identifier
    return i + 1


def mask(text):
    out = list(text)
    i, n = 0, len(text)
    while i < n:
        ch = text[i]
        if ch == "/" and text.startswith("//", i):
            j = text.find("\n", i)
            j = n if j < 0 else j
            for k in range(i, j):
                out[k] = " "
            i = j
        elif ch == "/" and text.startswith("/*", i):
            depth, j = 1, i + 2
            while j < n and depth:
                if text.startswith("/*", j):
                    depth += 1
                    j += 2
                elif text.startswith("*/", j):
                    depth -= 1
                    j += 2
                else:
                    j += 1
            if depth:
                raise ParseError("unterminated block comment")
            for k in range(i, j):
                if out[k] != "\n":
                    out[k] = " "
            i = j
        elif ch == '"':
            i = _mask_string(text, out, i)
        elif ch == "'":
            i = _mask_quote(text, out, i)
        else:
            i += 1
    return "".join(out)


def _skip_ws(s, i):
    while i < len(s) and s[i] in " \t\r\n":
        i += 1
    return i


def _parse_tree(s, i, prefix, out):
    if s.startswith("::", i):
        i = _skip_ws(s, i + 2)
        prefix = "::"
    while True:
        if i < len(s) and s[i] == "*":
            out.append(f"{prefix}::*" if prefix else "*")
            return _skip_ws(s, i + 1)
        m = IDENT.match(s, i)
        if not m:
            raise ParseError(f"expected a path segment at offset {i}: {s[i:i + 24]!r}")
        prefix = m.group(0) if not prefix else (
            "::" + m.group(0) if prefix == "::" else f"{prefix}::{m.group(0)}"
        )
        i = _skip_ws(s, m.end())
        if AS_KW.match(s, i):
            i = _skip_ws(s, i + 2)
            am = IDENT.match(s, i)
            if not am:
                raise ParseError("expected an alias name after `as`")
            i = _skip_ws(s, am.end())
        if i < len(s) and s[i] == "{":
            return _expand_group(s, i, prefix, out)
        if s.startswith("::", i):
            i = _skip_ws(s, i + 2)
            if i < len(s) and s[i] == "{":
                return _expand_group(s, i, prefix, out)
            continue
        if prefix.endswith("::self"):
            base = prefix[: -len("::self")]
            if base:
                out.append(base)
        else:
            out.append(prefix)
        return i


def _expand_group(s, i, prefix, out):
    i = _skip_ws(s, i + 1)
    while True:
        if i >= len(s):
            raise ParseError("unterminated `use` group")
        if s[i] == "}":
            return i + 1
        i = _skip_ws(s, _parse_tree(s, i, prefix, out))
        if i < len(s) and s[i] == ",":
            i = _skip_ws(s, i + 1)
            continue
        if i < len(s) and s[i] == "}":
            return i + 1
        raise ParseError(f"expected `,` or `}}` in `use` group at offset {i}")


def expand_uses(masked):
    """Rewrite each `use` statement as flat leaves, preserving line numbers."""
    pieces, i, n = [], 0, len(masked)
    while i < n:
        m = USE_TOKEN.search(masked, i)
        if not m:
            pieces.append(masked[i:])
            break
        vis = VISIBILITY.search(masked[:m.start()])
        visibility = vis.group(0) if vis else ""
        span_start = vis.start() if vis else m.start()
        pieces.append(masked[i:span_start])
        j, depth = m.end(), 0
        while j < n:
            c = masked[j]
            if c == "{":
                depth += 1
            elif c == "}":
                depth -= 1
                if depth < 0:
                    raise ParseError("unbalanced `}` outside a group")
            elif c == ";" and depth == 0:
                break
            j += 1
        else:
            raise ParseError(f"`use` starting at offset {m.start()} has no `;`")
        body, leaves = masked[m.end():j], []
        pos = _skip_ws(body, 0)
        while True:
            pos = _skip_ws(body, _parse_tree(body, pos, "", leaves))
            if pos >= len(body):
                break
            if body[pos] == ",":
                pos = _skip_ws(body, pos + 1)
                if pos >= len(body):
                    break
                continue
            raise ParseError(f"unexpected {body[pos]!r} in `use` list")
        pieces.append(" ".join(f"{visibility}use {leaf};" for leaf in leaves))
        pieces.append("\n" * masked[span_start:j].count("\n"))
        i = j + 1
    return "".join(pieces)


def normalise(text):
    return expand_uses(mask(text))


def scan(source, patterns):
    """Return [(relative path, line number, line)] for each forbidden hit."""
    hits, seen = [], 0
    for path in sorted(source.rglob("*.rs")):
        seen += 1
        for number, line in enumerate(normalise(path.read_text(encoding="utf-8")).split("\n"), 1):
            for category, pattern in patterns.items():
                if re.search(pattern, line):
                    hits.append((path.relative_to(root), number, line.strip(), category))
    # A scan that read no source is a scan that cannot detect anything: fail
    # closed rather than reporting a vacuous pass.
    if seen == 0:
        raise SystemExit(f"boundary scan found no Rust source under {source}")
    return hits


# --- rule sets --------------------------------------------------------------
#
# `i2pr-app-proto` (Plan 345), `i2pr-i2pcontrol` (Plan 286) and
# `i2pr-service-tunnels` keep exactly the patterns their shell rules above use.
# `i2pr-api` is new in Plan 362 and mirrors the sibling runtime-neutral crates.
source = root / "crates/i2pr-app-proto/src"
patterns = {
    "tokio": r"tokio::|async\s+fn|async_trait",
    "sockets": r"TcpStream|TcpListener|UdpSocket|UnixStream|UnixListener|TcpSocket",
    "process": r"std::process|Command::new|\.spawn\s*\(",
    "filesystem": r"std::fs|OpenOptions|File::open|File::create",
    "resolver": r"ToSocketAddrs|to_socket_addrs|lookup_host|\b(dns|resolve_hostname)\s*\(",
    "dynamic loader": r"libloading|dlopen\s*\(|LoadLibrary",
    "sandbox backend": r"seccomp|landlock|AppContainer|Seatbelt|NetworkNamespace",
}

# Plan 362: `i2pr-api` is the SAM 3.1 + I2CP wire/state crate. AGARDS place it
# in the same runtime-neutral class as the transport contracts, so it is held
# to the same rule list. `std::net` address *values* stay permitted here, exactly
# as Plan 345 permits them for i2pr-app-proto and as crates/i2pr-api/src/sam/
# forward.rs already relies on; the rule keys on socket types, not the module.
i2pr_api_patterns = {
    "tokio": r"tokio::",
    "async": r"async\s+fn|async_trait",
    "sockets": r"TcpStream|TcpListener|UdpSocket|UnixStream|UnixListener|TcpSocket",
    "filesystem": r"std::fs|OpenOptions|File::",
    "unbounded channels": r"unbounded_channel|unbounded::<|UnboundedSender|UnboundedReceiver",
    "raw JoinHandle": r"JoinHandle",
    "ownerless spawn": r"spawn\s*\(",
}

# The grouped-import alternations already spelled out in the shell rules above.
i2pcontrol_patterns = {
    "runtime ownership": (
        r"tokio::|std::net|std::fs|TcpStream|TcpListener|UdpSocket|UnixStream|"
        r"UnixListener|OpenOptions|File::|tokio::net|tokio::spawn|tokio::time|tokio::sync"
    ),
}
service_tunnel_patterns = {
    "runtime ownership": (
        r"tokio::|TcpListener|TcpStream|UdpSocket|UnixListener|UnixStream|"
        r"tokio::net|tokio::spawn|tokio::time|tokio::sync"
    ),
}

# --- positive controls ------------------------------------------------------
#
# Every forbidden category must be detectable, proving these checks stay live
# if the scan is edited later. Plan 362 extends the Plan 345 control with the
# grouped-import forms that used to evade the scan -- including the nested and
# multi-line groups a brace-shaped regex alternative could not survive.
CONTROL = [
    ("app-proto/flat", patterns, "tokio::spawn(async move {}); TcpStream::connect(addr);\n"
     "std::process::Command::new(\"x\").spawn(); std::fs::read(\"x\");\n"
     "name.to_socket_addrs(); libloading::Library::new(\"x\"); seccomp::apply();\n"),
    ("app-proto/grouped tokio", patterns, "use tokio::{net::TcpListener, spawn};\n"),
    ("app-proto/grouped fs", patterns, "use std::{fs, net};\nfn f() { std::fs::read(\"x\"); }\n"),
    ("i2pr-api/grouped tokio", i2pr_api_patterns, "use tokio::{net::TcpListener, spawn};\n"),
    ("i2pr-api/grouped fs+net", i2pr_api_patterns,
     "use std::{fs, net};\nfn f() { fs::read_to_string(\"x\").unwrap_or_default(); }\n"),
    ("i2pr-api/nested group", i2pr_api_patterns,
     "use std::{fs::{self, File}, net};\n"),
    ("i2pr-api/multi-line group", i2pr_api_patterns, "use std::{\n    fs,\n    net,\n};\n"),
    ("i2pr-api/glob group", i2pr_api_patterns, "use std::{io::{self, Write}, fs};\n"),
    ("i2pr-api/async fn", i2pr_api_patterns, "pub async fn f() {}\n"),
    ("i2pr-api/unbounded", i2pr_api_patterns, "fn f() { let _ = unbounded_channel::<u8>(); }\n"),
    ("i2pr-api/JoinHandle", i2pr_api_patterns, "fn f() -> JoinHandle<()> { todo!() }\n"),
    ("i2pr-api/spawn", i2pr_api_patterns, "fn f() { spawn(async {}); }\n"),
    ("i2pcontrol/grouped std::net", i2pcontrol_patterns, "use std::{net};\n"),
    ("service-tunnels/grouped", service_tunnel_patterns,
     "use tokio::{net::{TcpListener, TcpStream}};\n"),
]
if len(CONTROL) < 14:
    raise SystemExit(f"boundary checker positive controls were removed: {len(CONTROL)} < 14")
_control_checked = 0
for label, rule_set, probe in CONTROL:
    _control_checked += 1
    if not [c for c, p in rule_set.items() if re.search(p, normalise(probe))]:
        raise SystemExit(f"boundary checker positive control missed: {label}")
if _control_checked != len(CONTROL):
    raise SystemExit("not every boundary checker positive control was exercised")

# A negative control: the normaliser must key on the *import path*, not on a
# leaf name. A crate-local module called `net` and a plain address value are
# both legitimate and must stay unflagged by the i2pr-api rule set.
benign = "use crate::wire::{fs, net};\nuse std::net::IpAddr;\nfn f() -> IpAddr { IpAddr::V4(Ipv4Addr::LOCALHOST) }\n"
if [c for c, p in i2pr_api_patterns.items() if re.search(p, normalise(benign))]:
    raise SystemExit("boundary checker negative control flagged legitimate source")

# The scan must also fail closed rather than skip input it cannot parse.
for broken in ("use std::{fs, net\n", "use std::{fs, net}};\n", 'let s = "unterminated;\n'):
    try:
        normalise(broken)
    except ParseError:
        continue
    raise SystemExit(f"boundary checker failed to fail closed on: {broken!r}")

# --- scans ------------------------------------------------------------------

def violations(text):
    return [name for name, pattern in patterns.items() if re.search(pattern, text)]


bad = []
contract_sources = []
for path in source.rglob("*.rs"):
    text = path.read_text(encoding="utf-8")
    contract_sources.append(text)
    for category in violations(normalise(text)):
        bad.append(f"{path.relative_to(root)}: forbidden {category} API")
if bad:
    raise SystemExit("i2pr-app-proto runtime/OS boundary violation:\n" + "\n".join(bad))

contract_text = "\n".join(contract_sources)
directional_contract = (
    "pub enum AppToHostMessage",
    "pub enum HostToAppMessage",
    "pub enum AdminToHostMessage",
    "pub enum HostToAdminMessage",
    "pub fn decode_app_to_host_control",
    "pub fn decode_host_to_app_control",
    "pub fn decode_admin_to_host_control",
    "pub fn decode_host_to_admin_control",
    "pub struct RequestId",
    "pub enum RequestErrorCode",
)
missing_contract = [item for item in directional_contract if item not in contract_text]
if missing_contract:
    raise SystemExit(
        "i2pr-app-proto directional contract surface missing: "
        + ", ".join(missing_contract)
    )
if re.search(r"pub enum AppService\s*\{[^}]*BrokeredTcp", contract_text, re.S):
    raise SystemExit("i2pr-app-proto must not expose brokered_tcp as an openable service")

for contract_label in ("i2pr-app-proto", "i2pr-app-manager-proto"):
    manifest_text = (
        root / f"crates/{contract_label}/Cargo.toml"
    ).read_text(encoding="utf-8")
    if re.search(
        r"^(tokio|tokio-util|rustix|libloading|nix|windows|objc)\s*=",
        manifest_text,
        re.M,
    ):
        raise SystemExit(
            f"{contract_label} must not depend on runtime/OS backend crates"
        )

# Plan 368: the manager contract must stay a two-crate contract and must not name
# an administrator role or the Proposal 170 crate.
manager_manifest = (
    root / "crates/i2pr-app-manager-proto/Cargo.toml"
).read_text(encoding="utf-8")
if "i2pr-i2pcontrol" in manager_manifest:
    raise SystemExit(
        "i2pr-app-manager-proto must not depend on the Proposal 170 crate"
    )

# Plan 368: `i2pr-app-manager-proto` is the private trusted AppManager contract.
# It is runtime-neutral in exactly the Plan 345 class, so it reuses the same rule
# set rather than a narrower one that could be evaded.
for label, target, rule_set in (
    ("i2pr-app-manager-proto", root / "crates/i2pr-app-manager-proto/src", patterns),
    ("i2pr-i2pcontrol", root / "crates/i2pr-i2pcontrol/src", i2pcontrol_patterns),
    ("i2pr-service-tunnels", root / "crates/i2pr-service-tunnels/src", service_tunnel_patterns),
    ("i2pr-api", root / "crates/i2pr-api/src", i2pr_api_patterns),
):
    found = scan(target, rule_set)
    if found:
        detail = "\n".join(f"{p}:{n}: forbidden {c} API: {line[:100]}" for p, n, line, c in found)
        raise SystemExit(f"{label} runtime-neutrality violation:\n{detail}")

# Plan 362: i2pr-api is a wire/state crate; the daemon owns its listeners.
manifest = (root / "crates/i2pr-api/Cargo.toml").read_text(encoding="utf-8")
if re.search(
    r"i2pr-(daemon|runtime|testkit|console|service-tunnels)", manifest
):
    raise SystemExit("i2pr-api must not depend on daemon/runtime/service owners")

# Guard against silent weakening: the pre-existing raw scans above must never
# disappear. Plan 362 only appends to them. The anchor literals are checked in
# addition to the count so that lowering the count floor cannot disable this.
raw_scans = len(re.findall(
    r"^if grep|^for manifest in", script_path.read_text(encoding="utf-8"), re.M
))
if raw_scans < 17:
    raise SystemExit(f"pre-existing raw boundary scans were removed: {raw_scans} < 17")
script_text = script_path.read_text(encoding="utf-8")


def shell_only(text):
    """The shell body, with any `<<'PY'` heredoc body blanked out.

    The anchors below are checked against this rather than the whole file, so
    that the anchor list cannot satisfy itself.
    """
    kept, inside = [], False
    for line in text.split("\n"):
        if line.rstrip().endswith("<<'PY'"):
            inside = True
            kept.append(line)
            continue
        if inside:
            if line.rstrip() == "PY":
                inside = False
                kept.append(line)
            else:
                kept.append("")
            continue
        kept.append(line)
    return "\n".join(kept)


shell = shell_only(script_text)
for anchor in (
    "unbounded_channel|unbounded::<|UnboundedSender|UnboundedReceiver",
    "std::thread::sleep|thread::sleep|std::mem::forget|mem::forget",
    "tokio::spawn\\(",
    "JoinHandle",
    "^(tokio|tokio-util)[[:space:]]*=",
    "production crate depends on i2pr-testkit",
    "raw JoinHandle ownership requires a reviewed owner-specific implementation",
):
    if anchor not in shell:
        raise SystemExit(f"pre-existing raw boundary scan was removed: {anchor!r}")

print(
    "i2pr-app-proto runtime/OS boundary: ok (positive control passed)\n"
    "i2pr-api runtime-neutrality: ok (8 rules, grouped-import positive controls passed)"
)
PY

# std::net::IpAddr/SocketAddr values are allowed as validated data, but
# listener/stream ownership is forbidden in the runtime-neutral crate.
if grep -REn 'TcpListener|TcpStream|UdpSocket|UnixListener|UnixStream' \
  "$root/crates/i2pr-service-tunnels/src" >/dev/null; then
  echo "i2pr-service-tunnels must not own listeners or streams; daemon owns sockets" >&2
  exit 1
fi

if grep -En 'i2pr-transport|i2pr-tunnel|i2pr-runtime|i2pr-daemon|i2pr-testkit' \
  "$root/crates/i2pr-service-tunnels/Cargo.toml" >/dev/null; then
  echo "i2pr-service-tunnels must not depend on transport/tunnel internals, runtime, daemon, or testkit" >&2
  exit 1
fi

if grep -REn "unbounded_channel|unbounded::<|UnboundedSender|UnboundedReceiver" \
  "$root/crates/i2pr-service-tunnels/src" >/dev/null; then
  echo "unbounded asynchronous channels are forbidden in service-tunnels source" >&2
  exit 1
fi

echo "runtime boundary checks passed"
