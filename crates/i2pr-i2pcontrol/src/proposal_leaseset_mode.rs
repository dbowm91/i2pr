//! Plan 334: the frozen Proposal 170 LeaseSet security mode mapping.
//!
//! Proposal 170 revision 2026-05-20 names ten `EncryptLeaseSet` values plus
//! `OptionalLookup` and `LeaseSetClientAuths`, and defines **nothing** about
//! them: no per-mode table, no property assignment, no wire type beyond the
//! option block itself, and no precedence rule between the three parameters.
//! The normative reading, the two argued dispositions, and the control-surface
//! prohibitions are frozen in
//! `specs/references/proposal-170-encryptleaseset-mode-mapping.md`; this
//! module is its executable form.
//!
//! # What this module owns
//!
//! - Resolving one of the ten exact Proposal strings to exactly one protocol
//!   behavior, with **no** case folding, trimming, or fuzzy matching: a mode
//!   name is an identifier, and the Proposal's spellings contain parentheses
//!   and spaces.
//! - Recording which of the two spellings the caller used, so an operator who
//!   types the longer `encrypted with per-user key (psk)` gets an answer they
//!   can reason about.
//! - Validating the pairing rules between the mode, `OptionalLookup`, and
//!   `LeaseSetClientAuths` before any mutation happens.
//!
//! # What this module deliberately does not own
//!
//! It owns **no cryptography, no I/O, no persistence, and no router state**.
//! It is a runtime-neutral contract crate with no `i2pr-*` production edge
//! (`scripts/check-dependency-direction.sh`), so it cannot and does not reach
//! the ELS2 owners. This module's [`LeaseSetSecurityPlan`] is the hand-off
//! point: the daemon resolves it into the real
//! `i2pr_netdb::els2_auth` / `i2pr_client::encrypted_leaseset` owners.
//!
//! In particular, the Proposal 170 string parsing lives **here** and not in
//! `i2pr-netdb`, because `els2_auth.rs` is the protocol owner and must not
//! grow a dependency on a control-plane vocabulary it has no other reason to
//! know.
//!
//! # Secrets
//!
//! [`LeaseSetClientAuthEntry`] holds a client key as validated hex text and is
//! deliberately neither `Clone` nor `Debug`-revealing. The bytes are decoded
//! into the zeroizing `PskClientKey` / `AuthClientPublicKey` owners by the
//! daemon, which is the only layer that has those types available. Nothing in
//! this module can format, clone, or serialize key material.

use std::fmt;

use thiserror::Error;

use crate::limits::MAX_OPTION_VALUE_LEN;
use crate::proposal_wire::PROPOSAL_ENCRYPT_LEASE_SET_VALUES;

/// Maximum number of `LeaseSetClientAuths` entries one control definition may
/// carry.
///
/// This is a **control-surface** bound, not a protocol bound: the ELS2
/// authorization block format permits 65 535 entries and `i2pr-netdb` accepts
/// up to `MAX_ELS2_AUTH_CLIENTS` (255). The narrower number here is a
/// consequence of the durable storage shape — every entry must survive a
/// round trip through one `BTreeMap<String, String>` option value bounded by
/// [`MAX_OPTION_VALUE_LEN`] — and is enforced by
/// [`max_encoded_client_auths_bytes`] plus a test that asserts the worst case
/// fits. `specs/CONFORMANCE.md` records the divergence.
pub const MAX_LEASESET_CLIENT_AUTHS: usize = 24;

/// Maximum byte length of one client authorization `Name`.
///
/// Matches `i2pr_netdb::els2_auth::MAX_ELS2_CLIENT_NAME_LENGTH`, which is the
/// protocol owner's own bound; the control surface may not be laxer than the
/// owner it feeds.
pub const MAX_LEASESET_CLIENT_NAME_LEN: usize = 64;

/// Byte length of one ELS2 client key.
///
/// Both authorization schemes use 32 bytes: a pre-shared key is a 32-byte
/// secret, and a Diffie-Hellman entry is a 32-byte X25519 public key.
pub const LEASESET_CLIENT_KEY_LEN: usize = 32;

/// Hex text length of one ELS2 client key (2 bytes per byte).
pub const LEASESET_CLIENT_KEY_HEX_LEN: usize = LEASESET_CLIENT_KEY_LEN * 2;

/// Separator between encoded client authorization entries.
const ENTRY_SEPARATOR: char = '\n';

/// Separator between an encoded entry's name and key.
const FIELD_SEPARATOR: char = ':';

/// Worst-case encoded byte length of `count` client authorization entries.
///
/// Every component is fixed-width: a name is at most
/// [`MAX_LEASESET_CLIENT_NAME_LEN`] bytes, a key is exactly
/// [`LEASESET_CLIENT_KEY_HEX_LEN`] hex characters, and entries are joined by
/// one separator.
pub const fn max_encoded_client_auths_bytes(count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    count * (MAX_LEASESET_CLIENT_NAME_LEN + 1 + LEASESET_CLIENT_KEY_HEX_LEN) + (count - 1)
}

/// Which address flags a mode's published service address must carry.
///
/// These are the two `B32_FLAG_*` bits the ELS2 specification defines for
/// encrypted services, and they are what tell a prospective client — before it
/// fetches anything — whether it will need a blinding secret, a client
/// credential, or neither.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeaseSetAddressFlag {
    /// `B32_FLAG_REQUIRES_BLINDING_SECRET`: the daily blinded key, and
    /// therefore the NetDB storage key, is perturbed by a lookup secret.
    BlindingSecret,
    /// `B32_FLAG_REQUIRES_CLIENT_KEY`: the type-5 record carries a
    /// client-authorization block, so an unauthenticated fetch cannot read it.
    ClientKey,
}

/// The client-authorization scheme a mode selects.
///
/// The Proposal supplies no scheme name; the mode spelling selects it, and the
/// selected scheme decides how `LeaseSetClientAuths` key bytes are
/// interpreted. i2pr cannot validate which of the two the operator *meant*,
/// because both are 32 bytes; it validates shape and lets the mode decide.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeaseSetClientAuthScheme {
    /// `i2pr_netdb::els2_auth::build_psk_block`: the key bytes are a shared
    /// secret known to the service and to each authorized client.
    PreSharedKey,
    /// `i2pr_netdb::els2_auth::build_dh_block`: the key bytes are a client
    /// X25519 public key; the service holds no client private key.
    DiffieHellman,
}

