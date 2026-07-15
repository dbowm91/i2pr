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
  "$root/crates/i2pr-transport/src" "$root/crates/i2pr-transport-ntcp2/src" >/dev/null; then
  echo "transport contract crates must not own Tokio, sockets, or filesystem I/O" >&2
  exit 1
fi

if grep -REn 'async[[:space:]]+fn|async_trait|i2pr-(netdb|tunnel|client)' \
  "$root/crates/i2pr-transport" "$root/crates/i2pr-transport-ntcp2" >/dev/null; then
  echo "transport contracts must remain synchronous and independent of routing clients" >&2
  exit 1
fi

if grep -En 'i2pr-daemon|i2pr-runtime|i2pr-testkit' \
  "$root/crates/i2pr-transport/Cargo.toml" "$root/crates/i2pr-transport-ntcp2/Cargo.toml" >/dev/null; then
  echo "transport crates must not depend on runtime, daemon, or testkit" >&2
  exit 1
fi

echo "runtime boundary checks passed"