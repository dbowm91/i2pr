#!/usr/bin/env bash
# Plan 180 §15 static service-tunnel boundary checker.
#
# Reject the same M10 invariants `check-runtime-boundaries.sh` enforces
# plus the Plan 180-specific additions: service-specific Garlic/I2NP
# construction, non-loopback targets in production config paths,
# duplicate SAM-style raw byte pump reintroduction, and unbounded
# Tokio channel/semaphore usage by omission.
#
# Run from the repository root. Exits 0 on success, non-zero on
# any violation.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# 1. i2pr-service-tunnels stays runtime-neutral: no Tokio, sockets,
#    listeners, tasks, or timers. (Plan 174 §3; mirrored from
#    check-runtime-boundaries.sh.)
if grep -REn 'tokio::|TcpListener|TcpStream|UdpSocket|UnixListener|UnixStream|tokio::net|tokio::spawn|tokio::time|tokio::sync' \
  "$root/crates/i2pr-service-tunnels/src" >/dev/null; then
  echo "i2pr-service-tunnels must remain runtime-neutral: no Tokio, sockets, listeners, tasks, or timers" >&2
  exit 1
fi

# 2. i2pr-service-tunnels must not depend on transport/tunnel/
#    runtime/daemon/testkit.
if grep -En 'i2pr-transport|i2pr-tunnel|i2pr-runtime|i2pr-daemon|i2pr-testkit' \
  "$root/crates/i2pr-service-tunnels/Cargo.toml" >/dev/null; then
  echo "i2pr-service-tunnels must not depend on transport/tunnel internals, runtime, daemon, or testkit" >&2
  exit 1
fi

# Plans 349–350: the portable policy core must not acquire ownership of router
# state, persistence, or runtime facilities.
#
# Plan 359 amendment (2026-10-06). The alternation below originally ended with
# `|proto`. That was justified by an audit *result* -- "Plan 350's source/tree
# audit found no production use of i2pr-proto, so it is no longer a package
# dependency" -- and not by any claim that i2pr-proto is a router, runtime, or
# persistence facility. Proposal 170/351 subsequently added a production use of
# `i2pr_proto` (`EncryptedServiceAddress`, `is_encrypted_service_address`) on a
# branch that had never seen this guard, so the premise went stale while the
# rationale stayed true. i2pr-proto is the bounded wire-codec crate: it is
# runtime-neutral, performs no I/O, owns no state or persistence, and is
# already a production dependency of i2pr-api, i2pr-netdb, i2pr-transport, and
# i2pr-client. It is therefore not in the class this rule exists to exclude, so
# it is permitted explicitly rather than by omission. The forbidden set is
# unchanged for every crate that actually owns router state or a runtime.
portable_dependency_pattern='i2pr-(daemon|runtime|netdb(-persist)?|transport(-ntcp2|-ssu2)?|tunnel|testkit)'
if grep -En "$portable_dependency_pattern" "$root/crates/i2pr-service-tunnels/Cargo.toml" >/dev/null; then
  echo "portable service-tunnel core must not depend on router/runtime/NetDB/transport/tunnel crates" >&2
  exit 1
fi

# The package must not regress to a direct production dependency on any other
# i2pr crate. Dependency keys begin at the start of a TOML line; the package's
# own `name` is not matched. Plan 359 permits exactly one workspace edge,
# `i2pr-proto`, for the reason recorded above -- so it is named as an exception
# here instead of being silently dropped from the check.
if grep -En '^[[:space:]]*i2pr-[a-z0-9-]+[[:space:]]*=' \
  "$root/crates/i2pr-service-tunnels/Cargo.toml" \
  | grep -Ev '^[0-9]+:[[:space:]]*i2pr-proto[[:space:]]*=' >/dev/null; then
  echo "portable service-tunnel package must remain independent of workspace crates (i2pr-proto excepted by Plan 359)" >&2
  exit 1
fi