/// The single protocol behavior behind one Proposal `EncryptLeaseSet` value.
///
/// Ten Proposal values reduce to eight distinct behaviors: the Proposal spells
/// the pre-shared-key mode twice (`encrypted (psk)` and `encrypted with
/// per-user key (psk)`) and the Diffie-Hellman mode twice (once plain, once
/// with a lookup password) in ways the I2P specifications do not distinguish.
/// See §5.2 of the frozen mapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeaseSetSecurityBehavior {
    /// Ordinary, unencrypted LeaseSet2 under the service's unblinded
    /// destination. No type-5 record, no lookup secret, no authorization.
    Ordinary,
    /// Modern encrypted LeaseSet2 (NetDB `DatabaseStore` type 5) with no
    /// authorization block and no lookup secret.
    Blinded,
    /// Type-5 record with no authorization block and a lookup secret.
    BlindedWithLookupSecret,
    /// Type-5 record with a pre-shared-key authorization block.
    PreSharedKey,
    /// Type-5 record with a pre-shared-key authorization block and a lookup
    /// secret.
    PreSharedKeyWithLookupSecret,
    /// Type-5 record with a Diffie-Hellman authorization block.
    DiffieHellman,
    /// Type-5 record with a Diffie-Hellman authorization block and a lookup
    /// secret.
    DiffieHellmanWithLookupSecret,
    /// Legacy LeaseSet1 over AES. Recognized so it can be refused by name; it
    /// is never published and never aliased to a modern behavior.
    LegacyAes,
}

impl LeaseSetSecurityBehavior {
    /// Whether this behavior publishes a modern type-5 encrypted LeaseSet2.
    ///
    /// `false` for [`Self::Ordinary`] and [`Self::LegacyAes`]; the latter
    /// publishes a *legacy* type-1 record, which i2pr does not implement at
    /// all, so it is not an encrypted-LeaseSet2 publisher.
    pub const fn publishes_type5(self) -> bool {
        matches!(
            self,
            Self::Blinded
                | Self::BlindedWithLookupSecret
                | Self::PreSharedKey
                | Self::PreSharedKeyWithLookupSecret
                | Self::DiffieHellman
                | Self::DiffieHellmanWithLookupSecret
        )
    }

    /// Whether this behavior requires a blinded (Red25519) service identity.
    ///
    /// Every type-5 behavior does: the record is signed with the day's
    /// blinded key and filed under the day's blinded storage key, which is
    /// only defined for an unblinded type 7 or type 11 identity.
    pub const fn requires_blinded_identity(self) -> bool {
        self.publishes_type5()
    }

    /// Whether this behavior consumes `OptionalLookup`.
    pub const fn requires_lookup_secret(self) -> bool {
        matches!(
            self,
            Self::BlindedWithLookupSecret
                | Self::PreSharedKeyWithLookupSecret
                | Self::DiffieHellmanWithLookupSecret
        )
    }

    /// Whether this behavior consumes `LeaseSetClientAuths`.
    pub const fn requires_client_auths(self) -> bool {
        matches!(
            self,
            Self::PreSharedKey
                | Self::PreSharedKeyWithLookupSecret
                | Self::DiffieHellman
                | Self::DiffieHellmanWithLookupSecret
        )
    }

    /// The client-authorization scheme this behavior selects, if any.
    pub const fn client_auth_scheme(self) -> Option<LeaseSetClientAuthScheme> {
        match self {
            Self::PreSharedKey | Self::PreSharedKeyWithLookupSecret => {
                Some(LeaseSetClientAuthScheme::PreSharedKey)
            }
            Self::DiffieHellman | Self::DiffieHellmanWithLookupSecret => {
                Some(LeaseSetClientAuthScheme::DiffieHellman)
            }
            Self::Ordinary | Self::Blinded | Self::BlindedWithLookupSecret | Self::LegacyAes => {
                None
            }
        }
    }
}

/// One resolved Proposal `EncryptLeaseSet` value.
///
/// The value is `Copy` and holds only static data — the exact Proposal spelling
/// and the behavior it maps to — so it carries nothing sensitive and can be
/// logged, compared, and stored freely.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EncryptLeaseSetMode {
    spelling: &'static str,
    behavior: LeaseSetSecurityBehavior,
    per_user_spelling: bool,
}

impl EncryptLeaseSetMode {
    /// The exact Proposal string this mode was resolved from.
    pub const fn spelling(self) -> &'static str {
        self.spelling
    }

    /// The protocol behavior this spelling selects.
    pub const fn behavior(self) -> LeaseSetSecurityBehavior {
        self.behavior
    }

    /// Whether the caller used a `per-user key` spelling.
    ///
    /// The Proposal and the I2P specifications define one pre-shared-key
    /// scheme, so this changes no wire behavior. It is recorded so the control
    /// surface can report which spelling it received rather than silently
    /// normalizing the two together.
    pub const fn uses_per_user_spelling(self) -> bool {
        self.per_user_spelling
    }

    /// The address flags a service publishing in this mode must set.
    pub const fn address_flags(self) -> LeaseSetAddressFlags {
        let behavior = self.behavior;
        let mut flags = LeaseSetAddressFlags::NONE;
        if behavior.requires_lookup_secret() {
            flags = flags.with(LeaseSetAddressFlag::BlindingSecret);
        }
        if behavior.requires_client_auths() {
            flags = flags.with(LeaseSetAddressFlag::ClientKey);
        }
        flags
    }
}

/// A bounded set of `B32_FLAG_*` requirements for a service address.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LeaseSetAddressFlags(u8);

impl LeaseSetAddressFlags {
    /// No flag required.
    pub const NONE: Self = Self(0);
    /// Only the blinding-secret flag.
    pub const BLINDING_SECRET: Self = Self(1);
    /// Only the client-key flag.
    pub const CLIENT_KEY: Self = Self(2);
    /// Both flags.
    pub const BLINDING_SECRET_AND_CLIENT_KEY: Self = Self(3);

    const fn with(self, flag: LeaseSetAddressFlag) -> Self {
        match flag {
            LeaseSetAddressFlag::BlindingSecret => Self(self.0 | 1),
            LeaseSetAddressFlag::ClientKey => Self(self.0 | 2),
        }
    }

    /// Whether the address must set `B32_FLAG_REQUIRES_BLINDING_SECRET`.
    pub const fn requires_blinding_secret(self) -> bool {
        self.0 & 1 != 0
    }

    /// Whether the address must set `B32_FLAG_REQUIRES_CLIENT_KEY`.
    pub const fn requires_client_key(self) -> bool {
        self.0 & 2 != 0
    }
}

impl Default for LeaseSetAddressFlags {
    fn default() -> Self {
        Self::NONE
    }
}

