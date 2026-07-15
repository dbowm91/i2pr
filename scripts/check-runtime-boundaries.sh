#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if rg -n "unbounded_channel|unbounded::<|UnboundedSender|UnboundedReceiver" \
  "$root/crates/i2pr-runtime/src" "$root/crates/i2pr-testkit/src"; then
  echo "unbounded asynchronous channels are forbidden in runtime/testkit source" >&2
  exit 1
fi

if rg -n 'std::thread::sleep|thread::sleep|std::mem::forget|mem::forget' \
  "$root/crates/i2pr-runtime" "$root/crates/i2pr-testkit"; then
  echo "wall-clock sleeps and handle-forgetting are forbidden in deterministic lanes" >&2
  exit 1
fi

if rg -n 'tokio::spawn\(' "$root/crates/i2pr-runtime" "$root/crates/i2pr-testkit" \
  | rg -v 'let .* =|push\(|JoinSet'; then
  echo "every tokio::spawn call must retain an explicit owner" >&2
  exit 1
fi

if rg -n 'JoinHandle' "$root/crates/i2pr-runtime" "$root/crates/i2pr-testkit"; then
  echo "raw JoinHandle ownership requires a reviewed owner-specific implementation" >&2
  exit 1
fi

for manifest in "$root"/crates/*/Cargo.toml; do
  crate=$(basename "$(dirname "$manifest")")
  if [[ "$crate" != i2pr-runtime && "$crate" != i2pr-testkit ]] \
    && rg -n '^(tokio|tokio-util)[[:space:]]*=' "$manifest"; then
    echo "Tokio dependencies are confined to approved runtime/testkit manifests" >&2
    exit 1
  fi
done

if rg -n 'i2pr-testkit' "$root/crates"/*/Cargo.toml | rg -v 'crates/i2pr-testkit/Cargo.toml'; then
  echo "production crate depends on i2pr-testkit" >&2
  exit 1
fi

if rg -n 'tokio::|std::net|std::fs|TcpStream|TcpListener|UdpSocket|UnixStream|OpenOptions|File::' \
  "$root/crates/i2pr-transport/src" "$root/crates/i2pr-transport-ntcp2/src"; then
  echo "transport contract crates must not own Tokio, sockets, or filesystem I/O" >&2
  exit 1
fi

if rg -n 'async[[:space:]]+fn|async_trait|i2pr-(netdb|tunnel|client)' \
  "$root/crates/i2pr-transport" "$root/crates/i2pr-transport-ntcp2"; then
  echo "transport contracts must remain synchronous and independent of routing clients" >&2
  exit 1
fi

if rg -n 'i2pr-daemon|i2pr-runtime|i2pr-testkit' \
  "$root/crates/i2pr-transport/Cargo.toml" "$root/crates/i2pr-transport-ntcp2/Cargo.toml"; then
  echo "transport crates must not depend on runtime, daemon, or testkit" >&2
  exit 1
fi

echo "runtime boundary checks passed"
