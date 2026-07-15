//! Bounded codecs for the common I2P identity, addressing, and lease records.
//!
//! This module follows the pinned common-structures source listed in
//! `specs/SOURCES.md` (I2P website commit
//! `88596022920bdf99f27db27688faf4f204792fcd`) and the common-structure
//! dossier in `specs/protocols/01-common-identity-crypto.md`. It implements
//! structural validation and canonical encoding only. It does not implement
//! signatures, encryption, transport state machines, freshness policy, or
//! capability advertisement.
//!
//! Parsed signed records retain the exact bytes preceding their signature.
//! Callers can therefore pass [`RouterInfo::signed_bytes`] or
//! [`LeaseSet::signed_bytes`] to a later cryptographic verifier instead of
//! silently verifying a reserialized semantic value.

use std::{cmp::Ordering, fmt};

use sha2::{Digest, Sha256};

use crate::{CodecError, DecodeCursor, EncodeBuffer, decode_exact, encode_to_vec};

/// Maximum total size accepted for a common structure by the initial model.
pub const MAX_COMMON_STRUCTURE_SIZE: usize = 1024 * 1024;
/// Maximum body size of a Mapping, excluding its two-byte size field.
pub const MAX_MAPPING_BODY_SIZE: usize = u16::MAX as usize;
/// Maximum number of RouterAddress entries in a RouterInfo.
pub const MAX_ROUTER_ADDRESSES: usize = u8::MAX as usize;
/// Maximum number of classic Lease or Lease2 entries.
pub const MAX_LEASES: usize = 16;
/// Maximum number of encryption keys in the deferred LeaseSet2 model.
pub const MAX_ENCRYPTION_KEYS: usize = 8;

const KEY_AREA_SIZE: usize = 384;
const LEGACY_PUBLIC_KEY_SIZE: usize = 256;
const LEGACY_SIGNING_KEY_SIZE: usize = 128;

fn invalid(offset: usize, context: &'static str) -> CodecError {
    CodecError::InvalidFieldValue { offset, context }
}

fn unsupported(offset: usize, context: &'static str, value: u64) -> CodecError {
    CodecError::Unsupported {
        offset,
        context,
        value,
    }
}

fn take_array<const N: usize>(cursor: &mut DecodeCursor<'_>) -> Result<[u8; N], CodecError> {
    cursor
        .take(N)?
        .try_into()
        .map_err(|_| invalid(cursor.offset(), "fixed-size byte field"))
}

fn java_string_cmp(left: &str, right: &str) -> Ordering {
    left.encode_utf16().cmp(right.encode_utf16())
}

fn validate_text(value: &str, allow_empty: bool, context: &'static str) -> Result<(), CodecError> {
    let length = value.len();
    if (!allow_empty && length == 0) || length > u8::MAX as usize {
        return Err(CodecError::LengthExceeded {
            offset: 0,
            declared: length,
            maximum: u8::MAX as usize,
            context,
        });
    }
    if value.chars().any(char::is_control) {
        return Err(invalid(0, context));
    }
    Ok(())
}

mod certificate;
mod date;
mod hash;
mod identity;
mod keys;
mod lease;
mod mapping;
mod router_address;
mod router_info;

pub use certificate::*;
pub use date::*;
pub use hash::*;
pub use identity::*;
pub use keys::*;
pub use lease::*;
pub use mapping::*;
pub use router_address::*;
pub use router_info::*;

#[cfg(test)]
mod tests {
    use super::*;

    const MAX: usize = MAX_COMMON_STRUCTURE_SIZE;

    fn key_and_cert(signing_type: SigningKeyType, crypto_type: CryptoKeyType) -> KeyAndCert {
        let public_len = crypto_type.public_key_len().unwrap();
        let signing_len = signing_type.public_key_len().unwrap();
        let certificate =
            if signing_type == SigningKeyType::DsaSha1 && crypto_type == CryptoKeyType::ElGamal {
                Certificate::Null
            } else {
                Certificate::Key(KeyCertificate::for_types(signing_type, crypto_type).unwrap())
            };
        let padding_len = KEY_AREA_SIZE - public_len - signing_len.min(LEGACY_SIGNING_KEY_SIZE);
        KeyAndCert::new(
            PublicKey::new(crypto_type, vec![0x11; public_len]).unwrap(),
            SigningPublicKey::new(signing_type, vec![0x22; signing_len]).unwrap(),
            vec![0x33; padding_len],
            certificate,
        )
        .unwrap()
    }

