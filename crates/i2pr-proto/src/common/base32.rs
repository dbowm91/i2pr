//! Base 32 address encoding and the encrypted-service (`b33`) address form.
//!
//! Two address families share one base-32 alphabet and one suffix:
//!
//! - the **ordinary** 52-character form, which encodes a 32-byte Destination
//!   hash. i2pr already treats these as opaque text; nothing here changes,
//!   reorders, or revalidates them;
//! - the **encrypted-service** form, which was added with the encrypted
//!   LeaseSet2 specification. A traditional base-32 address cannot be used for
//!   an encrypted service because it carries only the hash of the Destination,
//!   and the client needs the unblinded *public key* to derive the daily
//!   blinded key. The encrypted-service form therefore puts the public key,
//!   the unblinded signature type, and the blinded signature type into the
//!   address.
//!
//! ```text
//! data = ((1 byte flags || 1 byte unblinded sigtype || 1 byte blinded sigtype) XOR checksum)
//!        || 32 byte public key
//! address = Base32Encode(data) || ".b32.i2p"
//! ```
//!
//! The checksum is a CRC-32 folded into the first three bytes, exactly as the
//! encrypted LeaseSet2 specification and Proposal 149 describe. Both pinned
//! references agree on the coverage, so the folding rule is not an i2pr
//! choice.
//!
//! Two properties are load-bearing and are enforced on every decode:
//!
//! - **Canonical form.** Base 32 is case-insensitive, but the unused trailing
//!   bits of the final encoded group must be zero. A non-canonical encoding is
//!   rejected rather than normalized, so one address has exactly one encoding.
//! - **No secret material.** The form has room for a flag saying a blinding
//!   secret or a per-client key is *required*; it never carries one. A secret
//!   can be discovered from a leaked address, so no such value is ever encoded.

use std::fmt;

/// The shared base-32 address suffix, including the leading dot.
pub const B32_SUFFIX: &str = ".b32.i2p";
/// Character count of an ordinary 32-byte-hash base-32 address.
pub const B32_HASH_CHARS: usize = 52;
/// Decoded byte count of an ordinary base-32 address.
pub const B32_HASH_DECODED_LEN: usize = 32;
/// Character count of an encrypted-service address with one-byte sigtypes.
pub const B32_SERVICE_CHARS: usize = 56;
/// Decoded byte count of an encrypted-service address with one-byte sigtypes.
pub const B32_SERVICE_DECODED_LEN: usize = 35;
/// Character count of an encrypted-service address with two-byte sigtypes.
pub const B32_SERVICE_WIDE_CHARS: usize = 60;
/// Decoded byte count of an encrypted-service address with two-byte sigtypes.
pub const B32_SERVICE_WIDE_DECODED_LEN: usize = 37;
/// Encoded length of a 32-byte signing public key.
pub const B32_SERVICE_KEY_LEN: usize = 32;
/// The RFC 4648 base-32 alphabet, lowercase.
const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

/// Errors returned by the base-32 and encrypted-service address codecs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Base32Error {
    /// The address did not carry the `.b32.i2p` suffix.
    MissingSuffix,
    /// The encoded character count matched no recognized address form.
    UnexpectedLength {
        /// Actual character count, excluding the suffix.
        actual: usize,
        /// The recognized character counts.
        expected: &'static str,
    },
    /// A character outside the base-32 alphabet was present.
    InvalidCharacter {
        /// Zero-based index of the offending character.
        index: usize,
    },
    /// The unused trailing bits of the final encoded group were not zero.
    NonCanonicalTrailingBits,
    /// The address decoded to a valid value but is not its canonical encoding.
    NonCanonicalEncoding,
    /// A flag bit reserved for future use was set.
    ReservedFlagSet {
        /// The offending reserved-bit mask.
        mask: u8,
    },
    /// The sigtype-width flag disagreed with the encoded length.
    SigtypeWidthMismatch,
    /// The unblinded signature type was neither 7 nor 11.
    UnsupportedUnblindedSigtype {
        /// The rejected value.
        value: u16,
    },
    /// The blinded signature type was not 11.
    UnsupportedBlindedSigtype {
        /// The rejected value.
        value: u16,
    },
    /// A one-byte sigtype form carried a value that does not fit in one byte.
    SigtypeTooWide {
        /// The rejected value.
        value: u16,
    },
    /// The address carried the wrong number of decoded public-key bytes.
    PublicKeyLength {
        /// Actual length.
        actual: usize,
        /// Required length.
        expected: usize,
    },
}

