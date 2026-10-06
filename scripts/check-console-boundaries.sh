#!/usr/bin/env bash
# Router-console boundary guard (Plan 356).
#
# This script enforces the ownership rule that keeps the console substrate
# decision checkable: the console crate owns the browser application and
# nothing else, and the daemon owns the socket through EggServe. It is a
# *strengthening* guard. Fix the code, never weaken this script.
#
# Rules enforced:
#   1. `i2pr-console` depends on no workspace crate and no HTTP substrate.
#   2. `i2pr-console` source names no socket, listener, Tokio, filesystem, or
#      task primitive in *any* file, and no `axum::serve` path.
#   3. The daemon routes the console through the EggServe server adapter.
#   4. No shared "eepsite" or equivalent asset tree exists in the tree.
#   5. Compiled assets reference no remote origin.
#   6. The workspace contains exactly one HTTP server substrate (EggServe).
#   7. The dependency-direction expected map actually covers every member,
#      closing the known 18-of-20 coverage gap for the new crate.
#
# Usage:
#   bash scripts/check-console-boundaries.sh              # full check (default)
#   bash scripts/check-console-boundaries.sh --self-test  # probes only
#   bash scripts/check-console-boundaries.sh --trace      # full + per-file trace

set -euo pipefail

cd "$(dirname "$0")/.."

case "${1:-}" in
    "") console_mode="scan" ;;
    --self-test) console_mode="self-test" ;;
    --trace) console_mode="trace" ;;
    -h|--help)
        printf 'usage: %s [--self-test|--trace]\n' "$0"
        exit 0
        ;;
    *)
        printf 'check-console-boundaries: unknown argument: %s\n' "$1" >&2
        exit 2
        ;;
esac

CONSOLE_CRATE="crates/i2pr-console"
CONSOLE_MANIFEST="${CONSOLE_CRATE}/Cargo.toml"
DAEMON_CONSOLE="crates/i2pr-daemon/src/console.rs"
CONSOLE_SRC="${CONSOLE_CRATE}/src"
failures=0

fail() {
    printf 'check-console-boundaries: FAIL: %s\n' "$1" >&2
    failures=$((failures + 1))
}

# grep -rl over a path list, tolerating a path with no files.
files_containing() {
    # $1 = pattern, rest = paths
    pattern="$1"
    shift
    if [ -d "$1" ]; then
        grep -rlE "$pattern" "$@" 2>/dev/null || true
    else
        grep -lE "$pattern" "$@" 2>/dev/null || true
    fi
}

# Prints the code of the given files with comment lines removed.
#
# The forbidden-name rules must judge executable code, not prose: this file
# has to *name* the APIs it forbids, and so does the module documentation
# that explains why they are absent.
code_only() {
    cat "$@" 2>/dev/null | grep -vE '^[[:space:]]*(//|/\*|\*|#)' || true
}

# Returns 0 when a forbidden pattern appears in executable code.
code_contains() {
    # $1 = pattern, rest = paths
    pattern="$1"
    shift
    code_only "$@" | grep -qE "${pattern}"
}

