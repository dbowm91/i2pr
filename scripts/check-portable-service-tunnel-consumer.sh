#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
manifest="$root/tests/portable-service-tunnel-consumer/Cargo.toml"
expected_rev="fa0824970b67b5ee90d5907765c408ccd0a19941"
target_dir="$(mktemp -d "${TMPDIR:-/tmp}/i2pr-portable-service-tunnel-consumer.XXXXXX")"
trap 'rm -rf "$target_dir"' EXIT

if ! grep -Fq "rev = \"$expected_rev\"" "$manifest"; then
  echo "external consumer must pin the reviewed Plan 350 revision $expected_rev" >&2
  exit 1
fi
if grep -En 'i2pr-(daemon|runtime|testkit)|crates/i2pr-service-tunnels/src|path[[:space:]]*=' \
  "$manifest" "$root/tests/portable-service-tunnel-consumer/tests"/*.rs >/dev/null; then
  echo "external consumer references a private source path or forbidden router crate" >&2
  exit 1
fi

export CARGO_TARGET_DIR="$target_dir"
cargo test --locked --manifest-path "$manifest"
tree="$(cargo tree --locked --manifest-path "$manifest" --edges normal)"
if grep -En 'i2pr-(daemon|runtime|testkit|proto|netdb|transport|tunnel|client)' <<<"$tree" >/dev/null; then
  echo "external consumer has a forbidden internal i2pr dependency" >&2
  exit 1
fi
if ! grep -Fq "i2pr-service-tunnels v0.1.0 (https://github.com/dbowm91/i2pr?rev=$expected_rev#${expected_rev:0:8})" <<<"$tree"; then
  echo "cargo tree does not resolve the exact pinned external package revision" >&2
  exit 1
fi

echo "portable service-tunnel external consumer passed at $expected_rev"