    fn ed_router_identity() -> RouterIdentity {
        RouterIdentity::new(key_and_cert(
            SigningKeyType::EdDsaSha512Ed25519,
            CryptoKeyType::X25519,
        ))
        .unwrap()
    }

    fn ed_destination() -> Destination {
        Destination::new(key_and_cert(
            SigningKeyType::EdDsaSha512Ed25519,
            CryptoKeyType::X25519,
        ))
        .unwrap()
    }

    #[test]
    fn fixed_mapping_encoding_is_canonical_and_sorted() {
        let mapping = Mapping::from_entries(vec![
            ("b".to_owned(), "2".to_owned()),
            ("a".to_owned(), "1".to_owned()),
        ])
        .unwrap();
        let expected = b"\x00\x0c\x01a=\x011;\x01b=\x012;";
        assert_eq!(mapping.encode_to_vec(MAX).unwrap(), expected);
        assert_eq!(Mapping::decode(expected, MAX).unwrap(), mapping);
    }

    #[test]
    fn mapping_rejects_duplicates_and_noncanonical_order() {
        let duplicate = b"\x00\x0c\x01a=\x011;\x01a=\x012;";
        assert!(matches!(
            Mapping::decode(duplicate, MAX),
            Err(CodecError::DuplicateField { .. })
        ));
        let unsorted = b"\x00\x0c\x01b=\x011;\x01a=\x012;";
        assert!(matches!(
            Mapping::decode(unsorted, MAX),
            Err(CodecError::NonCanonical { .. })
        ));
    }

    #[test]
    fn primitive_vectors_and_unknown_algorithm_paths_are_explicit() {
        assert_eq!(
            Date::from_millis(0x0102_0304_0506_0708)
                .encode_to_vec(8)
                .unwrap(),
            [1, 2, 3, 4, 5, 6, 7, 8]
        );
        assert_eq!(
            Hash::digest(b"").as_bytes(),
            &[
                0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14, 0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f,
                0xb9, 0x24, 0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c, 0xa4, 0x95, 0x99, 0x1b,
                0x78, 0x52, 0xb8, 0x55,
            ]
        );
        assert_eq!(
            SigningKeyType::from_code(0x1234),
            SigningKeyType::Unknown(0x1234)
        );
        assert!(matches!(
            PublicKey::new(CryptoKeyType::Unknown(55), vec![]),
            Err(CodecError::Unsupported { .. })
        ));
        assert!(matches!(
            Certificate::decode(&[5, 0, 4, 0x12, 0x34, 0, 4], MAX),
            Err(CodecError::Unsupported { .. })
        ));
    }

    #[test]
    fn key_certificate_excess_signing_material_round_trips() {
        let signing_type = SigningKeyType::EcdsaSha512P521;
        let crypto_type = CryptoKeyType::ElGamal;
        let signing_bytes = (0..132).map(|value| value as u8).collect::<Vec<_>>();
        let certificate = Certificate::Key(
            KeyCertificate::new(
                signing_type,
                crypto_type,
                signing_bytes[128..].to_vec(),
                Vec::new(),
            )
            .unwrap(),
        );
        let keys = KeyAndCert::new(
            PublicKey::new(crypto_type, vec![0x10; 256]).unwrap(),
            SigningPublicKey::new(signing_type, signing_bytes).unwrap(),
            Vec::new(),
            certificate,
        )
        .unwrap();
        let encoded = keys.encode_to_vec(MAX).unwrap();
        assert_eq!(KeyAndCert::decode(&encoded, MAX).unwrap(), keys);
    }