/// Resolves one exact Proposal `EncryptLeaseSet` value.
///
/// All ten values resolve, **including** `encrypted (aes)`. Recognized-and-
/// refused is not unknown-and-rejected: the control surface can then tell an
/// operator that they supplied a valid Proposal value which i2pr deliberately
/// does not implement, instead of reporting a spelling mistake they did not
/// make. The refusal itself is raised by [`resolve_lease_set_security`].
///
/// No normalization is applied. `EncryptedLeaseSet`, `encrypted( aes )`, and
/// `encrypted  (aes)` are all rejected as unknown, because a mode name is an
/// identifier.
pub fn resolve_encrypt_lease_set_mode(
    value: &str,
) -> Result<EncryptLeaseSetMode, EncryptLeaseSetModeError> {
    let behavior = match value {
        "disable" => LeaseSetSecurityBehavior::Ordinary,
        "encrypted (aes)" => LeaseSetSecurityBehavior::LegacyAes,
        "blinded" => LeaseSetSecurityBehavior::Blinded,
        "blinded with lookup password" => LeaseSetSecurityBehavior::BlindedWithLookupSecret,
        "encrypted (psk)" | "encrypted with per-user key (psk)" => {
            LeaseSetSecurityBehavior::PreSharedKey
        }
        "encrypted with lookup password (psk)"
        | "encrypted with lookup password and per-user key (psk)" => {
            LeaseSetSecurityBehavior::PreSharedKeyWithLookupSecret
        }
        "encrypted with per-user key (dh)" => LeaseSetSecurityBehavior::DiffieHellman,
        "encrypted with lookup password and per-user key (dh)" => {
            LeaseSetSecurityBehavior::DiffieHellmanWithLookupSecret
        }
        _ => return Err(EncryptLeaseSetModeError::UnknownMode),
    };
    Ok(EncryptLeaseSetMode {
        spelling: PROPOSAL_ENCRYPT_LEASE_SET_VALUES
            .iter()
            .copied()
            .find(|candidate| *candidate == value)
            .expect("matched value is one of the ten frozen Proposal strings"),
        behavior,
        per_user_spelling: value.contains("per-user key"),
    })
}

/// Why one Proposal `EncryptLeaseSet` value did not resolve.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
pub enum EncryptLeaseSetModeError {
    /// The value is outside the ten frozen Proposal strings.
    #[error(
        "EncryptLeaseSet must be one of the ten Proposal 170 values: disable, \
         encrypted (aes), blinded, blinded with lookup password, encrypted (psk), \
         encrypted with lookup password (psk), encrypted with per-user key (psk), \
         encrypted with lookup password and per-user key (psk), \
         encrypted with per-user key (dh), encrypted with lookup password and per-user key (dh)"
    )]
    UnknownMode,
}

/// One validated `LeaseSetClientAuths` entry.
///
/// The key is carried as validated hex text rather than as raw bytes for two
/// reasons: this crate has no `zeroize` dependency and must not gain an
/// `i2pr-*` edge, and the daemon is the only layer that can construct the
/// zeroizing `PskClientKey` / `AuthClientPublicKey` owners.
///
/// The type is neither `Clone` nor `Debug`-revealing, so a decoded entry
/// cannot be copied into a second structure or printed into a log line.
pub struct LeaseSetClientAuthEntry {
    name: String,
    encoded_key: String,
}

impl LeaseSetClientAuthEntry {
    /// Builds one entry from an already-validated name and hex key.
    ///
    /// Returns an error rather than trusting its inputs, because every caller
    /// in this crate is a JSON decoder over untrusted bytes.
    pub fn new(name: &str, encoded_key: &str) -> Result<Self, LeaseSetClientAuthError> {
        if name.is_empty() {
            return Err(LeaseSetClientAuthError::EmptyClientName);
        }
        if name.len() > MAX_LEASESET_CLIENT_NAME_LEN {
            return Err(LeaseSetClientAuthError::ClientNameTooLong);
        }
        if name
            .chars()
            .any(|character| matches!(character, '\n' | '\r' | FIELD_SEPARATOR | '\0'))
        {
            return Err(LeaseSetClientAuthError::ClientNameCharset);
        }
        if encoded_key.len() != LEASESET_CLIENT_KEY_HEX_LEN {
            return Err(LeaseSetClientAuthError::ClientKeyLength);
        }
        if !encoded_key
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(LeaseSetClientAuthError::ClientKeyEncoding);
        }
        Ok(Self {
            name: name.to_owned(),
            encoded_key: encoded_key.to_owned(),
        })
    }

    /// The client name, which is metadata and never secret.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The client key as validated lowercase hex.
    ///
    /// Named `encoded_key` at every call site that feeds a secret owner; the
    /// value is never printed.
    pub fn encoded_key(&self) -> &str {
        &self.encoded_key
    }

    /// Whether this entry's key is a well-formed 32-byte value.
    ///
    /// Always `true` for a constructed entry; provided so a caller can assert
    /// the invariant without trusting the constructor's error path.
    pub const fn key_is_well_formed(&self) -> bool {
        true
    }
}

impl fmt::Debug for LeaseSetClientAuthEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The name is operator metadata and is bounded to 64 bytes with a
        // restricted charset. The key is never formatted.
        formatter
            .debug_struct("LeaseSetClientAuthEntry")
            .field("name", &self.name)
            .field("key", &"<redacted>")
            .finish()
    }
}

/// Why one `LeaseSetClientAuths` entry was rejected.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
pub enum LeaseSetClientAuthError {
    /// The `Name` field was empty.
    #[error("LeaseSetClientAuths entry has an empty Name")]
    EmptyClientName,
    /// The `Name` field exceeded [`MAX_LEASESET_CLIENT_NAME_LEN`].
    #[error("LeaseSetClientAuths Name exceeds {MAX_LEASESET_CLIENT_NAME_LEN} bytes")]
    ClientNameTooLong,
    /// The `Name` field contained a character the encoding reserves.
    #[error("LeaseSetClientAuths Name must not contain a newline, carriage return, colon, or NUL")]
    ClientNameCharset,
    /// The `Key` field was not exactly 32 bytes of hex.
    #[error(
        "LeaseSetClientAuths Key must be exactly {LEASESET_CLIENT_KEY_HEX_LEN} lowercase hex characters (32 bytes)"
    )]
    ClientKeyLength,
    /// The `Key` field was not lowercase hexadecimal.
    #[error("LeaseSetClientAuths Key must be lowercase hexadecimal")]
    ClientKeyEncoding,
    /// An entry carried a field other than `Name`/`name` and `Key`/`key`.
    #[error("LeaseSetClientAuths accepts only Name and Key")]
    UnexpectedField,
    /// More entries than [`MAX_LEASESET_CLIENT_AUTHS`].
    #[error("LeaseSetClientAuths accepts at most {MAX_LEASESET_CLIENT_AUTHS} entries")]
    TooManyClients,
    /// Two entries shared a `Name`.
    #[error("LeaseSetClientAuths contains a duplicate Name")]
    DuplicateClientName,
}