# --- console_source_scan: rule 2's scanner and its self-test ----------------
#
# ONE implementation, driven two ways:
#
#   console_source_scan scan   <dir>   violations on stdout, per-file trace on
#                                      stderr with --trace
#   console_source_scan self-test      probes only; no cargo, no real tree
#
# The self-test does NOT re-implement the rule (that is how a guard rots into
# a comment). It builds fixture trees and calls `rule_violations` /
# `scan_tree` directly, exactly as check-m12-floodfill-boundaries.sh does.
console_source_scan() {
    python3 - "${1:-scan}" "${2:-${CONSOLE_SRC}}" <<'PY'
import re
import shutil
import sys
import tempfile
from pathlib import Path

MODE = sys.argv[1]
ROOT = sys.argv[2]


class ParseError(Exception):
    """Fail closed: never skip input the scanner cannot read."""


class ScanError(Exception):
    """A scan that cannot be trusted is a failure, not a pass."""


# --------------------------------------------------------------------------
# normaliser (Plan 362, kept verbatim)
#
# Blanks comment bodies and literal contents (preserving line counts and
# therefore line numbers) and rewrites each `use` statement into its flat
# leaves, so `use std::{fs, net};` yields exactly `use std::fs; use std::net;`.
# --------------------------------------------------------------------------

IDENT = re.compile(r"(?:r#)?[A-Za-z_][A-Za-z0-9_]*")
USE_TOKEN = re.compile(r"(?<![A-Za-z0-9_#])use(?![A-Za-z0-9_])")
VISIBILITY = re.compile(r"(?<![A-Za-z0-9_])pub(?:\([^()]*\))?\s*$")
AS_KW = re.compile(r"(?<![A-Za-z0-9_])as(?![A-Za-z0-9_])")
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
    out[i] = " "
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


# --------------------------------------------------------------------------
# rule 2, as data
# --------------------------------------------------------------------------

CFG_TEST = re.compile(r"#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]")

# Forbidden everywhere, in every file, in production code. Socket-keyed, not
# module-keyed: these are the things that can own or move a byte off the
# machine, so no allow-set entry can ever name one of them.
FORBIDDEN_TOKENS = (
    (r"\bTcpListener\b", "TcpListener"),
    (r"\bTcpStream\b", "TcpStream"),
    (r"\bUdpSocket\b", "UdpSocket"),
    (r"\bUnixListener\b", "UnixListener"),
    (r"\bUnixStream\b", "UnixStream"),
    (r"\bServer::bind\b", "Server::bind"),
    (r"\baxum::serve\b", "axum::serve"),
    (r"\bspawn\s*\(", "spawn("),
    (r"\bstd::fs\b", "std::fs"),
    (r"\bfs::", "fs::"),
    (r"\btokio::", "tokio::"),
)

# `std::net` itself is NOT blanket-banned and NOT blanket-allowed. AGENTS.md
# requires console requests to "match an exact `Host` authority including the
# port", which is impossible without an address value, so the four address
# types below are permitted by name. Everything else reached through
# `std::net` -- TcpStream, ToSocketAddrs, a bare `use std::net;`, a glob --
# stays forbidden. This is Plan 345's precedent, for i2pr-app-proto, applied
# to i2pr-console.
NET_MODULE = re.compile(r"\bstd::net\b")
ALLOWED_NET_TYPES = ("IpAddr", "SocketAddr", "Ipv4Addr", "Ipv6Addr")
ALLOWED_NET_LEAF = re.compile(r"::(?:" + "|".join(ALLOWED_NET_TYPES) + r")\b")


def rule_violations(text):
    """Forbidden constructs in `text`, which must already be normalised."""
    found = []
    for pattern, label in FORBIDDEN_TOKENS:
        if re.search(pattern, text):
            found.append(label)
    for m in NET_MODULE.finditer(text):
        if not ALLOWED_NET_LEAF.match(text, m.end()):
            found.append("std::net (not a permitted address value)")
    return found


def test_regions(lines):
    """Per-line flag: is this line inside a `#[cfg(test)]` region?

    Scoped, not latched. A test region ends where the item it annotated ends
    -- at the matching close brace, or at the `;` of a brace-less annotation.
    The old awk set `in_tests = 1` on the first `#[cfg(test)]` and never
    cleared it, which is the whole defect this scanner exists to close.
    """
    suppressed = [False] * len(lines)
    i = 0
    while i < len(lines):
        if not CFG_TEST.search(lines[i]):
            i += 1
            continue
        j = i
        while j < len(lines):
            stripped = lines[j].strip()
            if stripped and not stripped.startswith("#["):
                break
            j += 1
        if j >= len(lines):
            for k in range(i, len(lines)):
                suppressed[k] = True
            break
        if "{" in lines[j]:
            # The attribute lines between the annotation and the item belong to
            # the test item too; the brace walk itself starts at the item.
            for k in range(i, j):
                suppressed[k] = True
            depth, opened, k = 0, False, j
            while k < len(lines):
                if lines[k].count("{"):
                    opened = True
                depth += lines[k].count("{") - lines[k].count("}")
                suppressed[k] = True
                if opened and depth <= 0:
                    break
                k += 1
            i = k + 1
        else:
            for k in range(i, j + 1):
                suppressed[k] = True
            i = j + 1
    return suppressed


def scan_file(path):
    """Returns (hits, stats) for one console source file."""
    try:
        text = path.read_text(encoding="utf-8")
    except OSError as exc:
        raise ScanError(f"unreadable console source file {path}: {exc}") from exc
    try:
        norm = normalise(text)
    except ParseError as exc:
        raise ScanError(f"unparseable console source file {path}: {exc}") from exc
    if norm.count("\n") != text.count("\n"):
        raise ScanError(f"normaliser changed line count for {path}")
    lines = norm.split("\n")
    suppressed = test_regions(lines)
    hits, scanned = [], 0
    for idx, line in enumerate(lines):
        if suppressed[idx]:
            continue
        scanned += 1
        for label in rule_violations(line):
            hits.append((str(path), idx + 1, label, line.strip()))
    stats = {
        "lines": len(lines),
        "regions": sum(1 for line in lines if CFG_TEST.search(line)),
        "scanned": scanned,
        "suppressed": len(lines) - scanned,
    }
    return hits, stats


def scan_tree(root, trace=False, stream=None):
    files = sorted(p for p in Path(root).rglob("*.rs") if p.is_file())
    if not files:
        raise ScanError(
            f"console source scan visited zero files under {root}: "
            "refusing to report success"
        )
    all_hits, total = [], {"lines": 0, "scanned": 0, "suppressed": 0}
    for path in files:
        hits, stats = scan_file(path)
        all_hits.extend(hits)
        for key in total:
            total[key] += stats[key]
        if trace:
            print(
                f"FILE {path}\n"
                f"  lines={stats['lines']} cfg_test_regions={stats['regions']} "
                f"production_lines_scanned={stats['scanned']} "
                f"suppressed_lines={stats['suppressed']} hits={len(hits)}",
                file=stream or sys.stderr,
            )
    if trace:
        print(
            f"TRACE files={len(files)} lines={total['lines']} "
            f"production_lines_scanned={total['scanned']} "
            f"suppressed_lines={total['suppressed']} hits={len(all_hits)}",
            file=stream or sys.stderr,
        )
    return all_hits


# --------------------------------------------------------------------------
# self-test: fixtures drive the real scanner
# --------------------------------------------------------------------------

# The console source layout, mirrored. `theme.rs` deliberately carries a
# `#[cfg(test)]` early so the fixture reproduces the historical ordering that
# hid the other files.
FIXTURE_FILES = [
    "theme.rs",
    "html.rs",
    "color.rs",
    "assets.rs",
    "control.rs",
    "lib.rs",
    "secret.rs",
    "routes.rs",
    "security/auth.rs",
    "security/mod.rs",
    "security/authority.rs",
    "security/headers.rs",
    "security/session.rs",
]

CLEAN_BODY = """//! Fixture module.
pub struct Value(pub u8);

impl Value {
    pub fn get(&self) -> u8 {
        self.0
    }
}
"""

THEME_BODY = """//! Fixture module with an early test region.
pub struct Palette;

impl Palette {
    pub fn accent(&self) -> &'static str {
        "slate"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn helper() -> u8 {
        7
    }

    #[test]
    fn accent_is_slate() {
        assert_eq!(Palette.accent(), "slate");
    }
}
"""

AUTHORITY_CLEAN = """//! Fixture module that legitimately needs an address value.
use std::net::{IpAddr, SocketAddr};

pub fn is_loopback(peer: &SocketAddr) -> bool {
    match peer.ip() {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}
"""


def build_fixture(root):
    root = Path(root)
    for rel in FIXTURE_FILES:
        path = root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        if rel == "theme.rs":
            path.write_text(THEME_BODY, encoding="utf-8")
        elif rel == "security/authority.rs":
            path.write_text(AUTHORITY_CLEAN, encoding="utf-8")
        else:
            path.write_text(CLEAN_BODY, encoding="utf-8")
    return root


def self_test():
    failures = []
    tmp = tempfile.mkdtemp(prefix="i2pr-console-selftest.")
    try:
        # 1. the fixture itself is clean: the rule is satisfiable by correct
        #    console code, including the permitted address-value import.
        clean = build_fixture(Path(tmp) / "clean")
        clean_hits = scan_tree(clean)
        if clean_hits:
            failures.append(
                "clean fixture produced violations: "
                + "; ".join(f"{p}:{n}: {lab}" for p, n, lab, _ in clean_hits)
            )

        # 2. coverage: EVERY file rejects a violation. These are the files the
        #    pre-Plan-366 awk never examined, which is exactly why probing only
        #    theme.rs would have reproduced the false confidence this fixes.
        categories = {
            "TcpListener": 'use std::net::TcpListener;\nfn f() { TcpListener::bind("127.0.0.1:0").ok(); }\n',
            "TcpStream": 'use std::net::TcpStream;\nfn f() { let _ = TcpStream::connect("127.0.0.1:1"); }\n',
            "UdpSocket": 'use std::net::UdpSocket;\nfn f() { UdpSocket::bind("127.0.0.1:0").ok(); }\n',
            "UnixListener": 'use std::net::UnixListener;\nfn f() { UnixListener::bind("/tmp/s").ok(); }\n',
            "UnixStream": 'use std::net::UnixStream;\nfn f() { UnixStream::connect("/tmp/s").ok(); }\n',
            "Server::bind": 'fn f() { Server::bind("127.0.0.1:0"); }\n',
            "axum::serve": 'fn f() { axum::serve(listener, app); }\n',
            "spawn(": 'fn f() { spawn(fut); }\n',
            "std::fs": 'fn f() { std::fs::read_to_string("x").ok(); }\n',
            "fs::": 'use std::fs;\nfn f() { fs::read_to_string("x").ok(); }\n',
            "tokio::": 'use tokio::spawn;\nfn f() { tokio::spawn(fut); }\n',
        }
        for rel in FIXTURE_FILES:
            for label, probe in categories.items():
                root = build_fixture(Path(tmp) / f"probe-{rel}-{label}")
                target = root / rel
                target.write_text(probe, encoding="utf-8")
                hits = scan_tree(root)
                if not hits:
                    failures.append(
                        f"{rel}: injecting {label} produced NO violation "
                        "(this file is invisible to the scan)"
                    )
                    continue
                if not any(str(h[0]).endswith(rel) for h in hits):
                    failures.append(
                        f"{rel}: injecting {label} was reported against "
                        f"{hits[0][0]} instead"
                    )

        # 3. positive control: the allow-set stays usable, so the rule cannot
        #    be satisfied by banning every address value.
        for label, probe in {
            "address values": "use std::net::{IpAddr, SocketAddr};\n",
            "IpAddr alone": "use std::net::IpAddr;\n",
            "Ipv4Addr form": "use std::net::Ipv4Addr;\n",
            "Ipv6Addr form": "use std::net::Ipv6Addr;\n",
            "address value in a grouped import": "use std::{fmt, net::{IpAddr, SocketAddr}};\n",
            "address value alongside another group": "use std::{cmp, net::Ipv6Addr};\n",
            "doc comment naming axum::serve": "//! no `axum::serve` path exists here.\n",
        }.items():
            root = build_fixture(Path(tmp) / f"allow-{label}")
            (root / "secret.rs").write_text(probe, encoding="utf-8")
            hits = scan_tree(root)
            if hits:
                failures.append(
                    f"positive control rejected ({label}): "
                    + "; ".join(f"{p}:{n}: {lab}" for p, n, lab, _ in hits)
                )

        # 4. the allow-set cannot be widened: `std::net` reached for anything
        #    other than an address value stays forbidden, including the DNS
        #    resolver, which no FORBIDDEN_TOKENS entry would otherwise catch.
        for label, probe in {
            "bare std::net": "use std::net;\n",
            "glob std::net": "use std::net::*;\n",
            "DNS resolver": "use std::net::ToSocketAddrs;\n",
            "lookup_host": 'fn f() { std::net::lookup_host("x"); }\n',
            "grouped net into a socket": "use std::{net::TcpStream};\n",
            "glob group reaching the bare module": "use std::{fmt, net::{self, IpAddr}};\n",
        }.items():
            root = build_fixture(Path(tmp) / f"widen-{label}")
            (root / "secret.rs").write_text(probe, encoding="utf-8")
            hits = scan_tree(root)
            if not hits:
                failures.append(
                    f"widening the allow-set accepted ({label}); std::net must "
                    "stay forbidden outside the four named address types"
                )

        # 5. a test region is still suppressed, and production code AFTER one
        #    is still scanned. The old awk could not express either half.
        root = build_fixture(Path(tmp) / "scope")
        (root / "html.rs").write_text(
            "#[cfg(test)]\nfn helper() { let _ = TcpListener::bind(\"127.0.0.1:0\"); }\n"
            "pub fn production() -> u8 { 1 }\n"
            "#[cfg(test)]\nfn helper2() -> u8 { 2 }\n",
            encoding="utf-8",
        )
        hits = scan_tree(root)
        if hits:
            failures.append(
                "test regions are no longer suppressed: "
                + "; ".join(f"{p}:{n}: {lab}" for p, n, lab, _ in hits)
            )
        (root / "color.rs").write_text(
            "#[cfg(test)]\nmod tests {\n    #[test]\n    fn a() { let _ = TcpListener::bind(\"127.0.0.1:0\"); }\n}\n"
            "pub fn after() -> u8 { let _ = UdpSocket::bind(\"127.0.0.1:0\"); 3 }\n",
            encoding="utf-8",
        )
        hits = scan_tree(root)
        if not hits or not any("color.rs" in h[0] for h in hits):
            failures.append(
                "production code after a closed test region was not scanned "
                "(per-file reset regressed)"
            )

        # 6. a scan that visits zero files is a failure, not a pass.
        empty = Path(tmp) / "empty"
        empty.mkdir()
        try:
            scan_tree(empty)
            failures.append("a zero-file scan reported success")
        except ScanError:
            pass

        # 7. fail closed on unparseable input.
        for broken in ("use std::{net\n", "use std::{net}};\n", 'let s = "unterminated;\n'):
            try:
                normalise(broken)
            except ParseError:
                continue
            failures.append(f"normaliser failed to fail closed on {broken!r}")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    for failure in failures:
        print(f"check-console-boundaries: FAIL: {failure}", file=sys.stderr)
    if failures:
        raise SystemExit(1)
    print("check-console-boundaries: self-test ok")


if MODE == "self-test":
    try:
        self_test()
    except (ScanError, ParseError) as exc:
        print(f"check-console-boundaries: FAIL: {exc}", file=sys.stderr)
        raise SystemExit(1)
elif MODE in ("scan", "trace"):
    try:
        hits = scan_tree(ROOT, trace=(MODE == "trace"))
    except (ScanError, ParseError) as exc:
        print(f"check-console-boundaries: FAIL: {exc}", file=sys.stderr)
        raise SystemExit(2)
    for path, number, label, line in hits:
        print(f"{path}:{number}: {label}: {line}")
    raise SystemExit(1 if hits else 0)
else:
    print(f"check-console-boundaries: unknown mode {MODE!r}", file=sys.stderr)
    raise SystemExit(2)
PY
}

