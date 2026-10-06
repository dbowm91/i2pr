# Plan 351 registered this guard to hold the boundary its own work sits behind, while naming
# Plan 352 as the owner of the pre-existing config-secret leak paths. **Plan 352 has now landed.**
# Section 4 below was therefore inverted from "assert the hazard is still present" to "assert the
# fixed state", exactly as this header originally specified, so that reverting the fix fails here.
#
# ## Why this exists, and why it is narrower than a general secret audit
#
# Plan 351 adds an ELS2 **consumer** lookup secret to the daemon. Its Gate 3(a) is "no inline
# secret": the value arrives borrowed from the I2PControl control definition, is held only for
# the bounded lifetime of one resolve, and is never copied into a configuration struct.
#
# That rule is only meaningful because of a pre-existing hazard Plan 351's research measured.
# A TOML parse failure prints the entire offending source line
# (`toml-1.1.6/src/de/error.rs:140` does `writeln!(f, "{content}")`) and a type mismatch prints
# the value (`serde-1.0.228/src/core/de/mod.rs:410` does `write!(formatter, "string {:?}", s)`).
# Both chain through `ConfigError::Parse`, through the transparent `DaemonError::Config`, to
# `eprintln!("error: {error}")`. **Any** inline secret in the configuration file was printable to
# stderr.
#
# Plan 352 closed that, and closed the matching permission gap (`Config::load` was a bare
# `fs::read_to_string`, the only secret-bearing daemon file with no `& 0o077` gate). It asserts:
#
#   1. Plan 351's own contribution stays inside Gate 3(a) -- the consumer secret is not inline
#      anywhere, and cannot be reached through a config struct or a `DestinationRef`;
#   2. the container type stays safe -- `LookupSecret` keeps erase-on-drop semantics and gains no
#      `Debug`, `Display`, `Clone`, or serde;
#   3. the b33 failure surface stays closed -- `EncryptedTargetStatus` reasons are `&'static str`
#      and carry nothing dynamic;
#   4. the D1 redaction is real, not a message filter -- `ConfigError::Parse` no longer carries a
#      bare `toml::de::Error`, `RedactedTomlError::Display` cannot render the retained error, the
#      line/column diagnostic survives, and `Error::source` still retains full fidelity;
#   5. the D2 gate is real and conditional -- `& 0o077` on the load path, only when a password is
#      present, and a platform with no mode is refused rather than silently passed;
#   6. the D3 residual is recorded honestly -- `Config` is never serialized, and
#      `I2pControlPassword` keeps its redacting `Debug`. Its `Clone` derive is a **named
#      follow-on**, not a resolved defect, and this guard says so rather than asserting it away.
#
# The behavioural half lives in `crates/i2pr-daemon/tests/encrypted_service_consumer_wiring.rs`
# (Plan 351) and `crates/i2pr-daemon/tests/config_secret_hygiene.rs` (Plan 352).
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

# --- 4. the D1 fix: parse errors are position-only -----------------------------------------

# Before Plan 352, `ConfigError::Parse` held a bare `toml::de::Error`, whose `Display` renders the
# offending source *line* (`toml-1.1.6/src/de/error.rs:138`) and whose `message()` embeds the
# rejected key *and value* on a `deny_unknown_fields` failure. Section 4 used to assert those
# hazards were still present; Plan 352 inverted it, so this section now asserts the fixed state.
rg -q 'pub struct RedactedTomlError' "$daemon/config.rs" \
  || fail "RedactedTomlError is missing; Plan 352 owns the D1 fix"

# `ConfigError::Parse` must no longer carry the raw upstream type. This is the assertion that
# fails if someone reverts the wrapper and silently reintroduces the leak.
if rg -q 'Parse[(]#\[source\] toml::de::Error[)]' "$daemon/config.rs"; then
  fail "ConfigError::Parse must carry RedactedTomlError, not a bare toml::de::Error (D1 leak)"
fi
rg -q 'Parse[(]#\[source\] RedactedTomlError[)]' "$daemon/config.rs" \
  || fail "ConfigError::Parse must carry RedactedTomlError as its source (D1)"

# The redacting Display: scope to the impl block and require BOTH the absence of any
# interpolation of the retained upstream error AND the presence of the marker.
redacted_display="$(awk '/^impl std::fmt::Display for RedactedTomlError \{/{flag=1} flag{print} flag && /^}$/{exit}' \
  "$daemon/config.rs")"
[ -n "$redacted_display" ] || fail "RedactedTomlError has no Display impl"
if printf '%s\n' "$redacted_display" | rg -q 'self\.source|self\.0'; then
  fail "RedactedTomlError::Display must not render the retained toml error; that is the D1 leak"
fi
printf '%s\n' "$redacted_display" | rg -q 'source content redacted' \
  || fail "RedactedTomlError::Display must carry the redaction marker"
# The diagnostic must survive redaction. Without this, a fix that blanks the error entirely would
# satisfy every assertion above.
printf '%s\n' "$redacted_display" | rg -q 'line \{line\}, column \{column\}' \
  || fail "RedactedTomlError::Display must keep the line/column diagnostic (D1 non-goal)"
rg -q 'fn position_of' "$daemon/config.rs" \
  || fail "the byte-span to line/column resolver is missing; redaction must not destroy diagnostics"

# Redaction must not become information loss: the upstream error stays reachable for a caller that
# legitimately wants it. Asserted positively so "delete the source" is not a silent simplification.
redacted_error_impl="$(awk '/^impl std::error::Error for RedactedTomlError \{/{flag=1} flag{print} flag && /^}$/{exit}' \
  "$daemon/config.rs")"