    #[test]
    fn key_certificate_identity_and_destination_round_trip() {
        let identity = ed_router_identity();
        let encoded = identity.encode_to_vec(MAX).unwrap();
        assert_eq!(encoded.len(), 391);
        assert_eq!(RouterIdentity::decode(&encoded, MAX).unwrap(), identity);
        assert_eq!(
            Destination::decode(&ed_destination().encode_to_vec(MAX).unwrap(), MAX).unwrap(),
            ed_destination()
        );
        assert_ne!(identity.hash().unwrap(), Hash::digest(b""));
    }

    #[test]
    fn identity_truncation_is_rejected_at_every_boundary() {
        let encoded = ed_router_identity().encode_to_vec(MAX).unwrap();
        for end in 0..encoded.len() {
            assert!(
                RouterIdentity::decode(&encoded[..end], MAX).is_err(),
                "prefix {end}"
            );
        }
    }

    #[test]
    fn router_address_round_trip_and_typed_options() {
        let mut builder = Mapping::builder();
        builder
            .insert("host".to_owned(), "127.0.0.1".to_owned())
            .unwrap();
        builder
            .insert("port".to_owned(), "1234".to_owned())
            .unwrap();
        let address = RouterAddress::new(
            10,
            Date::from_millis(0),
            "NTCP2".to_owned(),
            builder.build().unwrap(),
        )
        .unwrap();
        let encoded = address.encode_to_vec(MAX).unwrap();
        assert_eq!(RouterAddress::decode(&encoded, MAX).unwrap(), address);
    }

    #[test]
    fn router_info_retains_signed_region_and_rejects_trailing_bytes() {
        let identity = ed_router_identity();
        let mut options = Mapping::builder();
        options.insert("caps".to_owned(), "Nf".to_owned()).unwrap();
        options
            .insert("router.version".to_owned(), "0.9.68".to_owned())
            .unwrap();
        let info = RouterInfo::new(
            identity,
            Date::from_millis(123),
            Vec::new(),
            Vec::new(),
            options.build().unwrap(),
            SignatureValue::new(SigningKeyType::EdDsaSha512Ed25519, vec![0x44; 64]).unwrap(),
        )
        .unwrap();
        let encoded = info.encode_to_vec(MAX).unwrap();
        let decoded = RouterInfo::decode(&encoded, MAX).unwrap();
        assert_eq!(decoded.signed_bytes(), info.signed_bytes());
        assert_eq!(decoded.encode_to_vec(MAX).unwrap(), encoded);
        assert_eq!(
            decoded.protocol_version().unwrap().unwrap().as_str(),
            "0.9.68"
        );
        assert_eq!(decoded.capabilities().unwrap().unwrap().as_str(), "Nf");
        let mut trailing = encoded.clone();
        trailing.push(0);
        assert!(matches!(
            RouterInfo::decode(&trailing, MAX),
            Err(CodecError::Truncated { .. }) | Err(CodecError::TrailingBytes { .. })
        ));
    }

    #[test]
    fn lease_and_classic_leaseset_round_trip() {
        let destination = ed_destination();
        let lease = Lease::new(Hash::from_bytes([0x55; 32]), 7, Date::from_millis(99));
        let lease_bytes = lease.encode_to_vec(MAX).unwrap();
        assert_eq!(lease_bytes.len(), 44);
        assert_eq!(Lease::decode(&lease_bytes, MAX).unwrap(), lease);
        let set = LeaseSet::new(
            destination,
            PublicKey::new(CryptoKeyType::ElGamal, vec![0x66; 256]).unwrap(),
            SigningPublicKey::new(SigningKeyType::EdDsaSha512Ed25519, vec![0x77; 32]).unwrap(),
            vec![lease],
            SignatureValue::new(SigningKeyType::EdDsaSha512Ed25519, vec![0x88; 64]).unwrap(),
        )
        .unwrap();
        let encoded = set.encode_to_vec(MAX).unwrap();
        let decoded = LeaseSet::decode(&encoded, MAX).unwrap();
        assert_eq!(decoded.signed_bytes(), set.signed_bytes());
        assert_eq!(decoded.encode_to_vec(MAX).unwrap(), encoded);
        assert!(matches!(
            decode_lease_set_variant(3, &encoded, MAX),
            Err(CodecError::Unsupported { value: 3, .. })
        ));
    }
}