# --- rule 1: console manifest purity ---------------------------------------

if [ ! -f "${CONSOLE_MANIFEST}" ]; then
    fail "missing ${CONSOLE_MANIFEST}"
fi

forbidden_console_deps="eggserve-server eggserve-core eggserve-primitives eggserve-h3 \
i2pr-runtime i2pr-client i2pr-netdb i2pr-i2pcontrol i2pr-daemon i2pr-core i2pr-crypto \
i2pr-proto i2pr-api i2pr-service-tunnels tokio tokio-util hyper hyper-util http \
reqwest reqwest-middleware ureq rustls native-tls"
for dep in ${forbidden_console_deps}; do
    if grep -qE "^[[:space:]]*${dep}[[:space:]]*(=|\.)" "${CONSOLE_MANIFEST}"; then
        fail "i2pr-console must not depend on ${dep}"
    fi
done

# The console is an application crate, so it needs `axum`, and nothing else
# from the substrate world.
if ! grep -qE '^axum[[:space:]]*=' "${CONSOLE_MANIFEST}"; then
    fail "i2pr-console must declare the axum application dependency"
fi

if [ "${console_mode}" = "self-test" ]; then
    console_source_scan self-test "${CONSOLE_SRC}" || exit 1
    exit 0
fi

# --- rule 2: console source purity ------------------------------------------

