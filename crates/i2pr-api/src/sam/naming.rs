//! Runtime-neutral SAM 3.1 naming validation and local resolution.
//!
//! This is intentionally not an address book and never calls system DNS.
//! It canonicalizes only a complete public Destination supplied by the
//! client; the daemon may add a locally-owned cached mapping.

use super::base64;
use super::private_destination::PUB_LENGTH;
use super::{MAX_SAM_NAME_BYTES, MAX_SAM_OPTION_VALUE_BYTES};

/// A validated naming request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamingLookupRequest {
    /// The SAM name to resolve.
    pub name: String,
}

/// Typed naming failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NamingLookupError {
    /// The name is empty or exceeds the SAM bound.
    InvalidName,
    /// The name is a malformed public-destination encoding.
    InvalidKey,
    /// The name is validly shaped but not locally available.
    KeyNotFound,
    /// `OPTIONS=true` is a later SAM feature.
    UnsupportedOptions,
}

impl core::fmt::Display for NamingLookupError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidName => "invalid SAM naming name",
            Self::InvalidKey => "invalid public destination key",
            Self::KeyNotFound => "name not found in the local naming surface",
            Self::UnsupportedOptions => "NAMING LOOKUP OPTIONS is unsupported in SAM 3.1",
        })
    }
}

/// Parses `NAMING LOOKUP NAME=...` and applies the M7 options policy.
pub fn parse_naming_lookup(
    command: &crate::sam::command::Command,
) -> Result<NamingLookupRequest, NamingLookupError> {
    if command
        .value("OPTIONS")
        .is_some_and(|value| value.eq_ignore_ascii_case("true"))
    {
        return Err(NamingLookupError::UnsupportedOptions);
    }
    let name = command
        .value("NAME")
        .ok_or(NamingLookupError::InvalidName)?;
    // A human-readable name uses the Plan 136 256-byte ceiling, but a
    // complete SAM public Destination is a larger, bounded Base64 option
    // (currently 524 characters). Keep the larger bound finite before any
    // decode while allowing the required idempotent public-key lookup.
    if name.is_empty()
        || name.len() > MAX_SAM_OPTION_VALUE_BYTES
        || (name.len() > MAX_SAM_NAME_BYTES && resolve_public_destination(name).is_err())
    {
        return Err(NamingLookupError::InvalidName);
    }
    Ok(NamingLookupRequest {
        name: name.to_owned(),
    })
}

/// Canonicalizes a complete SAM public Destination.
pub fn resolve_public_destination(name: &str) -> Result<String, NamingLookupError> {
    if name.len() > MAX_SAM_OPTION_VALUE_BYTES {
        return Err(NamingLookupError::InvalidName);
    }
    if name.eq_ignore_ascii_case("ME") || name.to_ascii_lowercase().ends_with(".i2p") {
        return Err(NamingLookupError::KeyNotFound);
    }
    let bytes = base64::decode(name, PUB_LENGTH).map_err(|_| NamingLookupError::InvalidKey)?;
    let destination =
        i2pr_proto::Destination::decode(&bytes, i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
            .map_err(|_| NamingLookupError::InvalidKey)?;
    let canonical = destination
        .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
        .map_err(|_| NamingLookupError::InvalidKey)?;
    if canonical.len() != PUB_LENGTH || canonical.as_slice() != bytes.as_slice() {
        return Err(NamingLookupError::InvalidKey);
    }
    Ok(base64::encode(&canonical))
}

/// Decodes a local, non-network .b32.i2p spelling into a destination hash.
/// This helper performs no lookup; callers must consult an existing
/// validated local store.
pub fn decode_b32_destination_hash(name: &str) -> Result<[u8; 32], NamingLookupError> {
    let suffix = ".b32.i2p";
    if !name.to_ascii_lowercase().ends_with(suffix) {
        return Err(NamingLookupError::InvalidKey);
    }
    let label = &name[..name.len() - suffix.len()];
    if label.len() != 52 {
        return Err(NamingLookupError::InvalidKey);
    }
    let mut out = [0_u8; 32];
    let mut accumulator = 0_u32;
    let mut bits = 0_u8;
    let mut offset = 0_usize;
    for byte in label.bytes() {
        let value = match byte.to_ascii_lowercase() {
            b'a'..=b'z' => byte.to_ascii_lowercase() - b'a',
            b'2'..=b'7' => byte - b'2' + 26,
            _ => return Err(NamingLookupError::InvalidKey),
        };
        accumulator = (accumulator << 5) | u32::from(value);
        bits += 5;
        while bits >= 8 {
            bits -= 8;
            if offset >= out.len() {
                if accumulator & ((1_u32 << bits) - 1) != 0 {
                    return Err(NamingLookupError::InvalidKey);
                }
                return Ok(out);
            }
            out[offset] = ((accumulator >> bits) & 0xff) as u8;
            offset += 1;
        }
    }
    if offset != out.len() || (bits > 0 && accumulator & ((1_u32 << bits) - 1) != 0) {
        return Err(NamingLookupError::InvalidKey);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sam::parser::parse_line;

    #[test]
    fn unknown_i2p_names_are_not_sent_to_dns() {
        let command = parse_line("NAMING LOOKUP NAME=unknown.i2p").unwrap();
        let request = parse_naming_lookup(command.command().unwrap()).unwrap();
        assert_eq!(
            resolve_public_destination(&request.name),
            Err(NamingLookupError::KeyNotFound)
        );
    }

    #[test]
    fn options_are_explicitly_unsupported() {
        let command = parse_line("NAMING LOOKUP NAME=ME OPTIONS=true").unwrap();
        assert!(matches!(
            command,
            crate::sam::command::CommandOutcome::Unsupported(_)
        ));
    }
}
