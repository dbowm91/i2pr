#!/usr/bin/env bash
# Plan 351 static guard: the encrypted-service (`.b33`) consumer wiring boundary.
#
# Plan 349 built `EncryptedServiceResolver` with no production caller. Plan 351 supplies one.
# Three things would silently undo the containment that made adding a caller acceptable, and
# none of them would fail a test on their own:
#
#   1. Gate 1 disappearing — an encrypted remote target on a service that cannot isolate its own
#      failure. Two of the three production callers of the router-material provisioning pass shut
#      the manager down, cancel the operator token, and shut the SSU2 handle down on any error,
#      so one unreachable `.b33` would take down every configured service in the product;
#   2. the `.b33` ceasing to be a *distinct* reference kind — i.e. a b33 folded into
#      `Base32Hash` or routed as "locally co-owned", either of which turns an unresolvable
#      remote endpoint into a silent local delivery;
#   3. the identity binding being skipped, i.e. an unwrapped inner LeaseSet2 installed without
#      proving its signing key equals the unblinded public key the `.b33` names. Without that
#      check any valid LeaseSet2 for any destination passes, and a publisher can hand a consumer
#      a working record for a service the address does not claim.
#
# This checker is the static half of those three claims. The behavioural half lives in
# `crates/i2pr-daemon/tests/encrypted_service_consumer_wiring.rs`.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tunnels="$root/crates/i2pr-service-tunnels/src"
daemon="$root/crates/i2pr-daemon/src"

fail() {
  echo "encrypted-service consumer wiring: $*" >&2
  exit 1
}

# --- 1. Gate 1: an encrypted target needs a delay-open client, at both layers --------------

# The runtime-neutral validator is the real boundary: it is what every spec passes through,
# regardless of which surface configured it. Losing the rule there would leave only the
# I2PControl duplicate, which a TOML-configured or programmatically-built spec would bypass.
rg -q 'an encrypted-service remote target requires a client service tunnel with delay open set' \
  "$tunnels/config.rs" \
  || fail "ServiceTunnelSpec::validate lost the Gate 1 encrypted-target rule"
rg -q 'DestinationRef::EncryptedService' "$tunnels/config.rs" \
  || fail "Gate 1 must key on the EncryptedService reference variant"

# The I2PControl duplicate exists so the operator gets a named rule. Both layers must be present
# or neither: one alone is a bypass, and having both is only safe if they cannot disagree, which
# is why removing either is a failure here rather than a refactor.
rg -q 'leaseset_password on a client tunnel requires target_destination to be an encrypted-service address' \
  "$daemon/i2pcontrol_tunnels.rs" \
  || fail "the I2PControl Gate 2 pairing rule is missing"

# The static-alias bypass must stay closed. Gate 1 is a per-service property and an alias is
# global, so an alias that could name a b33 would let a service without `delay_open` reach the
# encrypted path by naming the alias instead of the address.
rg -q 'an encrypted-service address must not be reachable through a static alias' \
  "$tunnels/destination.rs" \
  || fail "the static-alias bypass for encrypted-service addresses was reopened"

# --- 2. a b33 is a distinct reference kind, and never a destination hash ------------------

# A b33 must be dispatched before the 52-character Base32 branch, or a valid address is
# rejected with the misleading reason "Base32 label must be exactly 52 characters".
parse_impl="$(awk '/    pub fn parse\(value: &str\)/{flag=1} flag{print} flag && /^    \}$/{exit}' \
  "$tunnels/destination.rs")"
[ -n "$parse_impl" ] || fail "DestinationRef::parse impl block not found"
printf '%s\n' "$parse_impl" | rg -q 'is_encrypted_service_address' \
  || fail "DestinationRef::parse must dispatch an encrypted-service address explicitly"