impl fmt::Display for Base32Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSuffix => {
                formatter.write_str("base32 address is missing the .b32.i2p suffix")
            }
            Self::UnexpectedLength { actual, expected } => write!(
                formatter,
                "base32 address has {actual} characters; expected {expected}"
            ),
            Self::InvalidCharacter { index } => write!(
                formatter,
                "base32 address contains a character outside the alphabet at index {index}"
            ),
            Self::NonCanonicalTrailingBits => {
                formatter.write_str("base32 address is not canonical: unused trailing bits are set")
            }
            Self::NonCanonicalEncoding => formatter
                .write_str("encrypted-service address is not the canonical encoding of its value"),
            Self::ReservedFlagSet { mask } => write!(
                formatter,
                "encrypted-service address sets reserved flag bit {mask:#04x}"
            ),
            Self::SigtypeWidthMismatch => formatter.write_str(
                "encrypted-service address sigtype width flag disagrees with its length",
            ),
            Self::UnsupportedUnblindedSigtype { value } => write!(
                formatter,
                "encrypted-service address unblinded sigtype {value} is neither 7 nor 11"
            ),
            Self::UnsupportedBlindedSigtype { value } => write!(
                formatter,
                "encrypted-service address blinded sigtype {value} is not 11"
            ),
            Self::SigtypeTooWide { value } => write!(
                formatter,
                "sigtype {value} does not fit the one-byte address form"
            ),
            Self::PublicKeyLength { actual, expected } => write!(
                formatter,
                "encrypted-service address public key length {actual} is not {expected}"
            ),
        }
    }
}

impl std::error::Error for Base32Error {}

/// Flag bit: the two sigtype fields are two bytes each rather than one.
pub const B32_FLAG_WIDE_SIGTYPES: u8 = 0x01;
/// Flag bit: deriving the blinded public key requires a blinding secret.
pub const B32_FLAG_REQUIRES_BLINDING_SECRET: u8 = 0x02;
/// Flag bit: decrypting the record requires a per-client private key.
pub const B32_FLAG_REQUIRES_CLIENT_KEY: u8 = 0x04;
/// Flag bits that must be zero in every address i2pr accepts.
pub const B32_FLAG_RESERVED_MASK: u8 = 0xf8;
/// The only unblinded signing key types an encrypted service may use.
pub const B32_UNBLINDED_SIGTYPE_ED25519: u16 = 7;
/// The only unblinded signing key type a new encrypted service should use.
pub const B32_UNBLINDED_SIGTYPE_RED25519: u16 = 11;
/// The only blinded signing key type an encrypted service may use.
pub const B32_BLINDED_SIGTYPE: u16 = 11;

/// Computes the CRC-32 (reflected IEEE 802.3 polynomial) of `data`.
///
/// The encrypted-service address checksum uses this exact function, including
/// the final complement, so the value is not interchangeable with a raw
/// polynomial residue.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320_u32 & mask);
        }
    }
    !crc
}

/// Encodes `bytes` as unpadded lowercase base 32.
pub fn base32_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(5) * 8);
    let mut accumulator: u32 = 0;
    let mut bits = 0_u32;
    for byte in bytes {
        accumulator = (accumulator << 8) | u32::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            let index = ((accumulator >> bits) & 0x1f) as usize;
            out.push(char::from(ALPHABET[index]));
        }
    }
    if bits > 0 {
        let index = ((accumulator << (5 - bits)) & 0x1f) as usize;
        out.push(char::from(ALPHABET[index]));
    }
    out
}

