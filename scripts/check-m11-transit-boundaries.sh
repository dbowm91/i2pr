#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source_file="$root/crates/i2pr-tunnel/src/transit.rs"

fail() { echo "check-m11-transit-boundaries: $*" >&2; exit 1; }

for owner in TransitHopRole TransitHopRegistration TransitRegistry TransitAdmissionState TransitAdmissionToken; do
    if rg -U -n "derive\([^)]*Clone[^)]*\)[[:space:]]*(pub[[:space:]]+)?(struct|enum)[[:space:]]+$owner" "$source_file"; then
        fail "$owner must not implement Clone"
    fi
done

remove_body="$(sed -n '/pub fn remove(/,/pub fn remove_into(/p' "$source_file")"
[[ -n "$remove_body" ]] || fail "TransitRegistry::remove not found"
if rg -n 'expect\(|unwrap\(|panic!|unreachable!' <<<"$remove_body"; then
    fail "TransitRegistry::remove must return typed unknown-id errors"
fi
if rg -n 'tokio::|std::net|std::fs|async[[:space:]]+fn|JoinHandle|spawn\(' "$source_file"; then
    fail "runtime ownership leaked into runtime-neutral transit module"
fi
if rg -n 'std::sync|Mutex|RwLock|AtomicU|AtomicI' "$source_file"; then
    fail "M11 foundation must leave synchronization ownership to the runtime plan"
fi

echo "check-m11-transit-boundaries: passed"