encrypted_pos="$(printf '%s\n' "$parse_impl" | rg -n 'is_encrypted_service_address' | head -1 | cut -d: -f1)"
b32_pos="$(printf '%s\n' "$parse_impl" | rg -n 'strip_suffix\(B32_SUFFIX\)' | head -1 | cut -d: -f1)"
[ -n "$encrypted_pos" ] && [ -n "$b32_pos" ] && [ "$encrypted_pos" -lt "$b32_pos" ] \
  || fail "the encrypted-service dispatch must precede the .b32.i2p branch"

# The projection must keep three outcomes. Collapsing the encrypted case into `LocalCoOwned`
# would route an unresolvable remote endpoint to the local bridge, and collapsing it into
# `Remote` would require inventing a destination hash the address does not carry.
rg -q 'pub enum RemoteTargetProjection' "$daemon/service_tunnels.rs" \
  || fail "RemoteTargetProjection is missing; the Option-based projection is back"
for variant in LocalCoOwned EncryptedService; do
  rg -q "    $variant[(]|    $variant," "$daemon/service_tunnels.rs" \
    || fail "RemoteTargetProjection must keep the $variant outcome"
done
rg -q 'EncryptedService\(EncryptedServiceAddress\)' "$daemon/service_tunnels.rs" \
  || fail "RemoteTargetProjection::EncryptedService must carry the validated address by value"

# The lookup key must be usable verbatim. A blinded storage key is not a destination hash, so a
# seam that re-derives its key from a `DestinationHash` cannot serve this path at all.
rg -q 'pub fn begin_lease_set2_lookup_for_key_with_store' "$daemon/netdb_seam.rs" \
  || fail "the seam lost the lookup-key-supplied entry point"
seam_impl="$(awk '/    pub fn begin_lease_set2_lookup_for_key_with_store\(/{flag=1} flag{print} flag && /^    \}$/{exit}' \
  "$daemon/netdb_seam.rs")"
[ -n "$seam_impl" ] || fail "begin_lease_set2_lookup_for_key_with_store impl block not found"
if printf '%s\n' "$seam_impl" | rg -q 'router_hash_from_destination'; then
  fail "the verbatim-key entry point must not re-derive the key from a destination hash"
fi
rg -q 'LookupId::new\(request_id, LookupKind::LeaseSet2, lookup_key\)' "$daemon/netdb_seam.rs" \
  || fail "the lookup id must be built from the supplied key"
rg -q 'pub fn begin_encrypted_lease_lookup' "$daemon/destination_tunnels.rs" \
  || fail "the coordinator lost the encrypted lookup entry point"

# --- 3. the identity binding is not optional ----------------------------------------------

# `bind_encrypted_inner_to_address` is the only thing that ties a fetched record to the address
# the operator configured. Its two checks are the whole security argument: the unwrapped inner
# record may name itself only if it signs with the key the `.b33` published, and declares the
# signature type the `.b33` published.
# The policy lives with the rest of the ELS2 consumer policy so it is directly testable and a
# second consumer cannot reimplement it wrong. The product layer must call it rather than
# reimplement the two checks.
rg -Fq 'pub fn bind_inner_to_address' "$daemon/encrypted_service_resolver.rs" \
  || fail "the b33 identity binding owner is missing"
if rg -q 'fn bind_encrypted_inner_to_address' "$daemon/service_product.rs"; then
  fail "the product layer must call the ELS2 owner, not keep a private copy of the binding"
fi
rg -Fq 'bind_inner_to_address(&address, inner)' "$daemon/service_product.rs" \
  || fail "the install path must bind through the ELS2 owner"
bind_impl="$(awk '/^pub fn bind_inner_to_address\(/{flag=1} flag{print} flag && /^}$/{exit}' \
  "$daemon/encrypted_service_resolver.rs")"
[ -n "$bind_impl" ] || fail "bind_inner_to_address is missing"
printf '%s\n' "$bind_impl" | rg -Fq 'signing_key.as_bytes() != address.public_key()' \
  || fail "the binding must compare the inner signing key against the b33 public key"
printf '%s\n' "$bind_impl" | rg -Fq 'signing_key.key_type().code() != address.unblinded_sigtype()' \
  || fail "the binding must compare the unblinded signature type"