# A negative boundary rule needs a positive control. Confirm the same
# expressions catch representative forbidden manifest entries, and confirm the
# one permitted edge does NOT trip either rule.
if ! printf '%s\n' 'i2pr-daemon' | grep -En "$portable_dependency_pattern" >/dev/null || \
   ! printf '%s\n' 'i2pr-netdb-persist' | grep -En "$portable_dependency_pattern" >/dev/null || \
   ! printf '%s\n' 'i2pr-transport-ssu2' | grep -En "$portable_dependency_pattern" >/dev/null || \
   ! printf '%s\n' 'i2pr-tunnel' | grep -En "$portable_dependency_pattern" >/dev/null || \
   ! printf '%s\n' 'i2pr-runtime' | grep -En "$portable_dependency_pattern" >/dev/null || \
   ! printf '%s\n' 'i2pr-testkit' | grep -En "$portable_dependency_pattern" >/dev/null || \
   ! printf '%s\n' 'i2pr-client = { path = "../i2pr-client" }' | grep -En '^[[:space:]]*i2pr-[a-z0-9-]+[[:space:]]*=' | grep -Ev '^[[:space:]]*i2pr-proto[[:space:]]*=' >/dev/null || \
   ! printf '%s\n' 'i2pr-proto = { path = "../i2pr-proto" }' | grep -En '^[[:space:]]*i2pr-[a-z0-9-]+[[:space:]]*=' | grep -Ev '^[[:space:]]*i2pr-proto[[:space:]]*=' >/dev/null; then
  echo "Plan 359 dependency guard positive control failed" >&2
  exit 1
fi

# Policy code may use address value types (IpAddr/SocketAddr), but may not
# own runtime calls, filesystem/process/DNS access, async functions, or I/O
# socket types. This catches both std and Tokio spellings.
portable_source_pattern='tokio::|async[[:space:]]+fn|std::(net::)?(TcpStream|TcpListener|UdpSocket|UnixStream|UnixListener)|tokio::net|std::fs|tokio::fs|std::process|Command::new|to_socket_addrs|lookup_host|File::(open|create)'
if grep -REn "$portable_source_pattern" "$root/crates/i2pr-service-tunnels/src" >/dev/null; then
  echo "portable service-tunnel source must not own runtime, socket, filesystem, process, or DNS operations" >&2
  exit 1
fi

# Positive controls prove the source detector rejects runtime, socket, and
# filesystem ownership spellings rather than passing vacuously.
if ! printf '%s\n' 'tokio::spawn(task)' | grep -En "$portable_source_pattern" >/dev/null || \
   ! printf '%s\n' 'std::net::TcpStream' | grep -En "$portable_source_pattern" >/dev/null || \
   ! printf '%s\n' 'std::fs::read(path)' | grep -En "$portable_source_pattern" >/dev/null; then
  echo "Plan 349 source guard positive control failed" >&2
  exit 1
fi

# 3. service-tunnels must not build Garlic/I2NP. Plan 173 §3
#    forbids service-specific Garlic/I2NP construction in the
#    runtime-neutral crate.
if grep -REn 'GarlicClove|GarlicMessage|i2np::|i2np_message|build_short_tunnel|build_short_request|build_short_reply' \
  "$root/crates/i2pr-service-tunnels/src" >/dev/null; then
  echo "service-tunnels must not construct Garlic/I2NP messages" >&2
  exit 1
fi

# 4. Service-tunnels must not import transport/tunnel-build internals
#    beyond the runtime-neutral types already exposed through
#    `i2pr-client`.
if grep -REn 'i2pr_transport_ssu2|i2pr_transport_ntcp2|build_short_request|build_short_reply' \
  "$root/crates/i2pr-service-tunnels/src" >/dev/null; then
  echo "service-tunnels must not import transport internals" >&2
  exit 1
fi

# 5. The production daemon config must not permit non-loopback
#    client listeners or server TCP targets. The runtime-neutral
#    crate already rejects non-loopback values structurally; this
#    check enforces the same property on the TOML parsing surface.
if grep -REn 'bind_address.*"0\.0\.0\.0"|bind_address.*"192\.|bind_address.*"10\.' \
  "$root/crates/i2pr-daemon/src/config.rs" >/dev/null; then
  echo "production daemon config must not accept non-loopback bind addresses" >&2
  exit 1
fi