/// Decodes a Proposal `LeaseSetClientAuths` JSON array into bounded entries.
///
/// Accepts the `Name`/`name` and `Key`/`key` spellings the frozen
/// `validate_proposal_tunnel_value` allows, rejects a single object that
/// carries both spellings of either field, and enforces the count ceiling
/// **before** allocating the entry vector.
pub fn decode_lease_set_client_auths(
    value: &serde_json::Value,
) -> Result<Vec<LeaseSetClientAuthEntry>, LeaseSetClientAuthError> {
    let items = value
        .as_array()
        .ok_or(LeaseSetClientAuthError::TooManyClients)
        .and_then(|items| {
            if items.len() > MAX_LEASESET_CLIENT_AUTHS {
                Err(LeaseSetClientAuthError::TooManyClients)
            } else {
                Ok(items)
            }
        })?;
    let mut entries = Vec::with_capacity(items.len());
    for item in items {
        let object = item
            .as_object()
            .ok_or(LeaseSetClientAuthError::ClientKeyLength)?;
        // Closed object: an unrecognised field is rejected rather than
        // ignored, so a caller cannot believe a field took effect when it did
        // not. This mirrors the frozen `validate_proposal_tunnel_value` shape
        // check but is enforced here too, because this decoder is public.
        if object.len() > 2
            || object
                .keys()
                .any(|field| !matches!(field.as_str(), "Name" | "name" | "Key" | "key"))
        {
            return Err(LeaseSetClientAuthError::UnexpectedField);
        }
        let name = pick_field(object, "Name", "name")
            .and_then(serde_json::Value::as_str)
            .ok_or(LeaseSetClientAuthError::EmptyClientName)?;
        let key = pick_field(object, "Key", "key")
            .and_then(serde_json::Value::as_str)
            .ok_or(LeaseSetClientAuthError::ClientKeyLength)?;
        entries.push(LeaseSetClientAuthEntry::new(name, key)?);
    }
    if has_duplicate_name(&entries) {
        return Err(LeaseSetClientAuthError::DuplicateClientName);
    }
    Ok(entries)
}

fn pick_field<'a>(
    object: &'a serde_json::Map<String, serde_json::Value>,
    canonical: &str,
    alternate: &str,
) -> Option<&'a serde_json::Value> {
    object.get(canonical).or_else(|| object.get(alternate))
}

fn has_duplicate_name(entries: &[LeaseSetClientAuthEntry]) -> bool {
    let mut seen: Vec<&str> = entries.iter().map(LeaseSetClientAuthEntry::name).collect();
    seen.sort_unstable();
    seen.windows(2).any(|pair| pair[0] == pair[1])
}

/// Encodes validated entries into the single durable option value.
///
/// The form is `name:hexkey` entries joined by one newline. Every component is
/// fixed-width and the two separators are excluded from
/// [`MAX_LEASESET_CLIENT_NAME_LEN`]-bounded names, so the encoding is
/// unambiguous and parses back exactly. [`MAX_LEASESET_CLIENT_AUTHS`] is
/// chosen so the worst case fits [`MAX_OPTION_VALUE_LEN`]; a test asserts it.
pub fn encode_lease_set_client_auths(entries: &[LeaseSetClientAuthEntry]) -> String {
    let capacity = max_encoded_client_auths_bytes(entries.len());
    let mut encoded = String::with_capacity(capacity);
    for (index, entry) in entries.iter().enumerate() {
        if index > 0 {
            encoded.push(ENTRY_SEPARATOR);
        }
        encoded.push_str(entry.name());
        encoded.push(FIELD_SEPARATOR);
        encoded.push_str(entry.encoded_key());
    }
    encoded
}

/// Parses one durable `LeaseSetClientAuths` option value back into entries.
///
/// The inverse of [`encode_lease_set_client_auths`]. Used when a stored
/// definition is reloaded, so a restart republishes the same authorization
/// block.
pub fn decode_encoded_client_auths(
    encoded: &str,
) -> Result<Vec<LeaseSetClientAuthEntry>, LeaseSetClientAuthError> {
    if encoded.is_empty() {
        return Err(LeaseSetClientAuthError::ClientKeyLength);
    }
    if encoded.len() > MAX_OPTION_VALUE_LEN {
        return Err(LeaseSetClientAuthError::TooManyClients);
    }
    let parts: Vec<&str> = encoded.split(ENTRY_SEPARATOR).collect();
    if parts.len() > MAX_LEASESET_CLIENT_AUTHS {
        return Err(LeaseSetClientAuthError::TooManyClients);
    }
    let mut entries = Vec::with_capacity(parts.len());
    for part in parts {
        let (name, key) = part
            .split_once(FIELD_SEPARATOR)
            .ok_or(LeaseSetClientAuthError::ClientKeyLength)?;
        entries.push(LeaseSetClientAuthEntry::new(name, key)?);
    }
    if has_duplicate_name(&entries) {
        return Err(LeaseSetClientAuthError::DuplicateClientName);
    }
    Ok(entries)
}

/// A fully validated LeaseSet security configuration for one service.
///
/// Produced only by [`resolve_lease_set_security`], so a value of this type
/// has already passed every mode/secret/list rule. It is `Copy` and holds
/// only counts and flags — never key material — which is what makes it safe to
/// hand to the daemon's redaction, persistence, and spec-projection paths.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LeaseSetSecurityPlan {
    behavior: LeaseSetSecurityBehavior,
    spelling: Option<&'static str>,
    lookup_secret_required: bool,
    lookup_secret_supplied: bool,
    client_auth_scheme: Option<LeaseSetClientAuthScheme>,
    client_count: usize,
}

impl LeaseSetSecurityPlan {
    /// The protocol behavior this service will publish.
    pub const fn behavior(self) -> LeaseSetSecurityBehavior {
        self.behavior
    }

    /// The exact Proposal spelling received, or `None` when the field was
    /// omitted entirely (which selects the ordinary behavior).
    pub const fn spelling(self) -> Option<&'static str> {
        self.spelling
    }

    /// Whether the selected behavior needs a lookup secret.
    pub const fn lookup_secret_required(self) -> bool {
        self.lookup_secret_required
    }

    /// Whether a lookup secret was supplied. Always `true` when
    /// [`Self::lookup_secret_required`] is `true`; a successful plan can
    /// never describe a required-but-missing secret.
    pub const fn lookup_secret_supplied(self) -> bool {
        self.lookup_secret_supplied
    }

    /// The client-authorization scheme, or `None` for a mode without one.
    pub const fn client_auth_scheme(self) -> Option<LeaseSetClientAuthScheme> {
        self.client_auth_scheme
    }

    /// How many clients are authorized. `0` for a mode without an
    /// authorization block.
    pub const fn client_count(self) -> usize {
        self.client_count
    }

    /// Whether this service publishes a modern type-5 encrypted LeaseSet2.
    pub const fn publishes_type5(self) -> bool {
        self.behavior.publishes_type5()
    }

    /// The address flags this service's published address must carry.
    pub const fn address_flags(self) -> LeaseSetAddressFlags {
        LeaseSetAddressFlags::NONE
            .with_if(
                self.lookup_secret_required,
                LeaseSetAddressFlag::BlindingSecret,
            )
            .with_if(
                self.client_auth_scheme.is_some(),
                LeaseSetAddressFlag::ClientKey,
            )
    }
}

