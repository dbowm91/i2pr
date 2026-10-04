//! Plan 333: per-client authorization for encrypted LeaseSet2.
//!
//! # The two mechanisms
//!
//! The encrypted LeaseSet2 specification defines two ways for a server to limit
//! who can decrypt a record, and both work the same way: the server generates
//! a fresh 32-byte `authCookie`, and stores it in the record once per authorized
//! client, encrypted under a key only that client can derive.
//!
//! - **Pre-shared key.** Each client holds a 32-byte `psk_i`. The server draws a
//!   fresh 32-byte `authSalt` per publication.
//! - **Diffie-Hellman.** Each client holds an X25519 keypair and the server
//!   publishes the public half. The server draws a *generation-local* ephemeral
//!   X25519 keypair per publication and publishes `epk`.
//!
//! In both cases the derivation is `HKDF(salt, authInput, label, 52)` and the
//! per-client key, IV, and identifier are sliced out of it. Only the salt and the
//! `authInput` differ, which is why the two paths share one code path.
//!
//! # What the cookie actually buys
//!
//! The `authCookie` is not the record's content key. It is prepended to the
//! **layer-2** key input, so a client that cannot recover it derives a wrong
//! layer-2 key and the inner LeaseSet2 never decrypts. The layer-1 key is
//! unchanged, so a passive observer who can fetch the record still sees layer-1
//! plaintext — which is exactly the privacy property the specification asks for:
//! *an entity that only knows the Destination can see how many clients are
//! subscribed, but cannot track which clients are being added or revoked*.
//!
//! # What is deliberately absent
//!
//! No persistence, no Proposal 170 parsing, and no I2CP field. This module owns
//! the protocol material; [`Els2ClientAuthSecret`] owns the secret lifecycle
//! interface that a persistence policy and a control surface will consume.

use i2pr_crypto::red25519::MAX_LOOKUP_SECRET_LENGTH;
use i2pr_crypto::{CryptoError, X25519PrivateKey, constant_time_eq};
use i2pr_crypto::{TryCryptoRng, Zeroizing};
use thiserror::Error;

use crate::els2::{
    ELS2_AUTH_CLIENT_LENGTH, ELS2_LAYER1_FLAG_PER_CLIENT, ELS2_LAYER1_SCHEME_DH,
    ELS2_LAYER1_SCHEME_MASK, ELS2_LAYER1_SCHEME_PSK, ELS2_LAYER1_SCHEME_SHIFT,
    MAX_ELS2_AUTH_CLIENTS,
};

/// Length of the per-client authorization key derivation output.
///
/// The specification writes the slices as `keys[0:31]`, `keys[32:43]`, and
/// `keys[44:51]`, which cannot describe a 32-byte ChaCha20 key, a 12-byte IV, and
/// an 8-byte identifier. Both pinned references use a 52-byte output sliced
/// `0..32` / `32..44` / `44..52`, and this module follows them for the same reason
/// the layer keys do.
pub const ELS2_AUTH_OKM_LENGTH: usize = 52;
/// Length of the derived per-client key.
pub const ELS2_AUTH_KEY_LENGTH: usize = 32;
/// Length of the derived per-client IV.
pub const ELS2_AUTH_IV_LENGTH: usize = 12;
/// Length of the derived per-client identifier.
pub const ELS2_AUTH_CLIENT_ID_LENGTH: usize = 8;
/// Length of the encrypted `authCookie` stored per client.
pub const ELS2_AUTH_COOKIE_LENGTH: usize = 32;
/// HKDF `info` label for the Diffie-Hellman authorization derivation.
pub const ELS2_DH_AUTH_HKDF_INFO: &[u8] = b"ELS2_XCA";
/// HKDF `info` label for the pre-shared-key authorization derivation.
pub const ELS2_PSK_AUTH_HKDF_INFO: &[u8] = b"ELS2PSKA";
/// The crypto key type a client's Diffie-Hellman public key uses.
///
/// X25519 in I2P is an encryption key type (4), not a signing type, and the
/// authorization derivation never places a key type into its input: the
/// `authInput` is `sharedSecret || cpk_i || subcredential || publishedTimestamp`
/// with no type bytes at all. The constant therefore exists for a control
/// surface that has to name the type, and nothing in the derivation reads it.
pub const ELS2_DH_AUTH_KEY_TYPE_CODE: u16 = 4;