# The destination hash must come from the *inner record's* Destination, which is the only
# place a Destination exists. Hashing the address's public key would produce a plausible-looking
# 32 bytes that no LS2 is ever filed under.
printf '%s\n' "$bind_impl" | rg -Fq '.hash()' \
  || fail "the destination hash must be derived from the inner record's Destination"
bind_code="$(printf '%s\n' "$bind_impl" | sed 's://.*::')"
if printf '%s\n' "$bind_code" | rg -q 'address[.]public_key[(][)].*hash|Hash::(sha256|sha512|digest)[(][)]'; then
  fail "the binding must not fabricate a destination hash from the address"
fi

# The install must use the inner record's own destination hash, never the blinded storage key.
# Installing under the blinded key would place a record where nothing looks it up.
rg -q 'Some\(destination_hash\),' "$daemon/service_product.rs" \
  || fail "validation must pass the bound destination hash, not the storage key"

# The type-5 arm in the NetDB engine must stay a delegation. Validating or unwrapping there
# would move ADR 0032's closed type-11 profile policy out of the owner and into a layer that
# holds no daily material and no credential.
engine_arm="$(awk '/DatabaseStoreData::EncryptedLeaseSet\(record\) => \{/{flag=1} flag{print} flag && /^        \}$/{exit}' \
  "$root/crates/i2pr-netdb/src/lookup_engine.rs")"
[ -n "$engine_arm" ] || fail "the type-5 arm in lookup_engine.rs is missing"
# Comments are stripped first: this arm's own comment says "No `store.insert`", so a
# substring search over raw text would reject the very comment that documents the boundary.
engine_code="$(printf '%s\n' "$engine_arm" | sed 's://.*::')"
if printf '%s\n' "$engine_code" | rg -q 'ValidatedEncryptedLeaseSet2|store[.]insert[(]'; then
  fail "the NetDB type-5 arm must not validate, unwrap, or install the record"
fi
if printf '%s\n' "$engine_code" | rg -q 'EncryptedLeaseSet2Resolver|resolve[(]'; then
  fail "the NetDB type-5 arm must not unwrap; the ELS2 owner holds the daily material"
fi

# --- 4. the failure cannot escape ----------------------------------------------------------

# The b33 branch must record and continue. A `?` there propagates into the provisioning pass,
# whose callers tear the whole product down.
rg -q 'async fn provision_encrypted_service_target' "$daemon/service_product.rs" \
  || fail "the record-and-continue b33 entry point is missing"
wrapper="$(awk '/^async fn provision_encrypted_service_target\(/{flag=1} flag{print} flag && /^}$/{exit}' \
  "$daemon/service_product.rs")"
[ -n "$wrapper" ] || fail "provision_encrypted_service_target is missing"
# Two independent shapes of the same claim, because either alone is easy to satisfy
# accidentally: the wrapper must not be able to return a Result at all, and it must actually
# consume the failure rather than dropping it.
if printf '%s\n' "$wrapper" | rg -q '^\) -> Result'; then
  fail "the b33 provisioning entry point must not be able to return a Result"
fi
printf '%s\n' "$wrapper" | rg -q 'if let Err\(status\) =' \
  || fail "the b33 provisioning entry point must consume the typed failure"
printf '%s\n' "$wrapper" | rg -q 'record_encrypted_target_status' \
  || fail "every b33 outcome must be recorded on the per-service status surface"

# The resolve function's error type must be the closed status enum, never a ServiceProductError
# carrying a foreign string.
resolve_sig="$(awk '/^async fn resolve_encrypted_destination_for_service\(/{flag=1} flag{print} flag && /^\) -> Result<\(\), EncryptedTargetStatus> \{/{exit}' \
  "$daemon/service_product.rs")"
[ -n "$resolve_sig" ] || fail "resolve_encrypted_destination_for_service must return the closed status enum"

echo "encrypted-service consumer wiring OK"