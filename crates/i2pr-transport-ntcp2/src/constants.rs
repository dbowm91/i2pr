//! NTCP2 constants derived from the pinned protocol dossier.
//!
//! Normative traceability: `specs/protocols/03-ntcp2.md` and the pinned NTCP2
//! source entry in `specs/SOURCES.md` (I2P website commit
//! `88596022920bdf99f27db27688faf4f204792fcd`).

/// The I2P-specific Noise protocol name used for the initial symmetric state.
pub const PROTOCOL_NAME: &[u8] = b"Noise_XKaesobfse+hs2+hs3_25519_ChaChaPoly_SHA256";
/// NTCP2 has no additional Noise prologue beyond the protocol name.
pub const PROLOGUE: &[u8] = b"";

/// X25519 public/private/shared-secret length in bytes (RFC 7748).
pub const KEY_LENGTH: usize = 32;
/// SHA-256 transcript and HMAC output length in bytes.
pub const HASH_LENGTH: usize = 32;
/// ChaCha20-Poly1305 nonce length in bytes.
pub const NONCE_LENGTH: usize = 12;
/// ChaCha20-Poly1305 authentication tag length in bytes.
pub const AUTH_TAG_LENGTH: usize = 16;
/// AES-CBC block and published NTCP2 IV length in bytes.
pub const AES_BLOCK_LENGTH: usize = 16;
/// NTCP2 option block length in handshake messages.
pub const OPTION_BLOCK_LENGTH: usize = 16;
/// Maximum encrypted frame length, including its authentication tag.
pub const MAX_FRAME_LENGTH: usize = 65_535;
/// Maximum data-phase plaintext length before the authentication tag.
pub const MAX_FRAME_PLAINTEXT: usize = MAX_FRAME_LENGTH - AUTH_TAG_LENGTH;
/// Maximum frame plus its two-byte obfuscated length prefix.
pub const MAX_WIRE_FRAME_LENGTH: usize = MAX_FRAME_LENGTH + 2;
/// Maximum SessionConfirmed part-two frame length including its tag.
pub const MAX_SESSION_CONFIRMED_PART2: usize = 65_487;
/// Maximum SessionConfirmed part-two plaintext length.
pub const MAX_SESSION_CONFIRMED_PART2_PLAINTEXT: usize = 65_471;
/// Current maximum Java non-PQ SessionRequest padding from the pinned source.
pub const MAX_SESSION_REQUEST_PADDING: usize = 880;
/// Current maximum Java non-PQ SessionCreated padding from the pinned source.
pub const MAX_SESSION_CREATED_PADDING: usize = 848;
/// The largest nonce value that may be transmitted by the protocol.
pub const MAX_NONCE: u64 = u64::MAX - 1;
/// The fixed ASCII label used for the additional SipHash key derivation.
pub const ASK_LABEL: &[u8] = b"ask";
/// The fixed ASCII label used for SipHash material derivation.
pub const SIPHASH_LABEL: &[u8] = b"siphash";