/// Errors produced by the per-client authorization surface.
#[derive(Debug, Error)]
pub enum Els2AuthError {
    /// The client set was empty; an authorization block with no clients would
    /// publish a record nobody can read while advertising that authorization is on.
    #[error("encrypted LeaseSet2 authorization requires at least one client")]
    NoClients,
    /// The client set exceeded the bound.
    #[error("encrypted LeaseSet2 authorization client count {actual} exceeds {maximum}")]
    TooManyClients {
        /// Requested client count.
        actual: usize,
        /// Maximum accepted client count.
        maximum: usize,
    },
    /// Two clients derived the same identifier, so an entry lookup would be
    /// ambiguous and a client could be authorized by the wrong cookie.
    #[error("encrypted LeaseSet2 authorization client identifiers collide")]
    DuplicateClientId,
    /// A reserved layer-1 flag bit was set.
    #[error("encrypted LeaseSet2 layer-1 flags set reserved bits {mask:#04x}")]
    ReservedFlags {
        /// The offending reserved-bit mask.
        mask: u8,
    },
    /// The layer-1 per-client bit was clear while authorization was requested.
    #[error(
        "encrypted LeaseSet2 layer-1 flags {flags:#04x} do not request per-client authorization"
    )]
    AuthorizationNotRequested {
        /// The layer-1 flags byte.
        flags: u8,
    },
    /// The layer-1 reserved bits were set, or the flag byte was not the
    /// no-authorization value, so the block cannot be parsed.
    #[error(
        "encrypted LeaseSet2 layer-1 flags {flags:#04x} are not a recognized authorization block"
    )]
    UnrecognizedFlags {
        /// The layer-1 flags byte.
        flags: u8,
    },
    /// The client count field did not match the bytes that remained.
    #[error(
        "encrypted LeaseSet2 authorization block declares {declared} clients but carries {actual} bytes of entry data"
    )]
    ClientCountMismatch {
        /// Client count from the record.
        declared: usize,
        /// Bytes of entry data actually present.
        actual: usize,
    },
    /// The supplied credential could not authorize the record.
    ///
    /// One typed error for both "no key supplied" and "wrong key supplied": a
    /// caller that could tell them apart would learn whether a guessed key was
    /// close, and there is nothing actionable in the difference.
    #[error("encrypted LeaseSet2 no supplied client credential authorizes this record")]
    NotAuthorized,
    /// The layer-1 authorization block was truncated or over-long.
    #[error("encrypted LeaseSet2 authorization block is malformed: {0}")]
    Malformed(&'static str),
    /// A key or derivation operation failed.
    #[error(transparent)]
    Crypto(#[from] CryptoError),
    /// The random source failed.
    #[error("encrypted LeaseSet2 randomness unavailable")]
    RandomnessUnavailable,
    /// A secret was used in a role its owner does not permit.
    #[error("encrypted LeaseSet2 secret with role {role:?} cannot be used as a {expected}")]
    WrongSecretRole {
        /// The role the secret carries.
        role: Els2AuthSecretRole,
        /// What the caller asked for.
        expected: &'static str,
    },
}

/// A per-client pre-shared key.
///
/// The specification calls for a 32-byte key, so the type takes exactly that and
/// nothing else: a 31-byte or 33-byte "PSK" is a configuration error, not a key
/// to be padded or hashed into shape.
pub struct PskClientKey(Zeroizing<[u8; ELS2_AUTH_COOKIE_LENGTH]>);

impl PskClientKey {
    /// Wraps 32 pre-shared-key bytes.
    pub fn from_bytes(bytes: [u8; ELS2_AUTH_COOKIE_LENGTH]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    /// Generates a pre-shared key from an injected random source.
    pub fn generate<R: TryCryptoRng + ?Sized>(rng: &mut R) -> Result<Self, Els2AuthError> {
        let mut bytes = Zeroizing::new([0_u8; ELS2_AUTH_COOKIE_LENGTH]);
        rng.try_fill_bytes(&mut *bytes)
            .map_err(|_| Els2AuthError::RandomnessUnavailable)?;
        Ok(Self(bytes))
    }

    /// Borrows the key for one derivation.
    pub fn as_bytes(&self) -> &[u8; ELS2_AUTH_COOKIE_LENGTH] {
        &self.0
    }
}

impl core::fmt::Debug for PskClientKey {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("PskClientKey")
            .field("configured", &true)
            .finish()
    }
}

/// A client's Diffie-Hellman public key, as published by the server.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct AuthClientPublicKey([u8; 32]);

impl AuthClientPublicKey {
    /// Wraps 32 public-key bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Borrows the public key.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl core::fmt::Debug for AuthClientPublicKey {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("AuthClientPublicKey")
            .field("key_type_code", &ELS2_DH_AUTH_KEY_TYPE_CODE)
            .field("public_key", &format_args!("<{} bytes>", self.0.len()))
            .finish()
    }
}

/// The per-generation `authCookie`.
///
/// It is drawn fresh for every publication and is the only value that bridges
/// the layer-1 authorization block to the layer-2 key. It is zeroizing, is not
/// `Clone`, and has no `Display` or serde implementation.
pub struct AuthCookie(Zeroizing<[u8; ELS2_AUTH_COOKIE_LENGTH]>);

impl AuthCookie {
    /// Wraps 32 cookie bytes.
    pub fn from_bytes(bytes: [u8; ELS2_AUTH_COOKIE_LENGTH]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    /// Draws a fresh cookie from an injected random source.
    pub fn generate<R: TryCryptoRng + ?Sized>(rng: &mut R) -> Result<Self, Els2AuthError> {
        let mut bytes = Zeroizing::new([0_u8; ELS2_AUTH_COOKIE_LENGTH]);
        rng.try_fill_bytes(&mut *bytes)
            .map_err(|_| Els2AuthError::RandomnessUnavailable)?;
        Ok(Self(bytes))
    }

    /// Borrows the cookie for one key derivation.
    pub fn as_bytes(&self) -> &[u8; ELS2_AUTH_COOKIE_LENGTH] {
        &self.0
    }
}

impl core::fmt::Debug for AuthCookie {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("AuthCookie")
            .field("redacted", &true)
            .finish()
    }
}