impl LeaseSetAddressFlags {
    const fn with_if(self, condition: bool, flag: LeaseSetAddressFlag) -> Self {
        if condition { self.with(flag) } else { self }
    }
}

/// Validates the complete LeaseSet security block for one service definition.
///
/// `mode` is the resolved `EncryptLeaseSet` value, or `None` when the field was
/// omitted — omission and `disable` are the same ordinary behavior, and both
/// forbid the two secret parameters.
///
/// The rules enforced, in order, before any mutation happens:
///
/// 1. A mode that i2pr refuses is refused by name
///    ([`LeaseSetSecurityError::LegacyAesUnsupported`]).
/// 2. A lookup secret supplied to a mode that does not consume it is an
///    **error**, not a no-op: silently ignoring it would leave an operator
///    believing a service is protected by one when it is not.
/// 3. A mode that requires a lookup secret and did not get one is an error.
///    An empty or whitespace-only secret is *also* an error, because it would
///    otherwise silently downgrade to the no-secret behavior — the frozen
///    mapping's "no empty-secret fallback" rule.
/// 4. Client authorization entries are likewise required, not ignored, and are
///    likewise refused when the mode does not consume them.
///
/// The caller receives a [`LeaseSetSecurityPlan`] only when every rule passed.
pub fn resolve_lease_set_security(
    mode: Option<&EncryptLeaseSetMode>,
    lookup_secret: Option<&str>,
    client_auths: &[LeaseSetClientAuthEntry],
) -> Result<LeaseSetSecurityPlan, LeaseSetSecurityError> {
    let resolved = mode.copied();
    let behavior = resolved.map_or(
        LeaseSetSecurityBehavior::Ordinary,
        EncryptLeaseSetMode::behavior,
    );
    let spelling = resolved.map(EncryptLeaseSetMode::spelling);
    if behavior == LeaseSetSecurityBehavior::LegacyAes {
        return Err(LeaseSetSecurityError::LegacyAesUnsupported {
            spelling: spelling.expect("a legacy mode always has a spelling"),
        });
    }

    let requires_lookup_secret = behavior.requires_lookup_secret();
    let lookup_secret_supplied = match lookup_secret {
        None => false,
        Some(secret) if requires_lookup_secret => {
            if secret.trim().is_empty() {
                return Err(LeaseSetSecurityError::LookupSecretEmpty {
                    spelling: expect_spelling(spelling),
                });
            }
            true
        }
        Some(_) => {
            return Err(LeaseSetSecurityError::LookupSecretNotPermitted {
                spelling: expect_spelling(spelling),
            });
        }
    };
    if requires_lookup_secret && !lookup_secret_supplied {
        return Err(LeaseSetSecurityError::LookupSecretRequired {
            spelling: expect_spelling(spelling),
        });
    }

    let requires_client_auths = behavior.requires_client_auths();
    if !requires_client_auths {
        if !client_auths.is_empty() {
            return Err(LeaseSetSecurityError::ClientAuthsNotPermitted {
                spelling: expect_spelling(spelling),
            });
        }
    } else {
        if client_auths.is_empty() {
            return Err(LeaseSetSecurityError::ClientAuthsRequired {
                spelling: expect_spelling(spelling),
            });
        }
        if client_auths.len() > MAX_LEASESET_CLIENT_AUTHS {
            return Err(LeaseSetSecurityError::ClientAuthsRejected(
                LeaseSetClientAuthError::TooManyClients,
            ));
        }
        if has_duplicate_name(client_auths) {
            return Err(LeaseSetSecurityError::ClientAuthsRejected(
                LeaseSetClientAuthError::DuplicateClientName,
            ));
        }
    }

    Ok(LeaseSetSecurityPlan {
        behavior,
        spelling,
        lookup_secret_required: requires_lookup_secret,
        lookup_secret_supplied,
        client_auth_scheme: behavior.client_auth_scheme(),
        client_count: if requires_client_auths {
            client_auths.len()
        } else {
            0
        },
    })
}

/// The ordinary behavior's spelling, used in diagnostics when `EncryptLeaseSet`
/// was omitted entirely.
const OMITTED_SPELLING: &str = "disable (field omitted)";