# Every console source file, production code only. Test regions are skipped, but
# the skip is scoped to the item it annotates -- never latched for the rest of
# the run. A zero-file scan fails rather than passing.
if [ -d "${CONSOLE_SRC}" ]; then
    console_scan_rc=0
    console_socket_hits=$(console_source_scan "${console_mode}" "${CONSOLE_SRC}") \
        || console_scan_rc=$?
    if [ "${console_scan_rc}" -eq 2 ]; then
        fail "i2pr-console source scan could not be trusted (see message above)"
    elif [ "${console_scan_rc}" -ne 0 ]; then
        fail "i2pr-console source must not own sockets, Tokio, or tasks: ${console_socket_hits}"
    fi
fi

# --- rule 3: the daemon owns the EggServe adapter --------------------------

if [ ! -f "${DAEMON_CONSOLE}" ]; then
    fail "missing ${DAEMON_CONSOLE}"
else
    if ! grep -q 'TowerToEggserve' "${DAEMON_CONSOLE}"; then
        fail "the daemon console service must use the EggServe tower adapter"
    fi
    if ! grep -q 'Server::builder' "${DAEMON_CONSOLE}"; then
        fail "the daemon console service must construct the EggServe server"
    fi
    if code_contains 'axum::serve' "${DAEMON_CONSOLE}"; then
        fail "the daemon console service must not use axum::serve"
    fi