/// The authentication scheme named by the layer-1 flag bits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Els2AuthScheme {
    /// No per-client authorization; the flag bits are `000`.
    None,
    /// Diffie-Hellman client authorization; the flag bits are `000`.
    Dh,
    /// Pre-shared-key client authorization; the flag bits are `001`.
    Psk,
}

impl Els2AuthScheme {
    /// Returns the three scheme bits.
    pub const fn scheme_bits(self) -> u8 {
        match self {
            Self::Dh | Self::None => ELS2_LAYER1_SCHEME_DH,
            Self::Psk => ELS2_LAYER1_SCHEME_PSK,
        }
    }

    /// Decodes a flags byte into a scheme, or `None` when the per-client bit is clear.
    pub const fn from_flags(flags: u8) -> Option<Self> {
        if flags & ELS2_LAYER1_FLAG_PER_CLIENT == 0 {
            return Some(Self::None);
        }
        match (flags & ELS2_LAYER1_SCHEME_MASK) >> ELS2_LAYER1_SCHEME_SHIFT {
            ELS2_LAYER1_SCHEME_DH => Some(Self::Dh),
            ELS2_LAYER1_SCHEME_PSK => Some(Self::Psk),
            _ => None,
        }
    }

    /// Encodes the scheme into a layer-1 flags byte.
    pub const fn to_flags(self) -> u8 {
        match self {
            Self::None => 0,
            other => {
                ELS2_LAYER1_FLAG_PER_CLIENT | (other.scheme_bits() << ELS2_LAYER1_SCHEME_SHIFT)
            }
        }
    }
}

/// One entry in a layer-1 authorization block.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct AuthClientEntry {
    client_id: [u8; ELS2_AUTH_CLIENT_ID_LENGTH],
    client_cookie: [u8; ELS2_AUTH_COOKIE_LENGTH],
}

impl AuthClientEntry {
    /// Borrows the derived client identifier.
    pub const fn client_id(&self) -> &[u8; ELS2_AUTH_CLIENT_ID_LENGTH] {
        &self.client_id
    }

    /// Borrows the encrypted cookie.
    pub const fn client_cookie(&self) -> &[u8; ELS2_AUTH_COOKIE_LENGTH] {
        &self.client_cookie
    }
}

impl core::fmt::Debug for AuthClientEntry {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("AuthClientEntry")
            .field(
                "client_id",
                &format_args!("<{} bytes>", self.client_id.len()),
            )
            .field("client_cookie", &"<redacted>")
            .finish()
    }
}

/// A parsed layer-1 authorization block.
///
/// `salt` is `authSalt` for the pre-shared-key scheme and the server's ephemeral
/// `epk` for the Diffie-Hellman scheme. Both are public: they are stored in the
/// clear and only need to be fresh per generation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthBlock {
    scheme: Els2AuthScheme,
    salt: [u8; 32],
    entries: Vec<AuthClientEntry>,
}

impl AuthBlock {
    /// Returns the scheme the block was built for.
    pub const fn scheme(&self) -> Els2AuthScheme {
        self.scheme
    }

    /// Borrows the salt or ephemeral public key.
    pub const fn salt(&self) -> &[u8; 32] {
        &self.salt
    }

    /// Borrows the entries in the order they appear in the record.
    pub fn entries(&self) -> &[AuthClientEntry] {
        &self.entries
    }

    /// Returns the number of entries, which is the client count a passive
    /// observer can read from the record.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether the block carries no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Serializes the block into the layer-1 authorization data, which follows the
    /// layer-1 flags byte and precedes the inner ciphertext.
    pub fn encode_to_vec(&self) -> Result<Vec<u8>, Els2AuthError> {
        if self.entries.len() > MAX_ELS2_AUTH_CLIENTS {
            return Err(Els2AuthError::TooManyClients {
                actual: self.entries.len(),
                maximum: MAX_ELS2_AUTH_CLIENTS,
            });
        }
        let count =
            u16::try_from(self.entries.len()).map_err(|_| Els2AuthError::TooManyClients {
                actual: self.entries.len(),
                maximum: MAX_ELS2_AUTH_CLIENTS,
            })?;
        let mut out = Vec::with_capacity(32 + 2 + self.entries.len() * ELS2_AUTH_CLIENT_LENGTH);
        out.extend_from_slice(&self.salt);
        out.extend_from_slice(&count.to_be_bytes());
        for entry in &self.entries {
            out.extend_from_slice(&entry.client_id);
            out.extend_from_slice(&entry.client_cookie);
        }
        Ok(out)
    }

