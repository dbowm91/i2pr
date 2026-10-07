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

# --- 5. Plan 380: the ELS2 consumer credential -----------------------------------------------

# Plan 351 closed the gap where `begin_authorized` had no production caller. Three things would
# silently put it back, and none would fail a test on their own.

# 5a. The authorized branch must actually be reachable from the product. Before Plan 380 the
# product only ever called `begin`, so every `.b33` declaring `B32_FLAG_REQUIRES_CLIENT_KEY` was
# unresolvable and the branch was owner-only.
rg -Fq 'resolver.begin_authorized(' "$daemon/service_product.rs" \
  || fail "the product layer must call begin_authorized; Plan 380's whole point is the branch"
# …and the no-credential branch must still be there. Selection is by what the router holds, so
# dropping `begin` would make every consumer authorized, including ones with no credential.
rg -Fq 'resolver.begin(&address, secret_option' "$daemon/service_product.rs" \
  || fail "the no-credential begin branch must remain; selection is by credential presence"

# 5b. The credential must be secret material, not a String. `Debug` is the whole hazard: one
# derived `Debug` on the manager or on a containing struct would print the key. `Clone` is the
# second: a clone puts a second copy in memory that nothing wipes.
credential_type="$(awk '/^pub enum EncryptedTargetCredential \{/{flag=1} flag{print} flag && /^\}$/{exit}' \
  "$daemon/encrypted_target_credential.rs")"
[ -n "$credential_type" ] || fail "EncryptedTargetCredential is missing"
credential_code="$(printf '%s\n' "$credential_type" | sed 's://.*::')"
if printf '%s\n' "$credential_code" | rg -q 'String|Vec<|#\[derive'; then
  fail "EncryptedTargetCredential must hold key material directly, with no derived trait"
fi
# The trailing ` {` matters: without it the error type's own `Display` — whose name begins with
# the credential's — would match and the guard would fire on correct code.
for forbidden in 'impl core::fmt::Debug for EncryptedTargetCredential {' \
                 'impl core::fmt::Display for EncryptedTargetCredential {' \
                 'impl Clone for EncryptedTargetCredential {'; do
  if rg -Fq "$forbidden" "$daemon/encrypted_target_credential.rs"; then
    fail "EncryptedTargetCredential must not gain $forbidden — key material has no rendering"
  fi
done

# 5c. The manager may hold only the sealed form. `install_encrypted_target_credential` taking a
# `SealedEncryptedTargetCredential` is what makes "the manager never holds the key" structural;
# a `&str` parameter would make it a rule, and a rule is what gets forgotten.
install_sig="$(awk '/    pub fn install_encrypted_target_credential\(/{flag=1} flag{print} flag && /^    \}$/{exit}' \
  "$daemon/service_tunnels.rs")"
[ -n "$install_sig" ] || fail "the manager's credential install entry point is missing"
printf '%s\n' "$install_sig" | rg -q 'SealedEncryptedTargetCredential' \
  || fail "the manager must install only the sealed form; a plaintext parameter would be a leak"
# And the registry field must hold the sealed type, not the credential itself.
registry="$(awk '/^    encrypted_target_credentials:/{flag=1} flag{print} flag && /,$/{exit}' \
  "$daemon/service_tunnels.rs")"
[ -n "$registry" ] || fail "the manager has no credential registry"
printf '%s\n' "$registry" | rg -q 'SealedEncryptedTargetCredential' \
  || fail "the credential registry must hold SealedEncryptedTargetCredential"

# 5d. Sealing happens before storage, and under a key that is not the outproxy key. The two
# domains must not share one, or a stored outproxy credential copied into this slot would open.
rg -Fq 'fn seal_encrypted_target_credential' "$daemon/i2pcontrol_tunnels.rs" \
  || fail "the control plane must seal the credential before the definition is stored"
rg -Fq 'pub const ELS2_CONSUMER_CREDENTIAL_MARKER: &str = "$i2pr1e$"' \
  "$root/crates/i2pr-service-tunnels/src/outbound_secret.rs" \
  || fail "the consumer credential needs its own stored-form marker"
rg -Fq 'i2pr:els2-consumer-credential:secret-box:v1' "$daemon/outbound_secret.rs" \
  || fail "the consumer credential needs its own HKDF label"
if [ "$(rg -c 'ELS2_CONSUMER_CREDENTIAL_MARKER' "$daemon/outbound_secret.rs")" -lt 2 ]; then
  fail "the consumer-credential marker must be used in both the seal and the open arm"
fi

# 5d-bis. Both seal steps must be idempotent, because `edit` re-runs them.
#
# `edit` merges the stored options with the request's, so `normalize_definition_with_filter_root`
# sees a credential that is *already* the owner's stored form. Re-sealing it is silent credential
# destruction: the form is sealed twice, opening it once yields the previous stored form as text,
# and that text is what the owner then presents. Plan 342 could not see this — its rows covered
# generation round trips, which never re-run the seal step, and no row edited an outproxy tunnel.
for seal_fn in seal_outproxy_credential seal_encrypted_target_credential; do
  # The signature wraps across lines, so match on the name and the open paren only.
  seal_impl="$(awk "/^fn ${seal_fn}[(]/{flag=1} flag{print} flag && /^}$/{exit}" \
    "$daemon/i2pcontrol_tunnels.rs")"
  [ -n "$seal_impl" ] || fail "$seal_fn is missing"
  if ! printf '%s\n' "$seal_impl" | rg -q 'starts_with'; then
    fail "$seal_fn must recognise its own stored form; re-sealing it destroys the credential on \
      every edit of the tunnel"
  fi
done

# 5e. No advertisement change. The credential is an i2pr-local option; if it were ever added to
# either frozen Proposal 170 inventory this router would claim to accept a Proposal field that
# Proposal does not define, and `full-proposal-conformant` would become a false statement.
for frozen in crates/i2pr-i2pcontrol/src/proposal_wire.rs crates/i2pr-i2pcontrol/src/tunnel_options.rs; do
  if rg -Fq '"leaseset_client_credential"' "$root/$frozen"; then
    fail "leaseset_client_credential must not appear in $frozen — that is the frozen Proposal inventory"
  fi
done

# 5f. The required-but-absent case must fail closed before any lookup is composed, and must be
# distinguishable from a crypto or storage failure. Collapsing it back into a generic status would
# tell an operator to chase a network problem for a missing configuration value.
for arm in ClientCredentialRequired ClientCredentialRejected ClientCredentialUnusable; do
  rg -q "    $arm," "$daemon/service_tunnels.rs" \
    || fail "EncryptedTargetStatus lost the $arm arm"
done
rg -Fq 'EncryptedLeaseSetError::ClientAuthorizationRequired' "$daemon/service_product.rs" \
  || fail "the required-but-absent case must be mapped from the address's own demand"

echo "encrypted-service consumer wiring OK"