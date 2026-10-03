//! Full-Destination text structural validation.
//!
//! A book value is I2P-Base64 text (`A-Z a-z 0-9 - ~` with `=` padding
//! only in the final quantum, length a multiple of four, within the
//! destination ceiling) decoding to bytes that
//! [`i2pr_proto::Destination::decode`] accepts exactly under the
//! workspace algorithm policy. Validation never logs or echoes the
//! material; errors are opaque by construction.

use base64ct::{Base64, Encoding};

use crate::error::AddressBookError;

/// Maximum full-Destination text length in bytes.
pub const MAX_DESTINATION_TEXT_LEN: usize = 4096;
/// Maximum decoded Destination length in bytes.
pub const MAX_DESTINATION_BYTES: usize = 4096;

/// Validates `text` as a full Destination and returns its canonical
/// decoded bytes on success.
pub fn validate_destination_text(text: &str) -> Result<Vec<u8>, AddressBookError> {
    if text.is_empty() || text.len() > MAX_DESTINATION_TEXT_LEN || !text.len().is_multiple_of(4) {
        return Err(AddressBookError::InvalidDestination);
    }
    let bytes = text.as_bytes();
    let mut padding_start: Option<usize> = None;
    for (index, byte) in bytes.iter().enumerate() {
        let valid = byte.is_ascii_alphanumeric() || *byte == b'-' || *byte == b'~' || *byte == b'=';
        if !valid {
            return Err(AddressBookError::InvalidDestination);
        }
        if *byte == b'=' {
            if padding_start.is_none() {
                padding_start = Some(index);
            }
        } else if padding_start.is_some() {
            // Padding outside the final quantum (or data after padding).
            return Err(AddressBookError::InvalidDestination);
        }
    }
    if let Some(start) = padding_start
        && text.len() - start > 2
    {
        return Err(AddressBookError::InvalidDestination);
    }
    // Translate the I2P alphabet to RFC 4648 for the reviewed decoder.
    let mut translated = Vec::with_capacity(text.len());
    translated.extend_from_slice(bytes);
    for byte in translated.iter_mut() {
        if *byte == b'-' {
            *byte = b'+';
        } else if *byte == b'~' {
            *byte = b'/';
        }
    }
    let translated_str =
        core::str::from_utf8(&translated).map_err(|_| AddressBookError::InvalidDestination)?;
    let decoded =
        Base64::decode_vec(translated_str).map_err(|_| AddressBookError::InvalidDestination)?;
    if decoded.is_empty() || decoded.len() > MAX_DESTINATION_BYTES {
        return Err(AddressBookError::InvalidDestination);
    }
    i2pr_proto::Destination::decode(&decoded, MAX_DESTINATION_BYTES)
        .map_err(|_| AddressBookError::InvalidDestination)?;
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal valid destination bytes: 384-byte key area (X25519
    /// public at the head, Ed25519 signing at the tail, zero padding
    /// between) plus a type-5 key certificate for (Ed25519, X25519).
    fn minimal_destination_bytes() -> Vec<u8> {
        let mut out = vec![0u8; 384];
        out.extend_from_slice(&[5u8, 0, 4, 0, 7, 0, 4]);
        out
    }

    fn to_text(bytes: &[u8]) -> String {
        let mapped: Vec<u8> = Base64::encode_string(bytes)
            .bytes()
            .map(|byte| match byte {
                b'+' => b'-',
                b'/' => b'~',
                other => other,
            })
            .collect();
        String::from_utf8(mapped).expect("alphabet stays ASCII")
    }

    #[test]
    fn minimal_destination_round_trips() {
        let bytes = minimal_destination_bytes();
        let text = to_text(&bytes);
        assert_eq!(validate_destination_text(&text).expect("valid"), bytes);
    }

    #[test]
    fn malformed_text_fails() {
        for text in [
            "",
            "!!!=",
            "abcd+efg=",
            "abcd/efg=",
            "abc",
            "abcde",
            "ab=cdef=",
            "abcd====",
        ] {
            assert!(
                validate_destination_text(text).is_err(),
                "must reject {text:?}"
            );
        }
    }

    #[test]
    fn undecodable_payload_fails() {
        // Valid alphabet, decodes, but far too short for a destination.
        assert!(validate_destination_text("QUJD").is_err());
        // Minimal bytes with a truncated certificate.
        let mut bytes = minimal_destination_bytes();
        bytes.pop();
        assert!(validate_destination_text(&to_text(&bytes)).is_err());
    }

    #[test]
    fn oversized_text_fails() {
        let big = format!("{}.b32.i2p", "a".repeat(4096));
        assert!(validate_destination_text(&big).is_err());
    }
}