    /// Parses a layer-1 authorization block.
    ///
    /// The declared client count must match the remaining bytes exactly. A block
    /// that is short is rejected; a block with trailing bytes is rejected too,
    /// because those bytes would otherwise be interpreted as the start of the
    /// inner ciphertext and produce a plausible-looking wrong decryption.
    pub fn decode(scheme: Els2AuthScheme, input: &[u8]) -> Result<Self, Els2AuthError> {
        if scheme == Els2AuthScheme::None {
            return Err(Els2AuthError::AuthorizationNotRequested { flags: 0 });
        }
        if input.len() < 34 {
            return Err(Els2AuthError::Malformed("salt or count truncated"));
        }
        let mut salt = [0_u8; 32];
        salt.copy_from_slice(&input[..32]);
        let declared = usize::from(u16::from_be_bytes([input[32], input[33]]));
        if declared > MAX_ELS2_AUTH_CLIENTS {
            return Err(Els2AuthError::TooManyClients {
                actual: declared,
                maximum: MAX_ELS2_AUTH_CLIENTS,
            });
        }
        let body = &input[34..];
        let expected = declared * ELS2_AUTH_CLIENT_LENGTH;
        if body.len() != expected {
            return Err(Els2AuthError::ClientCountMismatch {
                declared,
                actual: body.len(),
            });
        }
        let mut entries = Vec::with_capacity(declared);
        for chunk in body.chunks_exact(ELS2_AUTH_CLIENT_LENGTH) {
            let mut client_id = [0_u8; ELS2_AUTH_CLIENT_ID_LENGTH];
            client_id.copy_from_slice(&chunk[..ELS2_AUTH_CLIENT_ID_LENGTH]);
            let mut client_cookie = [0_u8; ELS2_AUTH_COOKIE_LENGTH];
            client_cookie.copy_from_slice(&chunk[ELS2_AUTH_CLIENT_ID_LENGTH..]);
            entries.push(AuthClientEntry {
                client_id,
                client_cookie,
            });
        }
        Ok(Self {
            scheme,
            salt,
            entries,
        })
    }
}

/// The per-client material derived from one authorization block.
///
/// A client derives this from its own key plus the block's public salt, then uses
/// it to look itself up. It contains the derived key and IV, so it is treated as
/// secret and is zeroizing.
pub struct AuthClientMaterial {
    client_id: [u8; ELS2_AUTH_CLIENT_ID_LENGTH],
    key: i2pr_crypto::LayerCipherKey,
    iv: [u8; ELS2_AUTH_IV_LENGTH],
}

impl core::fmt::Debug for AuthClientMaterial {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("AuthClientMaterial")
            .field(
                "client_id",
                &format_args!("<{} bytes>", self.client_id.len()),
            )
            .field("key", &"<redacted>")
            .field("iv", &format_args!("<{} bytes>", self.iv.len()))
            .finish()
    }
}

impl AuthClientMaterial {
    /// Borrows the derived client identifier.
    pub const fn client_id(&self) -> &[u8; ELS2_AUTH_CLIENT_ID_LENGTH] {
        &self.client_id
    }

    /// Decrypts an entry's cookie and returns the `authCookie` on success.
    pub fn recover_cookie(&self, entry: &AuthClientEntry) -> Result<AuthCookie, Els2AuthError> {
        if !constant_time_eq(self.client_id.as_slice(), entry.client_id.as_slice()) {
            return Err(Els2AuthError::NotAuthorized);
        }
        let mut cookie = entry.client_cookie;
        i2pr_crypto::chacha20_xor_layer(&self.key, &self.iv, &mut cookie)
            .map_err(|_| Els2AuthError::NotAuthorized)?;
        Ok(AuthCookie::from_bytes(cookie))
    }
}

/// The input to one per-client authorization derivation.
fn auth_input(leading: &[u8; 32], subcredential: &[u8; 32], published: u32) -> [u8; 68] {
    let mut input = [0_u8; 68];
    input[..32].copy_from_slice(leading);
    input[32..64].copy_from_slice(subcredential);
    input[64..].copy_from_slice(&published.to_be_bytes());
    input
}

/// Derives a client's key, IV, and identifier from a 52-byte authorization output.
fn material_from_okm(okm: &[u8]) -> Result<AuthClientMaterial, Els2AuthError> {
    if okm.len() != ELS2_AUTH_OKM_LENGTH {
        return Err(Els2AuthError::Malformed(
            "authorization key derivation length",
        ));
    }
    let mut client_id = [0_u8; ELS2_AUTH_CLIENT_ID_LENGTH];
    client_id.copy_from_slice(&okm[44..52]);
    let mut iv = [0_u8; ELS2_AUTH_IV_LENGTH];
    iv.copy_from_slice(&okm[32..44]);
    Ok(AuthClientMaterial {
        client_id,
        key: i2pr_crypto::LayerCipherKey::from_bytes(
            okm[..ELS2_AUTH_KEY_LENGTH].try_into().expect("32-byte key"),
        ),
        iv,
    })
}

/// Derives the authorization material for one pre-shared-key client.
///
/// ```text
/// authInput = psk_i || subcredential || publishedTimestamp
/// okm       = HKDF(authSalt, authInput, "ELS2PSKA", 52)
/// ```
pub fn psk_client_material(
    psk: &PskClientKey,
    subcredential: &[u8; 32],
    published: u32,
    auth_salt: &[u8; 32],
) -> Result<AuthClientMaterial, Els2AuthError> {
    let input = auth_input(psk.as_bytes(), subcredential, published);
    let okm = i2pr_crypto::hkdf_sha256_extract_and_expand(
        auth_salt,
        &input,
        ELS2_PSK_AUTH_HKDF_INFO,
        ELS2_AUTH_OKM_LENGTH,
    )
    .map_err(|_| Els2AuthError::Malformed("authorization key derivation"))?;
    material_from_okm(&okm)
}

