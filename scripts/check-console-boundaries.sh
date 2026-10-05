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
#   2. `i2pr-console` source contains no listener, socket, Tokio, or
#      filesystem call, and no `axum::serve` path.
#   3. The daemon routes the console through the EggServe server adapter.
#   4. No shared "eepsite" or equivalent asset tree exists in the tree.
#   5. Compiled assets reference no remote origin.
#   6. The workspace contains exactly one HTTP server substrate (EggServe).
#   7. The dependency-direction expected map actually covers every member,
#      closing the known 18-of-20 coverage gap for the new crate.

set -euo pipefail

cd "$(dirname "$0")/.."

CONSOLE_CRATE="crates/i2pr-console"
CONSOLE_MANIFEST="${CONSOLE_CRATE}/Cargo.toml"
DAEMON_CONSOLE="crates/i2pr-daemon/src/console.rs"
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

# --- rule 2: console source purity ------------------------------------------

# Test modules may name these types to build a loopback authority policy;
# the rule judges production code only.
if [ -d "${CONSOLE_CRATE}/src" ]; then
    console_socket_hits=$(awk '
        /#\[cfg\(test\)\]/ { in_tests = 1 }
        !in_tests && /axum::serve|std::net|tokio::|TcpListener|TcpStream|UdpSocket|std::fs|fs::|spawn\(|Server::bind/ {
            print FILENAME ":" FNR ":" $0
        }
    ' $(find "${CONSOLE_CRATE}/src" -name '*.rs') || true)
    if [ -n "${console_socket_hits}" ]; then
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