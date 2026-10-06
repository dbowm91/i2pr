#!/usr/bin/env bash
# Plan 351 static guard: configuration secret hygiene, at the boundary Plan 351 owns.
#
# ## Why this exists, and why it is narrower than Plan 352
#
# Plan 351 adds an ELS2 **consumer** lookup secret to the daemon. Its Gate 3(a) is "no inline
# secret": the value arrives borrowed from the I2PControl control definition, is held only for
# the bounded lifetime of one resolve, and is never copied into a configuration struct.
#
# That rule is only meaningful because of a pre-existing hazard Plan 351's research measured.
# A TOML parse failure prints the entire offending source line
# (`toml-1.1.6/src/de/error.rs:138` does `writeln!(f, "{content}")`) and a type mismatch prints
# the value (`serde-1.0.228/src/core/de/mod.rs:410` does `write!(formatter, "string {:?}", s)`).
# Both chain through `ConfigError::Parse`, through the transparent `DaemonError::Config`, to
# `eprintln!("error: {error}")`. **Any** inline secret in the configuration file is printable to
# stderr today.
#
# Plan 352 is the plan that *fixes* that. This guard deliberately does not assert the fixed state,
# because asserting it now would put a red check into the `AGENTS.md` floor for a plan that has
# not run. It asserts instead:
#
#   1. Plan 351's own contribution stays inside Gate 3(a) — the consumer secret is not inline
#      anywhere, and cannot be reached through a config struct or a `DestinationRef`;
#   2. the container type stays safe — `LookupSecret` keeps erase-on-drop semantics and gains no
#      `Debug`, `Display`, `Clone`, or serde;
#   3. the b33 failure surface stays closed — `EncryptedTargetStatus` reasons are `&'static str`
#      and carry nothing dynamic;
#   4. **the hazard is still recorded**. The pre-existing leak paths must still be present, and
#      the guard names Plan 352 as their owner.
#
# Section 4 is what makes this script bidirectional. If D1/D2/D3 are fixed out of band, this
# guard fails and forces the Plan 352 record to be updated, instead of letting a security fix
# land as an undocumented drift. When Plan 352 lands, this section is deleted and its three
# assertions move to their "fixed state" form.
#
# The behavioural half lives in `crates/i2pr-daemon/tests/encrypted_service_consumer_wiring.rs`.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
daemon="$root/crates/i2pr-daemon/src"
tunnels="$root/crates/i2pr-service-tunnels/src"
crypto="$root/crates/i2pr-crypto/src"

fail() {
  echo "config secret hygiene: $*" >&2
  exit 1
}

# --- 1. the consumer lookup secret is not inline anywhere ----------------------------------

# `Config` and every `Raw*Config` struct are parsed from TOML, so a field there is a field whose
# parse failure prints it to stderr.
if rg -n 'leaseset_password|LookupSecret' "$daemon/config.rs"; then
  fail "an ELS2 lookup secret must never appear in Config or a Raw*Config struct (Plan 351 Gate 3a)"
fi

# `DestinationRef` is `Clone + Debug + PartialEq` and is stored in committed specs. A secret in a
# variant would be cloned into specs, printed by any `Debug`, and compared by `PartialEq`.
ref_impl="$(awk '/^pub enum DestinationRef \{/{flag=1} flag{print} flag && /^}$/{exit}' \
  "$tunnels/destination.rs")"
[ -n "$ref_impl" ] || fail "the DestinationRef enum is missing"
if printf '%s\n' "$ref_impl" | rg -qi 'secret|password|key_material|LookupSecret'; then
  fail "DestinationRef must not carry secret material; the address is public and the secret is not"
fi

# The secret must reach the resolver as a borrow, from a dedicated registry, not from a value
# copied into the spec.
rg -q 'install_encrypted_target_secret' "$daemon/i2pcontrol_tunnels.rs" \
  || fail "the consumer secret must be installed from the I2PControl definition reconciliation"
rg -q 'Arc<i2pr_crypto::red25519::LookupSecret>' "$daemon/service_tunnels.rs" \
  || fail "the manager must hold the consumer secret as Arc<LookupSecret>, not a copy"
# The field's own type, pinned. A `String` in a `HashMap<String, _>` key is a spec id and is
# fine, so the check is on the value type rather than on the token.
registry="$(awk '/^    encrypted_target_secrets:/{flag=1} flag{print} flag && /,$/{exit}' \
  "$daemon/service_tunnels.rs")"
[ -n "$registry" ] || fail "the encrypted_target_secrets registry declaration is missing"
registry_value="$(printf '%s\n' "$registry" | tr '\n' ' ' | sed 's/.*encrypted_target_secrets: *//; s/ *, *$//; s/  */ /g')"
[ "$registry_value" = 'Mutex<HashMap<String, Arc<i2pr_crypto::red25519::LookupSecret>>>' ] \
  || fail "the consumer secret registry must hold Arc<LookupSecret> and nothing else, found: $registry_value"
# `LookupSecret::as_option()` is the single accessor that hands out a `&str`, and `as_str()` is
# refused on the type. Pinning the *call sites* across the whole daemon is what actually closes
# Gate 3(a): any second caller would be a second copy-out, wherever it lives.
# `.as_option()` is the only accessor that hands out a `&str`, so pinning its call sites is what
# actually closes Gate 3(a): a second caller anywhere in the daemon is a second copy-out, and the
# receiver name varies (`held`, `h`, …), so the search is on the method, not on the binding.
copyout_sites="$(rg -n -F '.as_option()' "$daemon" || true)"
copyout_count="$(printf '%s' "$copyout_sites" | grep -c . || true)"
if [ "$copyout_count" != "1" ]; then
  fail "exactly one site may borrow the consumer secret as &str (found $copyout_count): $copyout_sites"