/// Decodes exactly `expected_decoded_len` bytes of canonical base 32.
///
/// The input is case-insensitive, but the unused trailing bits of the final
/// group must be zero: a non-canonical encoding is an error, not something to
/// normalize. Callers that need a different length pass the matching constant.
pub fn base32_decode(text: &str, expected_decoded_len: usize) -> Result<Vec<u8>, Base32Error> {
    let expected_chars = (expected_decoded_len * 8).div_ceil(5);
    if text.chars().count() != expected_chars {
        return Err(Base32Error::UnexpectedLength {
            actual: text.chars().count(),
            expected: "a form whose length matches the requested decoded size",
        });
    }
    let mut out = Vec::with_capacity(expected_decoded_len);
    let mut accumulator: u32 = 0;
    let mut bits = 0_u32;
    for (index, character) in text.chars().enumerate() {
        let value = base32_value(character).ok_or(Base32Error::InvalidCharacter { index })?;
        accumulator = (accumulator << 5) | value;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push(((accumulator >> bits) & 0xff) as u8);
        }
    }
    if bits > 0 && (accumulator & ((1 << bits) - 1)) != 0 {
        return Err(Base32Error::NonCanonicalTrailingBits);
    }
    if out.len() != expected_decoded_len {
        return Err(Base32Error::UnexpectedLength {
            actual: out.len(),
            expected: "a form whose length matches the requested decoded size",
        });
    }
    Ok(out)
}

fn base32_value(character: char) -> Option<u32> {
    let lowered = character.to_ascii_lowercase() as u8;
    ALPHABET
        .iter()
        .position(|candidate| *candidate == lowered)
        .map(|index| index as u32)
}

/// An encrypted-service base-32 address (`b33`).
///
/// The value carries the **unblinded** signing public key plus both signature
/// types, so a client can derive the daily blinded key from the address alone.
/// It is not a Destination hash and must never be treated as one.
///
/// The type is validated on construction and on decode, so a caller cannot
/// hold an address with a reserved flag, a mismatched sigtype width, an
/// unsupported sigtype, or a non-canonical encoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EncryptedServiceAddress {
    flags: u8,
    unblinded_sigtype: u16,
    blinded_sigtype: u16,
    public_key: [u8; B32_SERVICE_KEY_LEN],
}

impl EncryptedServiceAddress {
    /// Builds an address from a signing public key and its signature types.
    ///
    /// `unblinded_sigtype` must be 7 or 11 and `blinded_sigtype` must be 11.
    /// `requires_blinding_secret` and `requires_client_key` only set the
    /// corresponding flag; neither value is ever encoded.
    pub fn new(
        unblinded_sigtype: u16,
        blinded_sigtype: u16,
        public_key: [u8; B32_SERVICE_KEY_LEN],
        requires_blinding_secret: bool,
        requires_client_key: bool,
    ) -> Result<Self, Base32Error> {
        if unblinded_sigtype != B32_UNBLINDED_SIGTYPE_ED25519
            && unblinded_sigtype != B32_UNBLINDED_SIGTYPE_RED25519
        {
            return Err(Base32Error::UnsupportedUnblindedSigtype {
                value: unblinded_sigtype,
            });
        }
        if blinded_sigtype != B32_BLINDED_SIGTYPE {
            return Err(Base32Error::UnsupportedBlindedSigtype {
                value: blinded_sigtype,
            });
        }
        let mut flags = 0_u8;
        if requires_blinding_secret {
            flags |= B32_FLAG_REQUIRES_BLINDING_SECRET;
        }
        if requires_client_key {
            flags |= B32_FLAG_REQUIRES_CLIENT_KEY;
        }
        Ok(Self {
            flags,
            unblinded_sigtype,
            blinded_sigtype,
            public_key,
        })
    }