[ -n "$redacted_error_impl" ] || fail "RedactedTomlError must implement Error"
printf '%s\n' "$redacted_error_impl" | rg -q 'Some\(&self\.source\)' \
  || fail "RedactedTomlError::source must retain the upstream toml error for programmatic callers"

# The retained source is only safe while nothing walks the source chain to render it.
# The pattern must match the *alternate/pretty flag on a field* (`{error:#}`), not a literal
# `{:#}`: an earlier draft of this assertion searched for the latter and therefore matched
# nothing, so adding a chain-walking printer passed. Verified by mutation N5.
if rg -n '\{[A-Za-z_][A-Za-z0-9_]*:[^}"]*\}' "$daemon/main.rs" "$daemon/error.rs"; then
  fail "a chain-walking error printer was added to the config path; it would re-render the retained toml error (D1)"
fi

# --- 5. the D2 fix: a secret-bearing config is permission-gated -------------------------------

rg -q 'pub fn secret_file_permission_verdict' "$daemon/config.rs" \
  || fail "the platform-independent permission verdict is missing (D2)"
verdict="$(awk '/^pub fn secret_file_permission_verdict/{flag=1} flag{print} flag && /^}$/{exit}' \
  "$daemon/config.rs")"
[ -n "$verdict" ] || fail "secret_file_permission_verdict has no body"
printf '%s\n' "$verdict" | rg -q 'mode & 0o077 != 0' \
  || fail "the permission gate must use the & 0o077 idiom used by every other secret file (D2)"
printf '%s\n' "$verdict" | rg -q 'None => Err' \
  || fail "a platform with no POSIX mode must be refused, not silently passed (D2)"

# The gate must be a real gate on the load path, and conditional on a secret existing. An
# unconditional gate would be a blanket permission tightening that rejects ordinary configs.
load_fn="$(awk '/    pub fn load\(path: &Path\)/{flag=1} flag{print} flag && /^    }$/{exit}' \
  "$daemon/config.rs")"
[ -n "$load_fn" ] || fail "Config::load was not found; its shape changed, re-anchor this guard"
# Pin the *call*, not a mere mention: mutation N9 replaced the call with a bare
# `let _ = check_secret_file_permissions;` reference and this assertion passed.
printf '%s\n' "$load_fn" | rg -q 'check_secret_file_permissions\(path\)\?' \
  || fail "Config::load must call the permission gate with `?` (D2)"
printf '%s\n' "$load_fn" | rg -q 'if !config\.i2pcontrol\.password\.is_empty\(\)' \
  || fail "the gate must be conditional on a password being present; unconditional would reject secret-free configs (D2)"

# The refusal must name the path and the mode, and must carry no file content.
rg -q 'InsecureConfigPermissions \{' "$daemon/config.rs" \
  || fail "the group/world-readable refusal variant is missing (D2)"
rg -q 'expected 0600' "$daemon/config.rs" \
  || fail "the refusal must state the required mode (D2)"

# --- 6. the D3 residual: Config stays opaque --------------------------------------------------

config_line="$(rg -n '^pub struct Config [{]' "$daemon/config.rs" | head -1 | cut -d: -f1)"
[ -n "$config_line" ] || fail "the Config struct is missing"
# Walk *up* to the nearest `#[derive` rather than reading the single line above: a doc comment
# sits between the derive and the struct, so `line - 1` captured the comment and the Serialize
# assertion was vacuous. Verified by mutation N11.
config_derive="$(awk -v end="$config_line" 'NR < end && /#\[derive/ { line = $0 } END { print line }' \
  "$daemon/config.rs")"
[ -n "$config_derive" ] || fail "the Config derive line is missing"
config_struct="$config_derive
$(awk 'NR >= '"$config_line"' { print } NR > '"$config_line"' && /^}$/ { exit }' "$daemon/config.rs")"
[ -n "$config_struct" ] || fail "the Config struct is missing"
if printf '%s\n' "$config_struct" | rg -q 'Serialize|Deserialize'; then
  fail "Config must never be serialized; a secret config would be written back verbatim (D3)"
fi
# The top-level error sink is unchanged, so D1's rendering is still the only path.
if ! rg -q 'eprintln![(]"error: \{error\}"' "$daemon/main.rs"; then
  fail "the top-level config error sink changed; a new sink needs its own redaction review (D1)"
fi
# Plan 352 records, rather than removes, the derived Debug/Clone exposure: `I2pControlPassword` has a
# hand-written redacting Debug but still derives Clone, because a production service-spec closure
# clones the config that owns it. Removing that derive is a named follow-on, so this guard asserts
# the redaction and refuses a Debug that stops redacting -- it does not pretend the Clone is gone.
rg -q 'impl std::fmt::Debug for I2pControlPassword' "$daemon/config.rs" \
  || fail "I2pControlPassword must keep a hand-written redacting Debug (D3)"
rg -q 'I2pControlPassword\(\[redacted\]\)' "$daemon/config.rs" \
  || fail "I2pControlPassword's Debug must keep emitting the redaction marker (D3)"
if rg -n 'eprintln![^(]*\{[^{}]*password[^{}]*\}|println![^(]*\{[^{}]*password[^{}]*\}' "$daemon"; then
  fail "a config password is being printed directly (D1/D3)"
fi

echo "config secret hygiene OK"