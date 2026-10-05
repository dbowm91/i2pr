#!/usr/bin/env bash
# Plan 346 static guard: the ELS2 type-11 signature-profile boundary.
#
# The correction in Plan 346 is only safe while the deployed Java/i2pd transcript stays
# confined to the encrypted-LeaseSet2 type-5 owner. Three things would silently undo it, and
# none of them would fail a test on their own:
#
#   1. a generic "try both type-11 transcripts" verifier in the common signature layer, which
#      would downgrade every other type-11 verification in the router;
#   2. the strict primitive changing, which would invalidate every official Red25519 vector as
#      evidence;
#   3. a caller being able to hand the ELS2 signer or verifier an arbitrary byte slice, which
#      would undo the compensating control for the deployed transcript's missing domain
#      separator and length framing.
#
# This checker is the static half of those three claims. The behavioural half lives in
# `crates/i2pr-crypto/tests/red25519_deployed_els2_profile.rs` and
# `crates/i2pr-netdb/tests/els2_type11_profile.rs`.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
crypto="$root/crates/i2pr-crypto/src"
netdb="$root/crates/i2pr-netdb/src"
proto="$root/crates/i2pr-proto/src/common/els2.rs"
deployed="$crypto/red25519_deployed.rs"
profile="$netdb/els2_transcript.rs"

fail() {
  echo "ELS2 type-11 transcript boundary: $*" >&2
  exit 1
}

# --- 1. the common signature layer has no type-11 path, deployed or otherwise -------------

# `verify_signature` must keep refusing anything that is not the router signing type. If a
# type-11 verifier is ever added here, every type-11 verification in the router changes
# meaning at once.
rg -q 'public_key[.]key_type[(][)] != ROUTER_SIGNING_KEY_TYPE' "$crypto/lib.rs" \
  || fail "i2pr_crypto::verify_signature lost its non-type-7 guard"

# The module may be declared, but nothing at the crate root may re-export or call it: the only
# path to the deployed transcript is a direct `i2pr_crypto::red25519_deployed::…` use from the
# ELS2 owner, so a future convenience re-export cannot widen its reach silently.
rg -q '^pub mod red25519_deployed;$' "$crypto/lib.rs" \
  || fail "the deployed transcript module is not declared in i2pr-crypto"
if rg -n 'red25519_deployed' "$crypto/lib.rs" | rg -qv '^[0-9]+:pub mod red25519_deployed;$'; then
  fail "the i2pr-crypto root must not re-export or call the deployed transcript; only the ELS2 owner may"
fi
if sed -n '/^pub fn verify_signature/,/^}/p' "$crypto/lib.rs" | rg -n 'RedDsa|red25519'; then
  fail "the common signature layer must not reference any Red25519 transcript"
fi

# `red25519.rs` (strict) must not reach into the deployed module, and the deployed module must
# not redefine the strict one. The two are peers, not a chain.
if rg -n 'red25519_deployed' "$crypto/red25519.rs"; then
  fail "the strict primitive must not depend on the deployed transcript"
fi
if rg -n 'HSTAR_PREFIX' "$deployed"; then
  fail "the deployed transcript must not use the Proposal-146 hash-domain prefix"
fi

# The strict module keeps its published surface byte-for-byte.
rg -q 'pub const HSTAR_PREFIX: &\[u8\] = b"I2P_Red25519H\(x\)";' "$crypto/red25519.rs" \
  || fail "HSTAR_PREFIX changed; the strict primitive must stay byte-exact"
rg -q 'hasher[.]update\(length[.]to_le_bytes[(][)]\);' "$crypto/red25519.rs" \
  || fail "the two-byte little-endian message-length framing changed in the strict primitive"

# --- 2. the ELS2 owner is the only consumer of the deployed transcript --------------------

# The deployed transcript must be imported by exactly one production module: the ELS2 profile
# owner. The client reaches it through that owner, not around it.
producers="$(rg -l 'red25519_deployed' --glob '*.rs' "$root/crates" \
  | grep -v '/tests/' | grep -v '/i2pr-crypto/src/lib.rs$' | sort)"
expected="$profile"
[ "$producers" = "$expected" ] \
  || fail "the deployed transcript must be used by exactly the ELS2 profile owner, found: ${producers//$'\n'/, }"

# The client must go through the bounded ELS2 owner.
rg -q 'sign_type11_deployed' "$root/crates/i2pr-client/src/encrypted_leaseset.rs" \
  || fail "the ELS2 publisher must sign through the bounded owner (sign_type11_deployed)"
if rg -n 'red25519::sign\b|red25519::sign\(' "$root/crates/i2pr-client/src/encrypted_leaseset.rs"; then
  fail "the ELS2 publisher must not sign type-5 records with the strict primitive"
fi

# --- 3. the signed region is not a byte slice ---------------------------------------------

# `Els2SignedRegion` must be constructible only from a decoded type-5 record or offline-key
# block. A byte-slice constructor would let any caller supply any message to the deployed
# transcript, which is the one compensating control it genuinely needs.
rg -q 'pub struct Els2SignedRegion' "$proto" || fail "Els2SignedRegion is missing"
rg -q 'pub fn of_record' "$proto" || fail "Els2SignedRegion::of_record is missing"
rg -q 'pub fn of_offline_keys' "$proto" || fail "Els2SignedRegion::of_offline_keys is missing"

# Scope the constructor check to the type's own impl block, so unrelated `new` functions in the
# file cannot mask a real one and a real one cannot hide behind them.
region_impl="$(awk '/^impl<.a> Els2SignedRegion<.a> \{/{flag=1} flag{print} flag && /^\}/{exit}' "$proto")"
[ -n "$region_impl" ] || fail "the Els2SignedRegion impl block is missing"
if printf '%s\n' "$region_impl" | rg -n 'fn (new|from_bytes|from_slice|from_raw|as_mut|as_mut_slice)'; then
  fail "Els2SignedRegion must not expose a byte-slice constructor"
fi
printf '%s\n' "$region_impl" | rg -q 'pub const fn as_bytes' \
  || fail "Els2SignedRegion must expose an immutable borrow only"
if printf '%s\n' "$region_impl" | rg -q 'as_bytes_mut|&mut '; then
  fail "Els2SignedRegion must not expose mutable access to its region"
fi
if rg -n 'Els2SignedRegion[(][^)]' "$proto" "$netdb" "$root/crates/i2pr-client/src"; then
  fail "Els2SignedRegion's inner field must stay private to i2pr-proto"
fi

# The ELS2 profile API must take the region, never a bare message.
rg -q 'region: Els2SignedRegion' "$profile" || fail "the ELS2 profile API must take a typed region"
if rg -n 'message: &\[u8\]' "$profile"; then
  fail "the ELS2 profile API must not accept a caller-supplied message"
fi

# --- 4. fail-closed shape -----------------------------------------------------------------

rg -q 'pub enum Els2Type11Profile' "$profile" || fail "Els2Type11Profile is missing"
for state in Deployed Strict None Ambiguous; do
  rg -q "Self::$state" "$profile" || fail "Els2Type11Profile must carry the $state state"
done
rg -q 'Type11SignatureRejected' "$netdb/els2.rs" \
  || fail "the ELS2 validator must fail closed on a non-accepting type-11 outcome"

# An ambiguous match must never be an accepting state, checked structurally rather than by
# trusting the match arms.
rg -Fq 'matches!(self, Self::Deployed | Self::Strict)' "$profile" \
  || fail "only Deployed and Strict may be accepting states; Ambiguous must not be"

echo "ELS2 type-11 transcript boundary OK"