    /// Decodes an encrypted-service address from a hostname.
    ///
    /// The `.b32.i2p` suffix is required, the input is case-insensitive, and
    /// both the one-byte and two-byte sigtype forms are accepted. An ordinary
    /// 52-character address is rejected as [`Base32Error::UnexpectedLength`]
    /// rather than silently reinterpreted.
    pub fn from_text(host: &str) -> Result<Self, Base32Error> {
        let lowered = host.to_ascii_lowercase();
        let body = lowered
            .strip_suffix(B32_SUFFIX)
            .ok_or(Base32Error::MissingSuffix)?;
        let (decoded_len, wide) = match body.chars().count() {
            B32_SERVICE_CHARS => (B32_SERVICE_DECODED_LEN, false),
            B32_SERVICE_WIDE_CHARS => (B32_SERVICE_WIDE_DECODED_LEN, true),
            other => {
                return Err(Base32Error::UnexpectedLength {
                    actual: other,
                    expected: "52, 56, or 60",
                });
            }
        };
        let decoded = base32_decode(body, decoded_len)?;
        let key_offset = if wide { 5 } else { 3 };
        let checksum = crc32(&decoded[3..]);
        let mut data = decoded;
        data[0] ^= (checksum & 0xff) as u8;
        data[1] ^= ((checksum >> 8) & 0xff) as u8;
        data[2] ^= ((checksum >> 16) & 0xff) as u8;

        let flags = data[0];
        if flags & B32_FLAG_RESERVED_MASK != 0 {
            return Err(Base32Error::ReservedFlagSet {
                mask: flags & B32_FLAG_RESERVED_MASK,
            });
        }
        if (flags & B32_FLAG_WIDE_SIGTYPES != 0) != wide {
            return Err(Base32Error::SigtypeWidthMismatch);
        }
        let (unblinded_sigtype, blinded_sigtype) = if wide {
            (
                u16::from(data[1]) | (u16::from(data[2]) << 8),
                u16::from(data[3]) | (u16::from(data[4]) << 8),
            )
        } else {
            (u16::from(data[1]), u16::from(data[2]))
        };
        if unblinded_sigtype != B32_UNBLINDED_SIGTYPE_ED25519
            && unblinded_sigtype != B32_UNBLINDED_SIGTYPE_RED25519
        {
            return Err(Base32Error::UnsupportedUnblindedSigtype {
                value: unblinded_sigtype,
            });
        }
        if blinded_sigtype != B32_BLINDED_SIGTYPE {
            return Err(Base32Error::UnsupportedBlindedSigtype {
                value: blinded_sigtype,
            });
        }
        let public_key: [u8; B32_SERVICE_KEY_LEN] =
            data[key_offset..]
                .try_into()
                .map_err(|_| Base32Error::PublicKeyLength {
                    actual: data.len().saturating_sub(key_offset),
                    expected: B32_SERVICE_KEY_LEN,
                })?;
        let address = Self {
            flags: flags & !B32_FLAG_WIDE_SIGTYPES,
            unblinded_sigtype,
            blinded_sigtype,
            public_key,
        };
        // The checksum fold makes the leading three bytes a function of the
        // sigtypes and the key, so an alternative encoding of the *same*
        // address exists. Canonical form is enforced by re-encoding: only the
        // one string this type produces is accepted, so an address has exactly
        // one text form and a mutated leading group is a rejection rather than
        // a silent normalization.
        let canonical = if wide {
            address.to_text_wide()
        } else {
            address.to_text()?
        };
        if canonical != lowered {
            return Err(Base32Error::NonCanonicalEncoding);
        }
        Ok(address)
    }

    /// Encodes the address in the narrow (one-byte sigtype) form.
    ///
    /// i2pr only ever emits this form: both supported unblinded sigtypes fit in
    /// one byte, so the wide form exists to be parsed, not published.
    pub fn to_text(&self) -> Result<String, Base32Error> {
        if u8::try_from(self.unblinded_sigtype).is_err() {
            return Err(Base32Error::SigtypeTooWide {
                value: self.unblinded_sigtype,
            });
        }
        let mut data = [0_u8; B32_SERVICE_DECODED_LEN];
        data[0] = self.flags & !B32_FLAG_WIDE_SIGTYPES;
        data[1] = self.unblinded_sigtype as u8;
        data[2] = self.blinded_sigtype as u8;
        data[3..].copy_from_slice(&self.public_key);
        Ok(format!(
            "{}{B32_SUFFIX}",
            base32_encode(&fold_checksum(&mut data))
        ))
    }

    /// Encodes the address in the wide (two-byte sigtype) form.
    ///
    /// Provided for completeness and for differential testing. i2pr does not
    /// publish wide-form addresses; see [`Self::to_text`].
    pub fn to_text_wide(&self) -> String {
        let mut data = [0_u8; B32_SERVICE_WIDE_DECODED_LEN];
        data[0] = self.flags | B32_FLAG_WIDE_SIGTYPES;
        data[1] = (self.unblinded_sigtype & 0xff) as u8;
        data[2] = (self.unblinded_sigtype >> 8) as u8;
        data[3] = (self.blinded_sigtype & 0xff) as u8;
        data[4] = (self.blinded_sigtype >> 8) as u8;
        data[5..].copy_from_slice(&self.public_key);
        format!("{}{B32_SUFFIX}", base32_encode(&fold_checksum(&mut data)))
    }

    /// Returns the address flags with the sigtype-width bit cleared.
    pub const fn flags(&self) -> u8 {
        self.flags & !B32_FLAG_WIDE_SIGTYPES
    }