/// Derives the authorization material for one Diffie-Hellman client.
///
/// ```text
/// sharedSecret = DH(csk_i, epk)
/// authInput    = sharedSecret || cpk_i || subcredential || publishedTimestamp
/// okm          = HKDF(epk, authInput, "ELS2_XCA", 52)
/// ```
///
/// The all-zero shared secret is rejected by the reviewed `x25519-dalek`
/// wrapper before it can reach the derivation, so no second check is
/// duplicated here.
pub fn dh_client_material(
    csk: &X25519PrivateKey,
    epk: &[u8; 32],
    cpk: &AuthClientPublicKey,
    subcredential: &[u8; 32],
    published: u32,
) -> Result<AuthClientMaterial, Els2AuthError> {
    let shared = csk.diffie_hellman(epk)?;
    let mut input = Vec::with_capacity(68);
    input.extend_from_slice(shared.as_bytes());
    input.extend_from_slice(cpk.as_bytes());
    input.extend_from_slice(subcredential);
    input.extend_from_slice(&published.to_be_bytes());
    let okm = i2pr_crypto::hkdf_sha256_extract_and_expand(
        epk,
        &input,
        ELS2_DH_AUTH_HKDF_INFO,
        ELS2_AUTH_OKM_LENGTH,
    )
    .map_err(|_| Els2AuthError::Malformed("authorization key derivation"))?;
    material_from_okm(&okm)
}

/// Builds a pre-shared-key authorization block.
///
/// The salt is fresh per generation, the cookie is the caller's, and the emitted
/// entry order is shuffled when there is more than one client.
pub fn build_psk_block<R: TryCryptoRng + ?Sized>(
    cookie: &AuthCookie,
    subcredential: &[u8; 32],
    published: u32,
    psks: &[&PskClientKey],
    rng: &mut R,
) -> Result<AuthBlock, Els2AuthError> {
    if psks.is_empty() {
        return Err(Els2AuthError::NoClients);
    }
    if psks.len() > MAX_ELS2_AUTH_CLIENTS {
        return Err(Els2AuthError::TooManyClients {
            actual: psks.len(),
            maximum: MAX_ELS2_AUTH_CLIENTS,
        });
    }
    let mut auth_salt = [0_u8; 32];
    rng.try_fill_bytes(&mut auth_salt)
        .map_err(|_| Els2AuthError::RandomnessUnavailable)?;

    let mut entries = Vec::with_capacity(psks.len());
    for psk in psks {
        let material = psk_client_material(psk, subcredential, published, &auth_salt)?;
        entries.push(entry_for(&material, cookie.as_bytes())?);
    }
    reject_duplicate_ids(&entries)?;
    shuffle(&mut entries, rng);
    Ok(AuthBlock {
        scheme: Els2AuthScheme::Psk,
        salt: auth_salt,
        entries,
    })
}

/// Builds a Diffie-Hellman authorization block.
///
/// `esk` is generation-local: the server generates a fresh ephemeral keypair for
/// every publication, so a client that is revoked stops being able to derive a
/// key for subsequent records.
pub fn build_dh_block<R: TryCryptoRng + ?Sized>(
    cookie: &AuthCookie,
    subcredential: &[u8; 32],
    published: u32,
    client_keys: &[AuthClientPublicKey],
    esk: &X25519PrivateKey,
    rng: &mut R,
) -> Result<AuthBlock, Els2AuthError> {
    if client_keys.is_empty() {
        return Err(Els2AuthError::NoClients);
    }
    if client_keys.len() > MAX_ELS2_AUTH_CLIENTS {
        return Err(Els2AuthError::TooManyClients {
            actual: client_keys.len(),
            maximum: MAX_ELS2_AUTH_CLIENTS,
        });
    }
    let epk = esk.public_bytes();
    let mut entries = Vec::with_capacity(client_keys.len());
    for cpk in client_keys {
        let material = dh_client_material_from_server(esk, &epk, cpk, subcredential, published)?;
        entries.push(entry_for(&material, cookie.as_bytes())?);
    }
    reject_duplicate_ids(&entries)?;
    shuffle(&mut entries, rng);
    Ok(AuthBlock {
        scheme: Els2AuthScheme::Dh,
        salt: epk,
        entries,
    })
}

/// The server's half of the Diffie-Hellman derivation, which is the same
/// computation the client performs with its own private key.
fn dh_client_material_from_server(
    esk: &X25519PrivateKey,
    epk: &[u8; 32],
    cpk: &AuthClientPublicKey,
    subcredential: &[u8; 32],
    published: u32,
) -> Result<AuthClientMaterial, Els2AuthError> {
    let shared = esk.diffie_hellman(cpk.as_bytes())?;
    let mut input = Vec::with_capacity(68);
    input.extend_from_slice(shared.as_bytes());
    input.extend_from_slice(cpk.as_bytes());
    input.extend_from_slice(subcredential);
    input.extend_from_slice(&published.to_be_bytes());
    let okm = i2pr_crypto::hkdf_sha256_extract_and_expand(
        epk,
        &input,
        ELS2_DH_AUTH_HKDF_INFO,
        ELS2_AUTH_OKM_LENGTH,
    )
    .map_err(|_| Els2AuthError::Malformed("authorization key derivation"))?;
    material_from_okm(&okm)
}

fn entry_for(
    material: &AuthClientMaterial,
    cookie: &[u8; 32],
) -> Result<AuthClientEntry, Els2AuthError> {
    let mut encrypted = *cookie;
    i2pr_crypto::chacha20_xor_layer(&material.key, &material.iv, &mut encrypted)
        .map_err(|_| Els2AuthError::Malformed("cookie encryption"))?;
    Ok(AuthClientEntry {
        client_id: material.client_id,
        client_cookie: encrypted,
    })
}