# 6. The Plan 174 shared byte pump is the canonical
#    destination_streaming primitive; no service-specific parser may
#    introduce a duplicate raw byte pump. SAM owns its raw pump
#    adapter; service tunnels delegate to run_stream_pump.
pump_count=$(grep -REn 'fn run_stream_pump\b' "$root/crates/i2pr-daemon/src" | wc -l)
if [[ "$pump_count" -gt 1 ]]; then
  echo "multiple run_stream_pump definitions exist; only the shared Plan 174 pump is allowed" >&2
  exit 1
fi

# 7. The service-tunnel manager must not introduce unbounded
#    Tokio channels/semaphores. Plan 174 forbids unbounded growth.
if grep -REn 'unbounded_channel|unbounded::<|UnboundedSender|UnboundedReceiver' \
  "$root/crates/i2pr-daemon/src/service_tunnels.rs" \
  "$root/crates/i2pr-daemon/src/service_tunnels_http.rs" \
  "$root/crates/i2pr-daemon/src/service_tunnels_socks5.rs" \
  "$root/crates/i2pr-daemon/src/service_tunnels_irc_client.rs" \
  "$root/crates/i2pr-daemon/src/service_tunnels_irc_server.rs" >/dev/null; then
  echo "unbounded asynchronous channels are forbidden in service-tunnel source" >&2
  exit 1
fi

# 8. The service-tunnel manager exposes exactly one
#    `register_service_tunnel_manager` entry point so the daemon
#    graph never accidentally spawns one supervisor per spec.
register_count=$(grep -REn 'pub fn register_service_tunnel_manager' "$root/crates/i2pr-daemon/src" | wc -l)
if [[ "$register_count" -ne 1 ]]; then
  echo "exactly one service-tunnel manager registration entry point is required, got: $register_count" >&2
  exit 1
fi

# 9. Plan 343: the outproxy path may open exactly one kind of route, an I2P
#    Streaming connection to a destination that OutproxyEndpoint::parse has
#    already proved is an I2P destination. Neither outproxy file may name a
#    clearnet socket type, a resolver, or a TLS-to-clearnet client, because
#    the whole design is that there is no direct-clearnet branch to remove
#    later. This is the static half of the invariant; the behavioural half
#    is the no-provider fail-closed row in the wire lane.
#
#    Both files are scanned, and a control (the local TCP listener the
#    client speaks to) is what makes the rule non-vacuous: the daemon's
#    other service-tunnel files legitimately name TcpStream, so the check
#    would fail loudly if it were widened by accident.
outproxy_files=(
  "$root/crates/i2pr-service-tunnels/src/outproxy.rs"
  "$root/crates/i2pr-daemon/src/outproxy_route.rs"
)
for file in "${outproxy_files[@]}"; do
  if [[ ! -f "$file" ]]; then
    echo "expected outproxy source $file is missing" >&2
    exit 1
  fi
  # `std::net::IpAddr` is deliberately NOT matched: parsing an address is
  # how the target grammar refuses IP literals, which is the opposite of a
  # socket. Only real connect/listen/resolve/TLS-client spellings are barred.
  if grep -En 'TcpStream|TcpListener|UdpSocket|to_socket_addrs|lookup_host|\bTcpSocket\b|openssl|native_tls|reqwest|hyper' "$file" >/dev/null; then
    echo "outproxy source $file names a direct-clearnet capability" >&2
    exit 1
  fi
done

# 10. Positive control for rule 9: the guard must be able to fail. The
#     canonical service-tunnel client file does name a local TCP listener, so
#     running the same pattern over it must match. If this ever stops
#     matching, rule 9 has become vacuous rather than the code becoming safe.
if ! grep -En 'TcpStream|TcpListener' "$root/crates/i2pr-daemon/src/service_tunnels_http.rs" >/dev/null; then
  echo "rule 9 positive control no longer matches; the outproxy guard is vacuous" >&2
  exit 1
fi

# 11. Plan 343: the outproxy provider must not be able to load a plugin or
#     execute a command. UseOutproxyPlugin is a Proposal 170 wire boolean
#     that selects the configured provider path, never a module to load, so
#     a dynamic-library or process-spawn spelling in either outproxy file
#     would be a capability the guardrails do not allow.
if grep -En 'libloading|dlopen|Library::new|Command::new|std::process' \
  "${outproxy_files[@]}" >/dev/null; then
  echo "outproxy source must not load a plugin or spawn a process" >&2
  exit 1
fi

echo "service-tunnel boundary checks passed"
