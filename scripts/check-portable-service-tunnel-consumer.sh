#!/usr/bin/env bash
# Plan 351 external-consumer proof, extended by Plan 379 to a *second* fixture.
#
# There are two, and there must be two:
#
#   tests/portable-service-tunnel-consumer/          -> Plan 350 rev fa082497...
#   tests/portable-service-tunnel-consumer-current/  -> Plan 379 rev f0fb74a8...
#
# The historical one is the evidence that the portable boundary held *before* the
# `i2pr-proto` edge existed. The current one proves the same boundary against
# today's package, including the edge Plan 359 added. Repointing the historical
# fixture would destroy the comparison, so both run.
#
# The allowed internal-dependency sets differ per fixture and that difference is
# the point. The Plan 350 revision had no `i2pr-*` dependency at all. The current
# revision has exactly one -- `i2pr-proto` -- and must still have no more. A
# single shared pattern would have to be the looser one for both, and a looser
# pattern applied to the historical fixture would let a real regression pass.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

historical_rev="fa0824970b67b5ee90d5907765c408ccd0a19941"
current_rev="f0fb74a8582d6077a7c5db49d613699688115bad"

target_dir="$(mktemp -d "${TMPDIR:-/tmp}/i2pr-portable-service-tunnel-consumer.XXXXXX")"
trap 'rm -rf "$target_dir"' EXIT
export CARGO_TARGET_DIR="$target_dir"

# Every forbidden internal crate, in one place. Both fixtures reject all of these;
# the current one additionally permits `i2pr-proto`, which is why the pattern
# excludes it and the caller adds it back only for the fixture that may have it.
forbidden_internal='i2pr-(daemon|runtime|testkit|netdb|transport|tunnel|client|api|crypto|core|console|storage|su3|netdb-persist|transport-ntcp2|transport-ssu2|addressbook|i2pcontrol|appd|apphost|app-proto|app-manager-proto|app-fixture)'

# A guard that can only fail on the tree it was just tested against proves
# nothing. Every name the pattern is supposed to catch must actually match it,
# and the two names it is supposed to allow must actually not match it -- the
# package under test and the one permitted edge.
for forbidden in i2pr-daemon i2pr-runtime i2pr-testkit i2pr-netdb i2pr-transport \
                i2pr-tunnel i2pr-client i2pr-api i2pr-crypto i2pr-core \
                i2pr-console i2pr-storage i2pr-su3 i2pr-netdb-persist \
                i2pr-transport-ntcp2 i2pr-transport-ssu2 i2pr-addressbook \
                i2pr-i2pcontrol i2pr-appd i2pr-apphost i2pr-app-proto \
                i2pr-app-manager-proto i2pr-app-fixture; do
  if ! printf '%s\n' "$forbidden" | grep -Eq "$forbidden_internal"; then
    echo "forbidden-dependency detector misses $forbidden; the guard is vacuous" >&2
    exit 1
  fi
done
for allowed in i2pr-service-tunnels i2pr-proto; do
  if printf '%s\n' "$allowed" | grep -Eq "$forbidden_internal"; then
    echo "forbidden-dependency detector wrongly matches the permitted $allowed" >&2
    exit 1
  fi
done

# run_fixture <dir> <expected-rev> <allowed-internal-regex-or-empty>
run_fixture() {
  local dir="$1" expected_rev="$2" allow_internal="${3:-}"
  local manifest="$root/$dir/Cargo.toml"

  if ! grep -Fq "rev = \"$expected_rev\"" "$manifest"; then
    echo "$dir must pin the reviewed revision $expected_rev" >&2
    exit 1
  fi
  # A path dependency, a private source path, or a direct reference to a
  # forbidden router crate would defeat the whole point: the fixture would be
  # testing the workspace through a side door rather than the published package.
  if grep -En 'i2pr-(daemon|runtime|testkit)|crates/i2pr-service-tunnels/src|path[[:space:]]*=' \
    "$manifest" "$root/$dir/tests"/*.rs >/dev/null; then
    echo "$dir references a private source path or forbidden router crate" >&2
    exit 1
  fi

  cargo test --locked --manifest-path "$manifest"
  local tree
  tree="$(cargo tree --locked --manifest-path "$manifest" --edges normal)"

  if grep -En "$forbidden_internal" <<<"$tree" >/dev/null; then
    echo "$dir resolved a forbidden internal i2pr dependency" >&2
    exit 1
  fi
  if [[ -n "$allow_internal" ]] && ! grep -Eq "$allow_internal" <<<"$tree"; then
    echo "$dir did not resolve the internal dependency it is expected to need" >&2
    exit 1
  fi
  if ! grep -Fq "i2pr-service-tunnels v0.1.0 (https://github.com/dbowm91/i2pr?rev=$expected_rev#${expected_rev:0:8})" <<<"$tree"; then
    echo "$dir: cargo tree does not resolve the exact pinned external package revision" >&2
    exit 1
  fi
  echo "$dir passed at $expected_rev"
}

run_fixture "tests/portable-service-tunnel-consumer" "$historical_rev" ""
# Plan 359 permits exactly this one edge. `i2pr-proto` is excluded from
# $forbidden_internal and required here, so the fixture cannot silently gain a
# second internal dependency. No `^` anchor: `cargo tree` indents dependencies
# under the root package, so the line starts with a tree-drawing character.
run_fixture "tests/portable-service-tunnel-consumer-current" "$current_rev" \
  "i2pr-proto v0.1.0 \(https://github.com/dbowm91/i2pr\?rev=$current_rev#${current_rev:0:8}\)"

# Plan 379 WP E. The obsolete plans/349-351 branch committed 409 generated
# build artifacts under tests/portable-service-tunnel-consumer/target/. They are
# disposable, they were never merged, and nothing should ever track one again.
# This is cheap and it is the difference between "we deleted the branch" and
# "the branch's damage cannot recur".
if git -C "$root" ls-files -- 'tests/**/target/**' | grep -q .; then
  echo "generated fixture build artifacts are tracked; they must never be committed" >&2
  git -C "$root" ls-files -- 'tests/**/target/**' | head >&2
  exit 1
fi

echo "portable service-tunnel external consumers passed (historical $historical_rev, current $current_rev)"