fn reject_duplicate_ids(entries: &[AuthClientEntry]) -> Result<(), Els2AuthError> {
    let mut seen: Vec<[u8; ELS2_AUTH_CLIENT_ID_LENGTH]> = Vec::with_capacity(entries.len());
    for entry in entries {
        if seen.contains(&entry.client_id) {
            return Err(Els2AuthError::DuplicateClientId);
        }
        seen.push(entry.client_id);
    }
    Ok(())
}

/// Shuffles the emitted entry order.
///
/// The specification says a server *should* randomize the order so a client
/// cannot infer its position in the list, and from that when other clients were
/// added or revoked. A single entry is left alone because there is nothing to
/// hide and the shuffle would only consume randomness.
fn shuffle<R: TryCryptoRng + ?Sized>(entries: &mut [AuthClientEntry], rng: &mut R) {
    if entries.len() < 2 {
        return;
    }
    // Fisher-Yates from the end, with bounded rejection sampling so the index
    // distribution is uniform rather than biased toward the low end of the range.
    for index in (1..entries.len()).rev() {
        let bound = (index + 1) as u32;
        let mut chosen = 0_u32;
        while chosen >= bound {
            let mut bytes = [0_u8; 4];
            if rng.try_fill_bytes(&mut bytes).is_err() {
                // A failing random source leaves the order as-is rather than
                // aborting a publication that is otherwise valid; the order is a
                // privacy hint, not a correctness requirement.
                return;
            }
            chosen = u32::from_le_bytes(bytes) % bound;
        }
        entries.swap(index, chosen as usize);
    }
}

/// The client half of a record's authorization: exactly one credential.
pub enum Els2ClientAuth<'a> {
    /// The client holds this pre-shared key.
    Psk(&'a PskClientKey),
    /// The client holds this Diffie-Hellman private key and is the holder of this
    /// public key.
    Dh {
        /// The client's private key.
        private: &'a X25519PrivateKey,
        /// The client's own public key, which the record's derivation names.
        public: AuthClientPublicKey,
    },
}

impl core::fmt::Debug for Els2ClientAuth<'_> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Psk(_) => formatter
                .debug_struct("Els2ClientAuth::Psk")
                .field("psk", &"<redacted>")
                .finish(),
            Self::Dh { public, .. } => formatter
                .debug_struct("Els2ClientAuth::Dh")
                .field("private", &"<redacted>")
                .field("public", public)
                .finish(),
        }
    }
}

impl Els2ClientAuth<'_> {
    /// Returns the scheme this credential can satisfy.
    pub const fn scheme(&self) -> Els2AuthScheme {
        match self {
            Self::Psk(_) => Els2AuthScheme::Psk,
            Self::Dh { .. } => Els2AuthScheme::Dh,
        }
    }
}

/// Recovers the `authCookie` a client's credential authorizes for `block`.
///
/// The entry order is not assumed: every entry is examined, the identifier
/// comparison is constant-time, and the first match wins. A client whose key is
/// absent, wrong, or for the other scheme gets [`Els2AuthError::NotAuthorized`]
/// in every case.
pub fn recover_auth_cookie(
    block: &AuthBlock,
    subcredential: &[u8; 32],
    published: u32,
    client: &Els2ClientAuth<'_>,
) -> Result<AuthCookie, Els2AuthError> {
    if block.scheme != client.scheme() {
        return Err(Els2AuthError::NotAuthorized);
    }
    let material = match client {
        Els2ClientAuth::Psk(psk) => {
            psk_client_material(psk, subcredential, published, block.salt())?
        }
        Els2ClientAuth::Dh { private, public } => {
            dh_client_material(private, block.salt(), public, subcredential, published)?
        }
    };
    for entry in block.entries() {
        if let Ok(cookie) = material.recover_cookie(entry) {
            return Ok(cookie);
        }
    }
    Err(Els2AuthError::NotAuthorized)
}

/// The server-side half of a destination's authorization configuration.
///
/// This is protocol material, not a control surface: it is what Plan 334 maps
/// Proposal 170 fields onto, and it deliberately knows nothing about I2CP.
#[derive(Debug)]
pub struct Els2AuthorizationServerConfig {
    scheme: Els2AuthScheme,
    psks: Vec<PskClientKey>,
    client_keys: Vec<AuthClientPublicKey>,
    max_clients: usize,
}

impl Els2AuthorizationServerConfig {
    /// Builds a pre-shared-key server configuration.
    pub fn psk(psks: Vec<PskClientKey>) -> Result<Self, Els2AuthError> {
        if psks.is_empty() {
            return Err(Els2AuthError::NoClients);
        }
        if psks.len() > MAX_ELS2_AUTH_CLIENTS {
            return Err(Els2AuthError::TooManyClients {
                actual: psks.len(),
                maximum: MAX_ELS2_AUTH_CLIENTS,
            });
        }
        Ok(Self {
            scheme: Els2AuthScheme::Psk,
            psks,
            client_keys: Vec::new(),
            max_clients: MAX_ELS2_AUTH_CLIENTS,
        })
    }