fi

# No crate may bypass the substrate with a direct `axum::serve`.
bypass_hits=$(files_containing '^[[:space:]]*.*[^a-zA-Z_.]axum::serve\(' \
    crates/*/src/*.rs crates/*/src/**/*.rs tools/*/src/*.rs 2>/dev/null || true)
if [ -n "${bypass_hits}" ]; then
    fail "axum::serve is forbidden; EggServe owns the HTTP runtime: ${bypass_hits}"
fi

# --- rule 4: no shared external asset tree --------------------------------

eepsite_hits=$(find . -type d -name 'eepsite' -not -path './target/*' 2>/dev/null || true)
if [ -n "${eepsite_hits}" ]; then
    fail "the console must not vendor a shared eepsite tree: ${eepsite_hits}"
fi

# Every browser asset must come from the console crate's own asset directory.
if [ ! -d "${CONSOLE_CRATE}/assets" ]; then
    fail "missing ${CONSOLE_CRATE}/assets"
fi

# --- rule 5: compiled assets are self-contained ---------------------------

if [ -d "${CONSOLE_CRATE}/assets" ]; then
    # XML namespace URIs are identifiers, not fetches: an inline SVG must
    # declare `xmlns="http://www.w3.org/2000/svg"`. Everything else that
    # names an http(s) origin is a remote reference.
    remote_hits=$(grep -rlE 'https?://' "${CONSOLE_CRATE}/assets" 2>/dev/null \
        | xargs grep -lE 'https?://' 2>/dev/null || true)
    remote_hits=$(for asset in ${remote_hits}; do
        if grep -vE 'xmlns(:[a-zA-Z]+)?[[:space:]]*=' "${asset}" | grep -qE 'https?://'; then
            printf '%s\n' "${asset}"
        fi
    done)
    if [ -n "${remote_hits}" ]; then
        fail "console assets must not reference a remote origin: ${remote_hits}"
    fi
    inline_hits=$(grep -rlE '<script|onload=|onclick=' "${CONSOLE_CRATE}/assets" 2>/dev/null || true)
    if [ -n "${inline_hits}" ]; then
        fail "console assets must not carry inline code: ${inline_hits}"
    fi
fi

# --- rule 6: exactly one HTTP server substrate ----------------------------

for rival in actix-web warp rocket poem tide salvo hyper hyper-util; do
    rival_hits=$(files_containing "^[[:space:]]*${rival}[[:space:]]*(=|\\.)" \
        crates/*/Cargo.toml tools/*/Cargo.toml 2>/dev/null || true)
    if [ -n "${rival_hits}" ]; then
        fail "a second HTTP substrate is forbidden; found ${rival} in ${rival_hits}"
    fi
done

# --- rule 6b: Plan 358 local-principal ownership ---------------------------

DISPATCH_FILE="crates/i2pr-daemon/src/i2pcontrol_dispatch.rs"
if [ ! -f "${DISPATCH_FILE}" ]; then
    fail "missing ${DISPATCH_FILE}: the listener-independent control dispatcher"
fi
if ! grep -q 'LocalConsolePrincipal' "${DAEMON_CONSOLE}"; then
    fail "the daemon console client must hold the local console principal"
fi
if ! grep -q 'LocalConsolePrincipal' "${DISPATCH_FILE}"; then
    fail "the dispatcher must own the local console principal"
fi
# The console crate must not be able to name a control method.
if grep -qE '^[[:space:]]*i2pr-i2pcontrol' "${CONSOLE_MANIFEST}"; then
    fail "i2pr-console must not depend on i2pr-i2pcontrol"
fi

# --- rule 7: the dependency map covers every workspace member --------------

python3 - <<'PY'
import json
import subprocess
import sys

manifest = json.loads(
    subprocess.run(
        ["cargo", "metadata", "--locked", "--format-version", "1", "--no-deps"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
)
members = {
    package["name"] for package in manifest["packages"] if package["name"].startswith("i2pr-")
}
script = open("scripts/check-dependency-direction.sh", encoding="utf-8").read()
missing = sorted(name for name in members if f'"{name}"' not in script)
if missing:
    print(
        "check-console-boundaries: FAIL: workspace members absent from the "
        f"dependency-direction expected map: {missing}",
        file=sys.stderr,
    )
    raise SystemExit(1)
PY
if [ "$?" -ne 0 ]; then
    failures=$((failures + 1))
fi


if [ "${failures}" -ne 0 ]; then
    printf 'check-console-boundaries: %d violation(s)\n' "${failures}" >&2
    exit 1
fi

printf 'check-console-boundaries: ok\n'