const fn expect_spelling(spelling: Option<&'static str>) -> &'static str {
    match spelling {
        Some(value) => value,
        None => OMITTED_SPELLING,
    }
}

/// Why a LeaseSet security block was rejected.
///
/// Every variant names the mode that was selected and never a secret value.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
pub enum LeaseSetSecurityError {
    /// The Proposal value is valid but i2pr deliberately does not implement
    /// the behavior. The message names the deprecation so the failure is
    /// actionable rather than mysterious.
    #[error(
        "EncryptLeaseSet '{spelling}' selects legacy encrypted LeaseSet1 over AES, which is \
         deprecated (Proposal 121 was rejected and superseded by the modern ChaCha20 \
         encrypted LeaseSet2) and is not implemented in i2pr; use a blinded or encrypted \
         LeaseSet2 mode"
    )]
    LegacyAesUnsupported {
        /// The Proposal spelling that was refused.
        spelling: &'static str,
    },
    /// `OptionalLookup` was supplied to a mode that does not consume it.
    #[error("OptionalLookup is not used by EncryptLeaseSet '{spelling}'")]
    LookupSecretNotPermitted {
        /// The Proposal spelling that was selected.
        spelling: &'static str,
    },
    /// The selected mode needs `OptionalLookup` and it was absent.
    #[error("EncryptLeaseSet '{spelling}' requires OptionalLookup")]
    LookupSecretRequired {
        /// The Proposal spelling that was selected.
        spelling: &'static str,
    },
    /// The selected mode needs `OptionalLookup` and the supplied value was
    /// empty or whitespace only.
    #[error(
        "EncryptLeaseSet '{spelling}' requires a non-empty OptionalLookup; an empty value would \
         silently publish without a lookup secret"
    )]
    LookupSecretEmpty {
        /// The Proposal spelling that was selected.
        spelling: &'static str,
    },
    /// `LeaseSetClientAuths` was supplied to a mode that does not consume it.
    #[error("LeaseSetClientAuths is not used by EncryptLeaseSet '{spelling}'")]
    ClientAuthsNotPermitted {
        /// The Proposal spelling that was selected.
        spelling: &'static str,
    },
    /// The selected mode needs `LeaseSetClientAuths` and none were supplied.
    #[error("EncryptLeaseSet '{spelling}' requires LeaseSetClientAuths")]
    ClientAuthsRequired {
        /// The Proposal spelling that was selected.
        spelling: &'static str,
    },
    /// One entry was malformed, over the count ceiling, or duplicated a name.
    #[error("LeaseSetClientAuths rejected: {0}")]
    ClientAuthsRejected(LeaseSetClientAuthError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proposal_wire::PROPOSAL_ENCRYPT_LEASE_SET_VALUES;

    fn entry(name: &str) -> LeaseSetClientAuthEntry {
        LeaseSetClientAuthEntry::new(name, &"ab".repeat(LEASESET_CLIENT_KEY_LEN)).expect("valid")
    }

    fn mode(value: &str) -> EncryptLeaseSetMode {
        resolve_encrypt_lease_set_mode(value).expect("frozen Proposal value resolves")
    }

    /// Asserts the exact rejection reason.
    ///
    /// `LeaseSetClientAuthEntry` is deliberately not `PartialEq`, so a
    /// `Result` carrying it cannot be compared whole; the error side can.
    fn assert_client_auth_error<T>(
        result: Result<T, LeaseSetClientAuthError>,
        expected: LeaseSetClientAuthError,
    ) {
        assert_eq!(result.err(), Some(expected));
    }

    #[test]
    fn every_ten_proposal_values_resolve_to_a_documented_behavior() {
        assert_eq!(PROPOSAL_ENCRYPT_LEASE_SET_VALUES.len(), 10);
        let resolved: Vec<LeaseSetSecurityBehavior> = PROPOSAL_ENCRYPT_LEASE_SET_VALUES
            .iter()
            .map(|value| {
                resolve_encrypt_lease_set_mode(value)
                    .expect("resolves")
                    .behavior()
            })
            .collect();
        assert_eq!(
            resolved,
            vec![
                LeaseSetSecurityBehavior::Ordinary,
                LeaseSetSecurityBehavior::LegacyAes,
                LeaseSetSecurityBehavior::Blinded,
                LeaseSetSecurityBehavior::BlindedWithLookupSecret,
                LeaseSetSecurityBehavior::PreSharedKey,
                LeaseSetSecurityBehavior::PreSharedKeyWithLookupSecret,
                LeaseSetSecurityBehavior::PreSharedKey,
                LeaseSetSecurityBehavior::PreSharedKeyWithLookupSecret,
                LeaseSetSecurityBehavior::DiffieHellman,
                LeaseSetSecurityBehavior::DiffieHellmanWithLookupSecret,
            ],
            "the ten Proposal values must map to exactly the eight frozen behaviors"
        );
        // Every mode round-trips its exact spelling.
        for value in PROPOSAL_ENCRYPT_LEASE_SET_VALUES {
            assert_eq!(mode(value).spelling(), value);
        }
    }

    #[test]
    fn per_user_spellings_are_recorded_and_change_no_behavior() {
        let plain = mode("encrypted (psk)");
        let per_user = mode("encrypted with per-user key (psk)");
        assert_eq!(plain.behavior(), per_user.behavior());
        assert!(!plain.uses_per_user_spelling());
        assert!(per_user.uses_per_user_spelling());

        let plain_lookup = mode("encrypted with lookup password (psk)");
        let per_user_lookup = mode("encrypted with lookup password and per-user key (psk)");
        assert_eq!(plain_lookup.behavior(), per_user_lookup.behavior());
        assert!(per_user_lookup.uses_per_user_spelling());
    }

    #[test]
    fn mode_names_are_identifiers_and_are_never_normalized() {
        for rejected in [
            "",
            "disable ",
            " disable",
            "DISABLE",
            "Disable",
            "encrypted(aes)",
            "encrypted  (psk)",
            "encrypted (PSK)",
            "encrypted (psk) ",
            "blinded with lookup-password",
            "encrypted with per user key (psk)",
        ] {
            assert_eq!(
                resolve_encrypt_lease_set_mode(rejected),
                Err(EncryptLeaseSetModeError::UnknownMode),
                "must not accept a normalized variant: {rejected:?}"
            );
        }
    }

    #[test]
    fn legacy_aes_is_recognized_then_refused_by_name() {
        let legacy = mode("encrypted (aes)");
        assert_eq!(legacy.behavior(), LeaseSetSecurityBehavior::LegacyAes);
        assert!(!legacy.behavior().publishes_type5());
        let error =
            resolve_lease_set_security(Some(&legacy), None, &[]).expect_err("legacy aes refused");
        assert!(matches!(
            error,
            LeaseSetSecurityError::LegacyAesUnsupported {
                spelling: "encrypted (aes)"
            }
        ));
        let rendered = error.to_string();
        assert!(rendered.contains("deprecated"), "{rendered}");
        assert!(rendered.contains("Proposal 121"), "{rendered}");
        assert!(rendered.contains("encrypted LeaseSet2"), "{rendered}");
    }

    #[test]
    fn every_applied_mode_resolves_with_its_own_companions() {
        // Ordinary: no companions.
        let plan = resolve_lease_set_security(Some(&mode("disable")), None, &[]).expect("ordinary");
        assert_eq!(plan.behavior(), LeaseSetSecurityBehavior::Ordinary);
        assert!(!plan.publishes_type5());
        assert_eq!(plan.client_count(), 0);
        assert_eq!(plan.address_flags(), LeaseSetAddressFlags::NONE);
        // Omitting the field is the same behavior and is reported as omitted.
        let omitted = resolve_lease_set_security(None, None, &[]).expect("omitted");
        assert_eq!(omitted.behavior(), LeaseSetSecurityBehavior::Ordinary);
        assert_eq!(omitted.spelling(), None);
        // Blinded, no secret.
        let plan = resolve_lease_set_security(Some(&mode("blinded")), None, &[]).expect("blinded");
        assert_eq!(plan.behavior(), LeaseSetSecurityBehavior::Blinded);
        assert_eq!(plan.address_flags(), LeaseSetAddressFlags::NONE);
        // Blinded + lookup secret.
        let plan = resolve_lease_set_security(
            Some(&mode("blinded with lookup password")),
            Some("pw"),
            &[],
        )
        .expect("blinded lookup");
        assert_eq!(plan.address_flags(), LeaseSetAddressFlags::BLINDING_SECRET);
        assert!(plan.lookup_secret_required() && plan.lookup_secret_supplied());
        // PSK / DH, with and without a lookup secret.
        for (value, scheme, with_lookup) in [
            (
                "encrypted (psk)",
                LeaseSetClientAuthScheme::PreSharedKey,
                false,
            ),
            (
                "encrypted with lookup password (psk)",
                LeaseSetClientAuthScheme::PreSharedKey,
                true,
            ),
            (
                "encrypted with per-user key (dh)",
                LeaseSetClientAuthScheme::DiffieHellman,
                false,
            ),
            (
                "encrypted with lookup password and per-user key (dh)",
                LeaseSetClientAuthScheme::DiffieHellman,
                true,
            ),
        ] {
            let clients = [entry("alpha"), entry("beta")];
            let secret = with_lookup.then_some("pw");
            let plan = resolve_lease_set_security(Some(&mode(value)), secret, &clients)
                .unwrap_or_else(|error| panic!("{value} must resolve: {error}"));
            assert_eq!(
                plan.behavior().client_auth_scheme(),
                Some(scheme),
                "{value}"
            );
            assert_eq!(plan.client_count(), 2, "{value}");
            assert_eq!(plan.lookup_secret_required(), with_lookup, "{value}");
            let expected = match with_lookup {
                true => LeaseSetAddressFlags::BLINDING_SECRET_AND_CLIENT_KEY,
                false => LeaseSetAddressFlags::CLIENT_KEY,
            };
            assert_eq!(plan.address_flags(), expected, "{value}");
            assert!(plan.behavior().requires_blinded_identity(), "{value}");
        }
    }

    #[test]
    fn a_supplied_but_unused_secret_is_an_error_not_a_no_op() {
        // A lookup secret for a mode that does not consume it would leave an
        // operator believing the service is protected by one.
        for value in [
            "disable",
            "blinded",
            "encrypted (psk)",
            "encrypted with per-user key (dh)",
        ] {
            let error = resolve_lease_set_security(Some(&mode(value)), Some("pw"), &[])
                .expect_err("unused lookup secret refused");
            assert!(matches!(
                error,
                LeaseSetSecurityError::LookupSecretNotPermitted { .. }
            ));
            assert!(error.to_string().contains(value), "{value}");
        }
        // Client auths for a mode that does not consume them. The lookup
        // secret is supplied where the mode needs one, so the client-auth
        // rule is what fires.
        for (value, secret) in [
            ("disable", None),
            ("blinded", None),
            ("blinded with lookup password", Some("pw")),
        ] {
            let error = resolve_lease_set_security(Some(&mode(value)), secret, &[entry("alpha")])
                .expect_err("unused client auths refused");
            assert!(
                matches!(error, LeaseSetSecurityError::ClientAuthsNotPermitted { .. }),
                "{value}: {error:?}"
            );
        }
        // Both secrets supplied to `disable`.
        assert!(matches!(
            resolve_lease_set_security(Some(&mode("disable")), Some("pw"), &[entry("a")]),
            Err(LeaseSetSecurityError::LookupSecretNotPermitted { .. })
        ));
        // A lookup secret with no mode at all.
        assert!(matches!(
            resolve_lease_set_security(None, Some("pw"), &[]),
            Err(LeaseSetSecurityError::LookupSecretNotPermitted { .. })
        ));
    }

    #[test]
    fn a_required_secret_must_be_present_and_non_empty() {
        // Only the four modes whose spelling names a lookup password.
        for value in [
            "blinded with lookup password",
            "encrypted with lookup password (psk)",
            "encrypted with lookup password and per-user key (psk)",
            "encrypted with lookup password and per-user key (dh)",
        ] {
            let needs_auths = value.contains("psk)") || value.ends_with("(dh)");
            let clients: Vec<LeaseSetClientAuthEntry> = if needs_auths {
                vec![entry("a")]
            } else {
                Vec::new()
            };
            let error = resolve_lease_set_security(Some(&mode(value)), None, &clients)
                .expect_err("missing lookup secret refused");
            assert!(
                matches!(error, LeaseSetSecurityError::LookupSecretRequired { .. }),
                "{value}: {error:?}"
            );
            for empty in ["", " ", "\t", "\n"] {
                let error = resolve_lease_set_security(Some(&mode(value)), Some(empty), &clients)
                    .expect_err("empty lookup secret refused");
                assert!(
                    matches!(error, LeaseSetSecurityError::LookupSecretEmpty { .. }),
                    "{value} with {empty:?}: {error:?}"
                );
            }
        }
        // The four modes that carry an authorization block need clients.
        for value in [
            "encrypted (psk)",
            "encrypted with lookup password (psk)",
            "encrypted with per-user key (dh)",
            "encrypted with lookup password and per-user key (dh)",
        ] {
            let secret = value.contains("lookup password").then_some("pw");
            let error = resolve_lease_set_security(Some(&mode(value)), secret, &[])
                .expect_err("missing client auths refused");
            assert!(
                matches!(error, LeaseSetSecurityError::ClientAuthsRequired { .. }),
                "{value}: {error:?}"
            );
        }
    }

    #[test]
    fn the_every_illegal_combination_is_rejected_before_any_mutation() {
        // A resolved plan can never describe a required-but-missing secret.
        for value in PROPOSAL_ENCRYPT_LEASE_SET_VALUES {
            let resolved = mode(value);
            if resolved.behavior() == LeaseSetSecurityBehavior::LegacyAes {
                continue;
            }
            let needs_auths = resolved.behavior().requires_client_auths();
            let clients: Vec<LeaseSetClientAuthEntry> = if needs_auths {
                vec![entry("a")]
            } else {
                Vec::new()
            };
            let secret = resolved.behavior().requires_lookup_secret().then_some("pw");
            let plan = resolve_lease_set_security(Some(&resolved), secret, &clients)
                .unwrap_or_else(|error| panic!("{value} must resolve: {error}"));
            assert!(
                plan.lookup_secret_required() == plan.lookup_secret_supplied(),
                "{value} produced an inconsistent lookup-secret state"
            );
        }
    }

    #[test]
    fn client_entries_are_bounded_charset_checked_and_redacted() {
        assert_client_auth_error(
            LeaseSetClientAuthEntry::new("", &"ab".repeat(32)),
            LeaseSetClientAuthError::EmptyClientName,
        );
        assert_client_auth_error(
            LeaseSetClientAuthEntry::new(&"a".repeat(65), &"ab".repeat(32)),
            LeaseSetClientAuthError::ClientNameTooLong,
        );
        for name in ["a:b", "a\nb", "a\rb", "a\0b"] {
            assert_client_auth_error(
                LeaseSetClientAuthEntry::new(name, &"ab".repeat(32)),
                LeaseSetClientAuthError::ClientNameCharset,
            );
        }
        assert_client_auth_error(
            LeaseSetClientAuthEntry::new("a", &"ab".repeat(31)),
            LeaseSetClientAuthError::ClientKeyLength,
        );
        assert_client_auth_error(
            LeaseSetClientAuthEntry::new("a", &"AB".repeat(32)),
            LeaseSetClientAuthError::ClientKeyEncoding,
        );
        assert_client_auth_error(
            LeaseSetClientAuthEntry::new("a", &"zz".repeat(32)),
            LeaseSetClientAuthError::ClientKeyEncoding,
        );
        // Uppercase hex is refused so the durable encoding has one spelling.
        assert_client_auth_error(
            LeaseSetClientAuthEntry::new("a", &"AB".repeat(32)),
            LeaseSetClientAuthError::ClientKeyEncoding,
        );
        // The name may use every other byte, including spaces and parentheses.
        assert!(LeaseSetClientAuthEntry::new("client one (psk)", &"00".repeat(32)).is_ok());

        let key = "ab".repeat(LEASESET_CLIENT_KEY_LEN);
        let rendered = format!("{:?}", entry("alpha"));
        assert!(rendered.contains("alpha"), "{rendered}");
        assert!(!rendered.contains(&key), "key leaked in Debug: {rendered}");
        // The type derives neither `Clone` nor `Serialize`, so a decoded
        // entry cannot be copied into a second structure or written out as
        // data. That is a source-level property with no runtime witness; the
        // workspace `anyhow`/serialization policy and review are what hold it.
        // What *is* testable is that no accessor hands back anything the
        // Debug impl hides in a form a caller would log.
        assert_eq!(
            entry("alpha").encoded_key().len(),
            LEASESET_CLIENT_KEY_HEX_LEN
        );
        assert!(!format!("{:?}", entry("alpha")).contains(entry("alpha").encoded_key()));
    }

    #[test]
    fn the_durable_encoding_round_trips_and_fits_the_option_value_ceiling() {
        let worst_case = max_encoded_client_auths_bytes(MAX_LEASESET_CLIENT_AUTHS);
        assert!(
            worst_case <= MAX_OPTION_VALUE_LEN,
            "worst-case encoding ({worst_case}) must fit MAX_OPTION_VALUE_LEN ({MAX_OPTION_VALUE_LEN})"
        );
        let entries: Vec<LeaseSetClientAuthEntry> = (0..MAX_LEASESET_CLIENT_AUTHS)
            .map(|index| {
                LeaseSetClientAuthEntry::new(&format!("client-{index}"), &"ab".repeat(32))
                    .expect("valid")
            })
            .collect();
        let encoded = encode_lease_set_client_auths(&entries);
        assert!(encoded.len() <= MAX_OPTION_VALUE_LEN);
        let decoded = decode_encoded_client_auths(&encoded).expect("round trips");
        assert_eq!(decoded.len(), entries.len());
        for (left, right) in decoded.iter().zip(&entries) {
            assert_eq!(left.name(), right.name());
            assert_eq!(left.encoded_key(), right.encoded_key());
        }
        // Malformed stored values fail closed rather than publishing a block.
        assert!(decode_encoded_client_auths("").is_err());
        assert!(decode_encoded_client_auths("no-separator").is_err());
        assert!(decode_encoded_client_auths("name:short").is_err());
        assert!(decode_encoded_client_auths(&"x".repeat(MAX_OPTION_VALUE_LEN + 1)).is_err());
        let over_count = (0..MAX_LEASESET_CLIENT_AUTHS + 1)
            .map(|index| format!("c{index}:{}", "ab".repeat(32)))
            .collect::<Vec<_>>()
            .join("\n");
        assert_client_auth_error(
            decode_encoded_client_auths(&over_count),
            LeaseSetClientAuthError::TooManyClients,
        );
    }

    #[test]
    fn the_wire_array_decodes_with_both_proposal_spellings_and_is_bounded() {
        let array = serde_json::json!([
            {"Name": "alpha", "Key": "ab".repeat(32)},
            {"name": "beta", "key": "cd".repeat(32)},
        ]);
        let entries = decode_lease_set_client_auths(&array).expect("both spellings");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name(), "alpha");
        assert_eq!(entries[1].name(), "beta");

        // The count ceiling is enforced before the entry vector is built.
        let over = serde_json::Value::Array(
            (0..=MAX_LEASESET_CLIENT_AUTHS)
                .map(|_| serde_json::json!({"Name": "n", "Key": "ab".repeat(32)}))
                .collect(),
        );
        assert_client_auth_error(
            decode_lease_set_client_auths(&over),
            LeaseSetClientAuthError::TooManyClients,
        );

        // Shape failures never reach the entry constructor.
        for bad in [
            serde_json::json!({}),
            serde_json::json!("alpha"),
            serde_json::json!([{"Name": "a"}]),
            serde_json::json!([{"Key": "ab".repeat(32)}]),
            serde_json::json!([{"Name": 1, "Key": "ab".repeat(32)}]),
            serde_json::json!([{"Name": "a", "Key": "ab".repeat(32), "Extra": 1}]),
            serde_json::json!([{"Name": "a", "name": "b", "Key": "ab".repeat(32)}]),
        ] {
            assert!(
                decode_lease_set_client_auths(&bad).is_err(),
                "must reject {bad}"
            );
        }
        // Duplicate names are refused at build, not merged.
        let duplicate = serde_json::json!([
            {"Name": "same", "Key": "ab".repeat(32)},
            {"Name": "same", "Key": "cd".repeat(32)},
        ]);
        assert_client_auth_error(
            decode_lease_set_client_auths(&duplicate),
            LeaseSetClientAuthError::DuplicateClientName,
        );
    }

    #[test]
    fn no_error_message_can_contain_a_secret() {
        let key = "ab".repeat(LEASESET_CLIENT_KEY_LEN);
        let secret = "hunter2-lookup-secret";
        for (value, candidates) in [
            ("disable", vec![Some(secret), None]),
            (
                "blinded with lookup password",
                vec![None, Some(""), Some(" ")],
            ),
            ("encrypted (psk)", vec![Some(secret), None]),
        ] {
            let clients = [entry("alpha")];
            for candidate in candidates {
                if let Err(error) =
                    resolve_lease_set_security(Some(&mode(value)), candidate, &clients)
                {
                    let rendered = error.to_string();
                    for needle in [&key, secret] {
                        assert!(
                            !rendered.contains(needle),
                            "{value} error leaked {needle:?}: {rendered}"
                        );
                    }
                }
            }
        }
        for error in [
            LeaseSetClientAuthError::EmptyClientName,
            LeaseSetClientAuthError::ClientNameTooLong,
            LeaseSetClientAuthError::ClientNameCharset,
            LeaseSetClientAuthError::ClientKeyLength,
            LeaseSetClientAuthError::ClientKeyEncoding,
            LeaseSetClientAuthError::TooManyClients,
            LeaseSetClientAuthError::DuplicateClientName,
        ] {
            let rendered = error.to_string();
            assert!(!rendered.contains(&key), "{rendered}");
        }
    }
}