fi
case "$copyout_sites" in
  *"$daemon/service_product.rs"*) ;;
  *) fail "the single secret borrow site must be the resolve path in service_product.rs: $copyout_sites" ;;
esac


# The resolve path must pass `Option<&str>` down to the owner, never an owned string.
rg -q 'let secret_option = secret.as_ref\(\).and_then\(\|held\| held.as_option\(\)\);' \
  "$daemon/service_product.rs" \
  || fail "the resolve path must borrow the secret as Option<&str>, not copy it into a String"

# --- 2. the container stays a secret ------------------------------------------------------

# `LookupSecret` is the only type allowed to hold this value. It is `Zeroizing<String>` with no
# `Clone`, no serde, and a redacted `Debug`. Any of those returning reopens the leak one layer
# up, because the value is already inside a `Debug`-able `Arc`.
# The derive line sits above the struct, so it has to be captured explicitly: scoping the awk
# only to the struct body would make "LookupSecret grew a Clone" invisible to this guard.
secret_line="$(rg -n '^pub struct LookupSecret[(]' "$crypto/red25519.rs" | head -1 | cut -d: -f1)"
[ -n "$secret_line" ] || fail "the LookupSecret newtype is missing"
# Read the immediately preceding line by exact adjacency. An awk `prev` carry-over is wrong here:
# `LookupSecret` derives nothing today, so a carried `prev` would hold an unrelated struct's
# derive and this guard would fail on the unmodified tree.
secret_derive="$(sed -n "$((secret_line - 1))p" "$crypto/red25519.rs")"
secret_struct="$secret_derive
$(awk 'NR >= '"$secret_line"' { print } NR > '"$secret_line"' && /^}$/ { exit }' "$crypto/red25519.rs")"
printf '%s\n' "$secret_struct" | rg -q 'Zeroizing<String>' \
  || fail "LookupSecret must keep erase-on-drop semantics"
derive_line="$(printf '%s\n' "$secret_struct" | rg -n 'derive' || true)"
if [ -n "$derive_line" ]; then
  if printf '%s\n' "$derive_line" | rg -q 'Clone|Serialize|Deserialize'; then
    fail "LookupSecret must not derive Clone or any serde trait"
  fi
fi
if printf '%s\n' "$secret_struct" | rg -q 'pub fn as_str|pub fn expose|pub fn into_string'; then
  fail "LookupSecret must expose only as_option()/is_empty()/len(); as_str() invites a String copy"
fi

# --- 3. the b33 failure surface stays closed -----------------------------------------------

# A `String` reason here would be able to carry a foreign error message, and a foreign error
# message is exactly what Plan 352 shows can contain configuration bytes.
status_enum="$(awk '/^pub enum EncryptedTargetStatus \{/{flag=1} flag{print} flag && /^}$/{exit}' \
  "$daemon/service_tunnels.rs")"
[ -n "$status_enum" ] || fail "the EncryptedTargetStatus enum is missing"
if printf '%s\n' "$status_enum" | rg -q 'String|\bVec<'; then
  fail "EncryptedTargetStatus must carry no dynamic data; every field is a unit variant"
fi
reason_impl="$(awk '/    pub const fn reason\(self\) -> &.static str \{/{flag=1} flag{print} flag && /^    \}$/{exit}' \
  "$daemon/service_tunnels.rs")"
[ -n "$reason_impl" ] || fail "EncryptedTargetStatus::reason must return &'static str"
reason_count="$(printf '%s\n' "$reason_impl" | rg -c '=> "' || true)"
variant_count="$(printf '%s\n' "$status_enum" | rg -c '^    [A-Z]' || true)"
if [ "$reason_count" != "$variant_count" ]; then
  fail "every EncryptedTargetStatus variant needs a reason string ($variant_count variants, $reason_count reasons)"
fi
if printf '%s\n' "$reason_impl" | rg -q 'to_string|format!'; then
  fail "EncryptedTargetStatus::reason must be a literal per variant, never a formatted string"
fi

# --- 4. the pre-existing hazard is still recorded, and still owned -------------------------

# These four checks are the guard's reason for existing in the other direction. They assert the
# leak paths are still present. When Plan 352 fixes D1/D2/D3 this section is replaced by the
# "fixed state" assertions, and the guard's name is unchanged.
if ! rg -q 'ConfigError::Parse' "$daemon/config.rs"; then
  fail "ConfigError::Parse disappeared; Plan 352 owns the D1 fix — update this guard, do not leave it undocumented"
fi
# `{0}` is the interpolation that leaks. A Plan 352 fix that drops it must fail this guard so
# the plan record is updated, not silently drifted past.
if ! rg -Fq 'configuration parse failed: {0}' "$daemon/config.rs"; then
  fail "the ConfigError::Parse display string changed; Plan 352 owns the D1 fix"
fi
config_line="$(rg -n '^pub struct Config [{]' "$daemon/config.rs" | head -1 | cut -d: -f1)"
[ -n "$config_line" ] || fail "the Config struct is missing"
config_derive="$(sed -n "$((config_line - 1))p" "$daemon/config.rs")"
config_struct="$config_derive
$(awk 'NR >= '"$config_line"' { print } NR > '"$config_line"' && /^}$/ { exit }' "$daemon/config.rs")"
[ -n "$config_struct" ] || fail "the Config struct is missing"
if ! printf '%s\n' "$config_struct" | rg -q 'Debug'; then
  fail "Config no longer derives Debug; Plan 352 owns the D3 fix — update this guard"
fi
if ! rg -q 'eprintln![(]"error: \{error\}"' "$daemon/main.rs"; then
  fail "the top-level config error sink changed; Plan 352 owns the D1 fix"
fi

echo "config secret hygiene OK"