    /// Builds a Diffie-Hellman server configuration.
    pub fn dh(client_keys: Vec<AuthClientPublicKey>) -> Result<Self, Els2AuthError> {
        if client_keys.is_empty() {
            return Err(Els2AuthError::NoClients);
        }
        if client_keys.len() > MAX_ELS2_AUTH_CLIENTS {
            return Err(Els2AuthError::TooManyClients {
                actual: client_keys.len(),
                maximum: MAX_ELS2_AUTH_CLIENTS,
            });
        }
        Ok(Self {
            scheme: Els2AuthScheme::Dh,
            psks: Vec::new(),
            client_keys,
            max_clients: MAX_ELS2_AUTH_CLIENTS,
        })
    }

    /// Returns the scheme this configuration publishes.
    pub const fn scheme(&self) -> Els2AuthScheme {
        self.scheme
    }

    /// Returns the configured client count.
    pub fn client_count(&self) -> usize {
        match self.scheme {
            Els2AuthScheme::Psk => self.psks.len(),
            _ => self.client_keys.len(),
        }
    }

    /// Returns the configured client ceiling.
    pub const fn max_clients(&self) -> usize {
        self.max_clients
    }

    /// Builds the authorization block for one generation.
    ///
    /// For the Diffie-Hellman scheme the caller supplies the generation-local
    /// ephemeral key, so the ephemeral keypair's lifetime is owned by the caller
    /// that also owns the keypair's erasure.
    pub fn build_block<R: TryCryptoRng + ?Sized>(
        &self,
        cookie: &AuthCookie,
        subcredential: &[u8; 32],
        published: u32,
        esk: Option<&X25519PrivateKey>,
        rng: &mut R,
    ) -> Result<AuthBlock, Els2AuthError> {
        match self.scheme {
            Els2AuthScheme::Psk => {
                let keys: Vec<&PskClientKey> = self.psks.iter().collect();
                build_psk_block(cookie, subcredential, published, &keys, rng)
            }
            Els2AuthScheme::Dh => {
                let esk = esk.ok_or(Els2AuthError::Malformed(
                    "the Diffie-Hellman scheme needs a generation-local ephemeral key",
                ))?;
                build_dh_block(
                    cookie,
                    subcredential,
                    published,
                    &self.client_keys,
                    esk,
                    rng,
                )
            }
            Els2AuthScheme::None => Err(Els2AuthError::AuthorizationNotRequested { flags: 0 }),
        }
    }
}

/// The maximum accepted length of a client display name.
///
/// Names are metadata only: the specification's derivation never sees them, and
/// this module never places one in a cryptographic input. The bound exists so a
/// control surface cannot be turned into an unbounded store by a client list.
pub const MAX_ELS2_CLIENT_NAME_LENGTH: usize = 64;

/// A client display name attached to a configured key.
///
/// The type is deliberately inert. It has no accessor that returns anything the
/// derivation could consume, and its `Eq`/`Hash` are by value, so a name cannot
/// be confused with a key.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ClientName([u8; MAX_ELS2_CLIENT_NAME_LENGTH], u8);

impl ClientName {
    /// Wraps a name of at most [`MAX_ELS2_CLIENT_NAME_LENGTH`] bytes.
    pub fn new(name: &str) -> Result<Self, Els2AuthError> {
        let bytes = name.as_bytes();
        if bytes.len() > MAX_ELS2_CLIENT_NAME_LENGTH {
            return Err(Els2AuthError::Malformed("client name too long"));
        }
        if bytes.len() > MAX_LOOKUP_SECRET_LENGTH {
            return Err(Els2AuthError::Malformed("client name too long"));
        }
        let mut buffer = [0_u8; MAX_ELS2_CLIENT_NAME_LENGTH];
        buffer[..bytes.len()].copy_from_slice(bytes);
        Ok(Self(buffer, bytes.len() as u8))
    }

    /// Borrows the name.
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.0[..self.1 as usize]).expect("name was validated as UTF-8")
    }
}

impl core::fmt::Debug for ClientName {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("ClientName")
            .field("name", &self.as_str())
            .finish()
    }
}

impl core::fmt::Display for ClientName {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A one-time random source helper for the server half, kept here so the
/// generation-local ephemeral key and the cookie are drawn together and cannot
/// be confused for one another.
pub fn draw_generation_secrets<R: TryCryptoRng + ?Sized>(
    rng: &mut R,
) -> Result<(AuthCookie, X25519PrivateKey), Els2AuthError> {
    let cookie = AuthCookie::generate(rng)?;
    let esk = X25519PrivateKey::generate(rng).map_err(|_| Els2AuthError::RandomnessUnavailable)?;
    Ok((cookie, esk))
}

/// Which side of the authorization a secret belongs to.
///
/// The role is carried with the secret rather than inferred from a byte string,
/// because the four values are all 32 bytes and are not interchangeable: a
/// server pre-shared key used in the client's derivation produces a wrong
/// identifier and, more importantly, a persistence layer that mixed them up would
/// store a client key where a server key belongs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Els2AuthSecretRole {
    /// A pre-shared key held by the server.
    ServerPsk,
    /// A Diffie-Hellman public key of a client, held by the server.
    ServerDhClientPublic,
    /// A pre-shared key held by a client.
    ClientPsk,
    /// A Diffie-Hellman private key held by a client.
    ClientDhPrivate,
}

impl Els2AuthSecretRole {
    /// Returns whether this role's value must never leave the machine.
    pub const fn is_client_secret(self) -> bool {
        matches!(self, Self::ClientPsk | Self::ClientDhPrivate)
    }
}

