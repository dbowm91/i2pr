#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if rg -n "unbounded_channel|unbounded::<|UnboundedSender|UnboundedReceiver" \
  "$root/crates/i2pr-runtime/src" "$root/crates/i2pr-testkit/src"; then
  echo "unbounded asynchronous channels are forbidden in runtime/testkit source" >&2
  exit 1
fi

if rg -n 'std::thread::sleep|thread::sleep|tokio::time::sleep\([^)]*Duration::from_secs\([0-9]{2,}' \
  "$root/crates/i2pr-runtime" "$root/crates/i2pr-testkit"; then
  echo "wall-clock or long unbounded sleeps are forbidden in deterministic lanes" >&2
  exit 1
fi

if rg -n 'i2pr-testkit' "$root/crates"/*/Cargo.toml | rg -v 'crates/i2pr-testkit/Cargo.toml'; then
  echo "production crate depends on i2pr-testkit" >&2
  exit 1
fi

echo "runtime boundary checks passed"
