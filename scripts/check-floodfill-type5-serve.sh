#!/usr/bin/env bash
# Plan 350 static guard: the floodfill's servable record types.
#
# The bug this prevents is silent and data-only. `ServerNetDb::database_store_for_answer`
# implements a `record_type` arm per storeable record type, and `FloodfillStoreService` keeps a
# separate list of which types to probe for each `DatabaseLookupMessage::lookup_type`. Nothing
# in the type system connects the two, so a type can be fully implemented in the store, fully
# admitted by `handle`, and then never named in any lookup list. The record is stored, the
# floodfill reports success, and every consumer lookup misses forever.
#
# That is exactly what happened to type 5 (encrypted LeaseSet2): `handle` refused it outright
# and the lookup lists omitted it, so no reference client could ever resolve an i2pr-published
# encrypted service. Neither failure is observable from a unit test about another record type,
# which is why this is a static check rather than a test.
#
# The behavioural half lives in `crates/i2pr-netdb/tests/floodfill_type5_serve.rs`.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
service="$root/crates/i2pr-netdb/src/floodfill_service.rs"
store="$root/crates/i2pr-netdb/src/server_store.rs"

fail() {
  echo "floodfill type-5 serve coverage: $*" >&2
  exit 1
}

# --- the store must actually implement every declared servable type -----------------------

# Pull the `record_type` arms out of `database_store_for_answer`. The function is a single
# `match record_type` with arms `0 =>`, `1 =>`, `3 =>`, `5 =>`, `7 =>`, so arm numbers are the
# record types. Deriving the set from the source is the point: a new storeable type that is
# not declared below must fail here, not silently go unserved.
store_arms="$(awk '
  /pub fn database_store_for_answer\(/ { infn = 1; next }
  infn && /^    }/ { infn = 0 }
  infn && /^            [0-9]+ => \{/ {
    line = $0
    gsub(/[^0-9]/, "", line)
    print line
  }
' "$store" | sort -n -u | tr '\n' ' ')"

# Only the bracket contents are read, so the `u8` in the type annotation cannot be mistaken
# for a record type.
declared="$(rg -o 'SERVABLE_RECORD_TYPES: &\[u8\] = &\[([^]]*)\]' -r '$1' "$service" \
  | rg -o '\b[0-9]+\b' | sort -n -u | tr '\n' ' ')"

[ -n "$store_arms" ] || fail "could not read the record_type arms from database_store_for_answer"
[ -n "$declared" ] || fail "SERVABLE_RECORD_TYPES is missing or unparseable in floodfill_service.rs"

[ "$store_arms" = "$declared" ] \
  || fail "database_store_for_answer implements types [$store_arms] but SERVABLE_RECORD_TYPES declares [$declared]; every storeable type must be declared, and every declared type must be servable"

# --- every declared type must be probed by at least one lookup list -----------------------

normal="$(rg -o 'SERVABLE_NORMAL_LOOKUP: &\[u8\] = &\[([^]]*)\]' -r '$1' "$service" \
  | rg -o '\b[0-9]+\b' | sort -n -u | tr '\n' ' ')"
lease="$(rg -o 'SERVABLE_LEASE_LOOKUP: &\[u8\] = &\[([^]]*)\]' -r '$1' "$service" \
  | rg -o '\b[0-9]+\b' | sort -n -u | tr '\n' ' ')"

[ -n "$normal" ] || fail "SERVABLE_NORMAL_LOOKUP is missing or unparseable"
[ -n "$lease" ] || fail "SERVABLE_LEASE_LOOKUP is missing or unparseable"

probed="$(printf '%s%s' "$normal" "$lease" | tr ' ' '\n' | rg -v '^$' | sort -n -u | tr '\n' ' ')"
[ "$probed" = "$declared" ] \
  || fail "declared servable types [$declared] but the lookup lists probe only [$probed]; a type that no lookup probes is stored and never served"

# --- type 5 specifically, because it is the one with two independent gates -----------------

# The store entry point must not refuse type 5. Plan 350 removed this hold-back; a
# reintroduced refusal would make every reference-published type-5 store a silent no-op.
if sed -n '/    pub fn handle(/,/^    }/p' "$service" | rg -n 'record_type == 5'; then
  fail "FloodfillStoreService::handle refuses record_type 5; the type-5 validate arm is unreachable and reference-published encrypted LeaseSet2 records cannot be stored"
fi

# The LeaseSet lookup list is the one a reference client uses for a blinded storage key.
for required in 1 3 5 7; do
  printf '%s' "$lease" | tr ' ' '\n' | rg -qx "$required" \
    || fail "SERVABLE_LEASE_LOOKUP must include record type $required"
done

# --- the lists must actually be used, not merely defined -----------------------------------

for name in SERVABLE_NORMAL_LOOKUP SERVABLE_LEASE_LOOKUP SERVABLE_RECORD_TYPES; do
  uses="$(rg -c "\b$name\b" "$service" || true)"
  [ "${uses:-0}" -ge 2 ] \
    || fail "$name is declared but never used by lookup_body; a shadowing literal would silently restore the old lists"
done

# lookup_body must route through the named lists rather than inline literals.
lookup_body="$(sed -n '/    fn lookup_body(/,/^    }/p' "$service")"
printf '%s' "$lookup_body" | rg -q 'SERVABLE_NORMAL_LOOKUP' \
  || fail "lookup_body does not use SERVABLE_NORMAL_LOOKUP"
printf '%s' "$lookup_body" | rg -q 'SERVABLE_LEASE_LOOKUP' \
  || fail "lookup_body does not use SERVABLE_LEASE_LOOKUP"
if printf '%s' "$lookup_body" | rg -q '=> &\[([0-9]+, )+[0-9]+\]'; then
  fail "lookup_body still uses an inline record-type list; route both branches through the named constants so the guard can see them"
fi

# --- the store layer must still refuse to derive a subcredential ---------------------------

# A floodfill stores type-5 records opaquely. If it ever learned to derive the subcredential it
# would be holding material it has no business holding, since it never learns the unblinded key.
printf '%s' "$lookup_body" | rg -q 'database_store_for_answer' \
  || fail "lookup_body no longer delegates to database_store_for_answer"
rg -q 'EncryptedLeaseSet\(value\) =>' "$service" \
  || fail "the type-5 validate arm is missing from floodfill_service.rs"

echo "floodfill type-5 serve coverage OK"