/// One narrow owner for an encrypted-LeaseSet2 authorization secret.
///
/// # Persistence contract
///
/// [`Els2ClientAuthSecret::to_persisted_bytes`] exists for an owner-restricted
/// store and nothing else. It is explicitly **not** a wire format, not a
/// configuration serialization, and not a password verifier:
///
/// - It is a reversible encoding of the secret itself, because the protocol
///   needs the secret on every publication and on every retrieval. A one-way
///   verifier would be useless here, and the plan forbids one.
/// - It must be stored with the same owner restrictions as a router identity, and
///   encrypted at rest by whatever that store already does. This module does not
///   implement that encryption; it refuses to pretend the bytes are safe to write
///   to an ordinary file.
/// - It must be written atomically with the destination generation that owns it,
///   so a restart cannot leave a client authorized by a previous generation's key.
///
/// Restart safety follows from the derivation rather than from any stored state:
/// a client that reconstructs its secret from the persisted bytes and reads the
/// same record decrypts it, because nothing in the derivation depends on process
/// state.
pub struct Els2ClientAuthSecret {
    role: Els2AuthSecretRole,
    bytes: Zeroizing<[u8; 32]>,
}

impl Els2ClientAuthSecret {
    /// Wraps 32 secret bytes for a role.
    pub fn new(role: Els2AuthSecretRole, bytes: [u8; 32]) -> Self {
        Self {
            role,
            bytes: Zeroizing::new(bytes),
        }
    }

    /// Returns the role this secret plays.
    pub const fn role(&self) -> Els2AuthSecretRole {
        self.role
    }

    /// Borrows the raw secret for one derivation.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }

    /// Borrows the secret as a server or client pre-shared key.
    ///
    /// A pre-shared key is the same 32 bytes on both sides, so the role check
    /// accepts either and leaves the caller's context to decide which side it is
    /// acting for. What it refuses is a Diffie-Hellman key, which is the mistake
    /// that would actually leak a private key into a public derivation.
    pub fn as_psk(&self) -> Result<PskClientKey, Els2AuthError> {
        if !matches!(
            self.role,
            Els2AuthSecretRole::ServerPsk | Els2AuthSecretRole::ClientPsk
        ) {
            return Err(Els2AuthError::WrongSecretRole {
                role: self.role,
                expected: "pre-shared key",
            });
        }
        Ok(PskClientKey::from_bytes(*self.bytes))
    }

    /// Borrows the secret as a client Diffie-Hellman private key.
    pub fn as_dh_private(&self) -> Result<X25519PrivateKey, Els2AuthError> {
        if self.role != Els2AuthSecretRole::ClientDhPrivate {
            return Err(Els2AuthError::WrongSecretRole {
                role: self.role,
                expected: "Diffie-Hellman private key",
            });
        }
        Ok(X25519PrivateKey::from_bytes(*self.bytes))
    }

    /// Borrows the secret as a client Diffie-Hellman public key.
    pub fn as_dh_public(&self) -> Result<AuthClientPublicKey, Els2AuthError> {
        if self.role != Els2AuthSecretRole::ServerDhClientPublic {
            return Err(Els2AuthError::WrongSecretRole {
                role: self.role,
                expected: "Diffie-Hellman public key",
            });
        }
        Ok(AuthClientPublicKey::from_bytes(*self.bytes))
    }

    /// Encodes the secret for an owner-restricted store.
    ///
    /// The role is prefixed so a store that mixes generations or roles cannot
    /// silently reinterpret one secret as another.
    pub fn to_persisted_bytes(&self) -> Zeroizing<Vec<u8>> {
        let mut out = Zeroizing::new(Vec::with_capacity(33));
        out.push(secret_role_tag(self.role));
        out.extend_from_slice(&self.bytes[..]);
        out
    }

    /// Decodes a secret written by [`Self::to_persisted_bytes`].
    pub fn from_persisted_bytes(input: &[u8]) -> Result<Self, Els2AuthError> {
        if input.len() != 33 {
            return Err(Els2AuthError::Malformed("persisted secret length"));
        }
        let role = secret_role_from_tag(input[0])?;
        let mut bytes = [0_u8; 32];
        bytes.copy_from_slice(&input[1..]);
        Ok(Self {
            role,
            bytes: Zeroizing::new(bytes),
        })
    }
}

impl core::fmt::Debug for Els2ClientAuthSecret {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("Els2ClientAuthSecret")
            .field("role", &self.role)
            .field("secret", &"<redacted>")
            .finish()
    }
}

fn secret_role_tag(role: Els2AuthSecretRole) -> u8 {
    match role {
        Els2AuthSecretRole::ServerPsk => 1,
        Els2AuthSecretRole::ServerDhClientPublic => 2,
        Els2AuthSecretRole::ClientPsk => 3,
        Els2AuthSecretRole::ClientDhPrivate => 4,
    }
}

fn secret_role_from_tag(tag: u8) -> Result<Els2AuthSecretRole, Els2AuthError> {
    match tag {
        1 => Ok(Els2AuthSecretRole::ServerPsk),
        2 => Ok(Els2AuthSecretRole::ServerDhClientPublic),
        3 => Ok(Els2AuthSecretRole::ClientPsk),
        4 => Ok(Els2AuthSecretRole::ClientDhPrivate),
        _ => Err(Els2AuthError::Malformed("persisted secret role tag")),
    }
}
