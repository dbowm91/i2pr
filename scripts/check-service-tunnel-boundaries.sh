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