    /// Returns whether deriving the blinded key requires a blinding secret.
    pub const fn requires_blinding_secret(&self) -> bool {
        self.flags & B32_FLAG_REQUIRES_BLINDING_SECRET != 0
    }

    /// Returns whether decrypting the record requires a per-client key.
    pub const fn requires_client_key(&self) -> bool {
        self.flags & B32_FLAG_REQUIRES_CLIENT_KEY != 0
    }

    /// Returns the unblinded signing signature type (7 or 11).
    pub const fn unblinded_sigtype(&self) -> u16 {
        self.unblinded_sigtype
    }

    /// Returns the blinded signing signature type, always 11.
    pub const fn blinded_sigtype(&self) -> u16 {
        self.blinded_sigtype
    }

    /// Borrows the unblinded signing public key.
    pub const fn public_key(&self) -> &[u8; B32_SERVICE_KEY_LEN] {
        &self.public_key
    }
}

fn fold_checksum(data: &mut [u8]) -> Vec<u8> {
    let checksum = crc32(&data[3..]);
    data[0] ^= (checksum & 0xff) as u8;
    data[1] ^= ((checksum >> 8) & 0xff) as u8;
    data[2] ^= ((checksum >> 16) & 0xff) as u8;
    data.to_vec()
}

/// Returns whether `host` is an encrypted-service address rather than an
/// ordinary 52-character base-32 address.
///
/// This is a shape test, not a validation: it never reports a malformed
/// encrypted-service address as ordinary, because the length distinguishes the
/// two forms.
pub fn is_encrypted_service_address(host: &str) -> bool {
    let lowered = host.to_ascii_lowercase();
    let Some(body) = lowered.strip_suffix(B32_SUFFIX) else {
        return false;
    };
    matches!(
        body.chars().count(),
        B32_SERVICE_CHARS | B32_SERVICE_WIDE_CHARS
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(seed: u8) -> [u8; B32_SERVICE_KEY_LEN] {
        let mut bytes = [0_u8; B32_SERVICE_KEY_LEN];
        for (index, slot) in bytes.iter_mut().enumerate() {
            *slot = seed.wrapping_add(index as u8).wrapping_mul(31);
        }
        bytes
    }

    #[test]
    fn crc32_matches_the_published_check_value() {
        // The CRC-32 check value for "123456789" pins the polynomial,
        // reflection, initial value, and final complement together.
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn base32_round_trips_every_accepted_length() {
        for len in [
            B32_HASH_DECODED_LEN,
            B32_SERVICE_DECODED_LEN,
            B32_SERVICE_WIDE_DECODED_LEN,
        ] {
            let bytes: Vec<u8> = (0..len).map(|index| index as u8).collect();
            let encoded = base32_encode(&bytes);
            assert_eq!(
                base32_decode(&encoded, len).expect("decode"),
                bytes,
                "length {len}"
            );
        }
    }

    #[test]
    fn base32_matches_rfc4648_lowercase_vectors() {
        assert_eq!(base32_encode(b""), "");
        assert_eq!(base32_encode(b"f"), "my");
        assert_eq!(base32_encode(b"fo"), "mzxq");
        assert_eq!(base32_encode(b"foo"), "mzxw6");
        assert_eq!(base32_encode(b"foob"), "mzxw6yq");
        assert_eq!(base32_encode(b"fooba"), "mzxw6ytb");
        assert_eq!(base32_encode(b"foobar"), "mzxw6ytboi");
    }

    #[test]
    fn base32_rejects_non_canonical_trailing_bits() {
        // 52 characters encode 260 bits; the last 4 must be zero. Flipping one
        // of them changes the decoded prefix but must not normalize.
        let mut encoded: Vec<char> = base32_encode(&[0_u8; B32_HASH_DECODED_LEN])
            .chars()
            .collect();
        let last = encoded[51];
        let flipped = if last == 'a' { 'b' } else { 'a' };
        encoded[51] = flipped;
        let candidate: String = encoded.into_iter().collect();
        assert!(matches!(
            base32_decode(&candidate, B32_HASH_DECODED_LEN),
            Err(Base32Error::NonCanonicalTrailingBits)
        ));
    }

    #[test]
    fn base32_rejects_foreign_characters_and_wrong_lengths() {
        let mut foreign: Vec<char> = base32_encode(&[0_u8; B32_HASH_DECODED_LEN])
            .chars()
            .collect();
        foreign[9] = '1';
        let candidate: String = foreign.into_iter().collect();
        assert!(matches!(
            base32_decode(&candidate, B32_HASH_DECODED_LEN),
            Err(Base32Error::InvalidCharacter { index: 9 })
        ));
        assert!(matches!(
            base32_decode("mzxw6ytb", B32_HASH_DECODED_LEN),
            Err(Base32Error::UnexpectedLength { .. })
        ));
    }

    #[test]
    fn service_address_round_trips_narrow_and_wide_forms() {
        for unblinded in [
            B32_UNBLINDED_SIGTYPE_ED25519,
            B32_UNBLINDED_SIGTYPE_RED25519,
        ] {
            for (secret, client) in [(false, false), (true, false), (false, true), (true, true)] {
                let address = EncryptedServiceAddress::new(
                    unblinded,
                    B32_BLINDED_SIGTYPE,
                    key(0x5a),
                    secret,
                    client,
                )
                .expect("address");
                let text = address.to_text().expect("narrow text");
                assert_eq!(text.chars().count(), B32_SERVICE_CHARS + B32_SUFFIX.len());
                let decoded = EncryptedServiceAddress::from_text(&text).expect("decode");
                assert_eq!(decoded, address);
                assert_eq!(decoded.requires_blinding_secret(), secret);
                assert_eq!(decoded.requires_client_key(), client);

                let wide_text = address.to_text_wide();
                assert_eq!(
                    wide_text.chars().count(),
                    B32_SERVICE_WIDE_CHARS + B32_SUFFIX.len()
                );
                assert_eq!(
                    EncryptedServiceAddress::from_text(&wide_text).expect("wide decode"),
                    address
                );
            }
        }
    }

    #[test]
    fn service_address_is_case_insensitive_and_lowercases() {
        let address = EncryptedServiceAddress::new(
            B32_UNBLINDED_SIGTYPE_ED25519,
            B32_BLINDED_SIGTYPE,
            key(1),
            false,
            false,
        )
        .expect("address");
        let text = address.to_text().expect("text");
        let shouted = text.to_ascii_uppercase();
        assert_eq!(
            EncryptedServiceAddress::from_text(&shouted).expect("upper"),
            address
        );
    }

    #[test]
    fn service_address_rejects_missing_suffix_and_ordinary_form() {
        let text = format!("{}a{}", "mzxw6ytboi", "a".repeat(41));
        assert!(matches!(
            EncryptedServiceAddress::from_text(&text),
            Err(Base32Error::MissingSuffix)
        ));
        let ordinary = format!("{}{B32_SUFFIX}", "a".repeat(52));
        assert!(matches!(
            EncryptedServiceAddress::from_text(&ordinary),
            Err(Base32Error::UnexpectedLength {
                expected: "52, 56, or 60",
                ..
            })
        ));
    }

    #[test]
    fn no_single_bit_corruption_decodes_back_to_the_same_address() {
        // The CRC-32 is a typo detector folded into the leading group, not a
        // MAC. A bit flip inside the folded prefix therefore maps onto a
        // *different* address value rather than always failing, while a flip in
        // the key region fails the checksum. The invariant that must hold for
        // every flip is that no corrupted string ever decodes back to the
        // original address: the text stays a faithful function of the value.
        let address = EncryptedServiceAddress::new(
            B32_UNBLINDED_SIGTYPE_RED25519,
            B32_BLINDED_SIGTYPE,
            key(0x21),
            true,
            false,
        )
        .expect("address");
        let text = address.to_text().expect("text");
        let body = &text[..B32_SERVICE_CHARS];
        // The folded prefix covers roughly the first three bytes: 24 bits of a
        // 5-bit alphabet is 4.8 characters, so the boundary is taken at the
        // first character whose bits fall entirely inside `data[3..]`.
        const KEY_REGION_START: usize = 5;
        let mut prefix_flips_accepted_as_other = 0_usize;
        let mut key_flips_rejected = 0_usize;
        for index in 0..body.len() {
            for bit in 0..5_u32 {
                let mut characters: Vec<char> = body.chars().collect();
                let current = base32_value(characters[index]).expect("alphabet");
                let flipped = current ^ (1 << bit);
                characters[index] = char::from(ALPHABET[flipped as usize]);
                let candidate: String = characters.into_iter().collect();
                let full = format!("{candidate}{B32_SUFFIX}");
                match EncryptedServiceAddress::from_text(&full) {
                    Err(_) if index >= KEY_REGION_START => key_flips_rejected += 1,
                    Ok(decoded) => {
                        assert_ne!(
                            decoded, address,
                            "corruption at {index} bit {bit} decoded back to the original address"
                        );
                        assert_eq!(
                            decoded.to_text().expect("canonical").len(),
                            text.len(),
                            "an accepted corruption must still be canonical"
                        );
                        assert!(index < KEY_REGION_START);
                        prefix_flips_accepted_as_other += 1;
                    }
                    Err(_) => {}
                }
            }
        }
        assert_eq!(
            key_flips_rejected,
            (B32_SERVICE_CHARS - KEY_REGION_START) * 5
        );
        // Most fold-prefix flips are still caught, because most of them land on
        // a sigtype or a reserved flag bit. The ones that survive become a
        // different *valid* address, which is the documented consequence of
        // using a checksum rather than a MAC.
        assert!(prefix_flips_accepted_as_other > 0);
        assert!(prefix_flips_accepted_as_other <= KEY_REGION_START * 5);
    }

    #[test]
    fn reserved_flag_bits_are_rejected() {
        let address = EncryptedServiceAddress::new(
            B32_UNBLINDED_SIGTYPE_ED25519,
            B32_BLINDED_SIGTYPE,
            key(9),
            false,
            false,
        )
        .expect("address");
        let mut data = [0_u8; B32_SERVICE_DECODED_LEN];
        data[0] = 0x08;
        data[1] = 7;
        data[2] = 11;
        data[3..].copy_from_slice(address.public_key());
        let text = format!("{}{B32_SUFFIX}", base32_encode(&fold_checksum(&mut data)));
        assert!(matches!(
            EncryptedServiceAddress::from_text(&text),
            Err(Base32Error::ReservedFlagSet { mask: 0x08 })
        ));
    }

    #[test]
    fn sigtype_width_flag_must_agree_with_length() {
        let address = EncryptedServiceAddress::new(
            B32_UNBLINDED_SIGTYPE_ED25519,
            B32_BLINDED_SIGTYPE,
            key(4),
            false,
            false,
        )
        .expect("address");
        // Narrow bytes carrying the wide flag: 35 bytes cannot be parsed as wide.
        let mut data = [0_u8; B32_SERVICE_DECODED_LEN];
        data[0] = B32_FLAG_WIDE_SIGTYPES;
        data[1] = 7;
        data[2] = 11;
        data[3..].copy_from_slice(address.public_key());
        let text = format!("{}{B32_SUFFIX}", base32_encode(&fold_checksum(&mut data)));
        assert!(matches!(
            EncryptedServiceAddress::from_text(&text),
            Err(Base32Error::SigtypeWidthMismatch)
        ));
    }

    #[test]
    fn unsupported_sigtypes_are_rejected_at_construction_and_decode() {
        assert!(matches!(
            EncryptedServiceAddress::new(8, B32_BLINDED_SIGTYPE, key(0), false, false),
            Err(Base32Error::UnsupportedUnblindedSigtype { value: 8 })
        ));
        assert!(matches!(
            EncryptedServiceAddress::new(7, 12, key(0), false, false),
            Err(Base32Error::UnsupportedBlindedSigtype { value: 12 })
        ));
        let mut data = [0_u8; B32_SERVICE_DECODED_LEN];
        data[0] = 0;
        data[1] = 8;
        data[2] = 11;
        let text = format!("{}{B32_SUFFIX}", base32_encode(&fold_checksum(&mut data)));
        assert!(matches!(
            EncryptedServiceAddress::from_text(&text),
            Err(Base32Error::UnsupportedUnblindedSigtype { value: 8 })
        ));
    }

    #[test]
    fn classifier_separates_ordinary_and_encrypted_forms() {
        assert!(!is_encrypted_service_address(&format!(
            "{}{B32_SUFFIX}",
            "a".repeat(52)
        )));
        assert!(!is_encrypted_service_address("example.i2p"));
        assert!(is_encrypted_service_address(&format!(
            "{}{B32_SUFFIX}",
            "a".repeat(56)
        )));
        assert!(is_encrypted_service_address(&format!(
            "{}{B32_SUFFIX}",
            "a".repeat(60)
        )));
    }
}
