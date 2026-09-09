//! Plan 174 §5 destination reference policy.
//!
//! Structural parsing and validation only. No DNS lookup, no
//! filesystem lookup, no network lookup, and no implicit clearnet
//! fallback. Resolution of a validated reference to a LeaseSet or a
//! remote runtime remains a daemon/client composition operation.

#![forbid(unsafe_code)]

use crate::errors::ServiceTunnelError;

/// Canonical I2P Base32 suffix for destination hashes.
pub const B32_SUFFIX: &str = ".b32.i2p";
/// Canonical I2P suffix for static aliases.
pub const I2P_SUFFIX: &str = ".i2p";
/// Exact Base32 label length for a 32-byte destination hash.
pub const B32_LABEL_LEN: usize = 52;
/// Maximum static alias total length including the `.i2p` suffix.
pub const MAX_STATIC_ALIAS_LEN: usize = 67;
/// Maximum static alias label length excluding the `.i2p` suffix.
pub const MAX_STATIC_ALIAS_LABEL_LEN: usize = 63;
/// Maximum configured-destination public material length.
pub const MAX_CONFIGURED_DESTINATION_LEN: usize = 4096;

/// A validated I2P destination reference.
///
/// The three variants mirror Plan 173 §3.4:
/// canonical Base32 `.b32.i2p` hashes, bounded static aliases from
/// strict configuration, and explicitly configured full Destination
/// material carried only as public data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DestinationRef {
    /// Canonical 52-character Base32 label plus `.b32.i2p`.
    Base32Hash {
        /// Lower-case canonical label (52 characters).
        label: String,
        /// Decoded 32-byte destination hash.
        hash: [u8; 32],
    },
    /// Bounded lower-case `.i2p` static alias name.
    StaticAlias(String),
    /// Explicitly configured full Destination material as public
    /// data. Never carries private destination material.
    ConfiguredDestination(String),
}

impl DestinationRef {
    /// Parses one destination reference string structurally.
    ///
    /// Dispatch:
    /// - ends with `.b32.i2p` -> [`Self::Base32Hash`];
    /// - ends with `.i2p` -> [`Self::StaticAlias`];
    /// - otherwise -> [`Self::ConfiguredDestination`] when the value
    ///   is bounded public material and is not an IP literal.
    pub fn parse(value: &str) -> Result<Self, ServiceTunnelError> {
        if value.is_empty() {
            return Err(ServiceTunnelError::InvalidDestinationRef {
                value: String::new(),
                reason: "must not be empty",
            });
        }
        if value.len() > MAX_CONFIGURED_DESTINATION_LEN {
            return Err(ServiceTunnelError::InvalidDestinationRef {
                value: truncated(value),
                reason: "exceeds the destination reference ceiling",
            });
        }
        if contains_nul_control_or_whitespace(value) {
            return Err(ServiceTunnelError::InvalidDestinationRef {
                value: truncated(value),
                reason: "must not contain NUL, control, or whitespace",
            });
        }
        // IP literals are never valid I2P destination references,
        // even when they carry an `.i2p` suffix trick.
        if value.parse::<std::net::IpAddr>().is_ok() {
            return Err(ServiceTunnelError::InvalidDestinationRef {
                value: truncated(value),
                reason: "IP literals are rejected for I2P destination references",
            });
        }
        if let Some(label) = value.strip_suffix(B32_SUFFIX) {
            return parse_base32_hash(label, value);
        }
        // Mixed-suffix tricks such as `.i2p.example.com` do not end
        // with `.i2p` and therefore fall through to the configured
        // branch, which rejects them because they are not valid
        // alias spellings and are not valid public material either.
        // Handle them explicitly for a precise reason.
        if value.contains(".i2p.") || value.contains(".i2p:") {
            return Err(ServiceTunnelError::InvalidDestinationRef {
                value: truncated(value),
                reason: "mixed suffix tricks are rejected",
            });
        }
        if value.ends_with(I2P_SUFFIX) {
            validate_static_alias(value)?;
            return Ok(Self::StaticAlias(value.to_owned()));
        }
        parse_configured_destination(value)
    }

    /// Returns the canonical string form.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Base32Hash { label, .. } => label,
            Self::StaticAlias(alias) => alias,
            Self::ConfiguredDestination(material) => material,
        }
    }

    /// Returns the full canonical spelling including suffix for the
    /// Base32 and alias forms.
    pub fn canonical_string(&self) -> String {
        match self {
            Self::Base32Hash { label, .. } => format!("{label}{B32_SUFFIX}"),
            Self::StaticAlias(alias) => alias.clone(),
            Self::ConfiguredDestination(material) => material.clone(),
        }
    }
}

/// Validates a bounded lower-case `.i2p` static alias name.
pub fn validate_static_alias(value: &str) -> Result<(), ServiceTunnelError> {
    let rejected = |reason: &'static str| ServiceTunnelError::InvalidAlias {
        value: truncated(value),
        reason,
    };
    if value.is_empty() {
        return Err(rejected("must not be empty"));
    }
    if value.len() > MAX_STATIC_ALIAS_LEN {
        return Err(rejected("exceeds the static alias ceiling"));
    }
    if !value.ends_with(I2P_SUFFIX) {
        return Err(rejected("must end with .i2p"));
    }
    // `.b32.i2p` spellings are Base32 references, not aliases.
    if value.ends_with(B32_SUFFIX) {
        return Err(rejected("b32 references are not static aliases"));
    }
    let label = &value[..value.len() - I2P_SUFFIX.len()];
    if label.is_empty() {
        return Err(rejected("alias label must not be empty"));
    }
    if label.len() > MAX_STATIC_ALIAS_LABEL_LEN {
        return Err(rejected("alias label exceeds 63 bytes"));
    }
    if label.starts_with('-') || label.ends_with('-') {
        return Err(rejected("alias label must not start or end with hyphen"));
    }
    if label.contains("..") {
        return Err(rejected("alias label must not contain empty labels"));
    }
    if label.parse::<std::net::IpAddr>().is_ok() {
        return Err(rejected("IP literals are rejected for aliases"));
    }
    for part in label.split('.') {
        if part.is_empty() {
            return Err(rejected("alias label must not contain empty labels"));
        }
        if part.len() > MAX_STATIC_ALIAS_LABEL_LEN {
            return Err(rejected("alias sub-label exceeds 63 bytes"));
        }
        for byte in part.bytes() {
            let ok = byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-';
            if !ok {
                return Err(rejected(
                    "alias labels must be lower-case alphanumeric or hyphen",
                ));
            }
        }
        if part.starts_with('-') || part.ends_with('-') {
            return Err(rejected(
                "alias sub-label must not start or end with hyphen",
            ));
        }
    }
    Ok(())
}

fn parse_base32_hash(label: &str, full: &str) -> Result<DestinationRef, ServiceTunnelError> {
    let rejected = |reason: &'static str| ServiceTunnelError::InvalidDestinationRef {
        value: truncated(full),
        reason,
    };
    if label.len() != B32_LABEL_LEN {
        return Err(rejected("Base32 label must be exactly 52 characters"));
    }
    for byte in label.bytes() {
        // Canonical form is lower-case `a-z2-7` only. Upper-case,
        // `0`/`1`/`8`/`9`, hyphen, and other alphabet bytes are
        // rejected here.
        if !matches!(byte, b'a'..=b'z' | b'2'..=b'7') {
            return Err(rejected(
                "Base32 label must use the canonical a-z2-7 alphabet",
            ));
        }
    }
    let hash = decode_base32_label(label).ok_or_else(|| rejected("Base32 label is malformed"))?;
    Ok(DestinationRef::Base32Hash {
        label: label.to_owned(),
        hash,
    })
}

fn parse_configured_destination(value: &str) -> Result<DestinationRef, ServiceTunnelError> {
    let rejected = |reason: &'static str| ServiceTunnelError::InvalidDestinationRef {
        value: truncated(value),
        reason,
    };
    // Mixed-suffix and clearnet forms never reach here as valid
    // configured material.
    if value.contains("://") || value.contains('@') || value.contains('/') || value.contains(':') {
        // Allow `config:`-prefixed explicit material below; bare
        // URIs, user-info, paths, and ports are rejected.
        if !value.starts_with("config:") {
            return Err(rejected(
                "configured destinations must not carry URIs or ports",
            ));
        }
    }
    if value.parse::<std::net::IpAddr>().is_ok() {
        return Err(rejected(
            "IP literals are rejected for I2P destination references",
        ));
    }
    if value.to_ascii_lowercase().contains("priv") {
        return Err(rejected(
            "configured destinations must not carry private destination material",
        ));
    }
    if value.len() > MAX_CONFIGURED_DESTINATION_LEN {
        return Err(rejected("exceeds the destination reference ceiling"));
    }
    // Configured material must be printable ASCII without control.
    if !value.bytes().all(|b| (0x21..=0x7e).contains(&b)) {
        return Err(rejected("configured destinations must be printable ASCII"));
    }
    Ok(DestinationRef::ConfiguredDestination(value.to_owned()))
}

fn decode_base32_label(label: &str) -> Option<[u8; 32]> {
    let mut out = [0_u8; 32];
    let mut accumulator: u32 = 0;
    let mut bits: u8 = 0;
    let mut offset: usize = 0;
    for byte in label.bytes() {
        let value = match byte {
            b'a'..=b'z' => byte - b'a',
            b'2'..=b'7' => byte - b'2' + 26,
            _ => return None,
        };
        accumulator = (accumulator << 5) | u32::from(value);
        bits += 5;
        while bits >= 8 {
            bits -= 8;
            if offset >= out.len() {
                if accumulator & ((1_u32 << bits) - 1) != 0 {
                    return None;
                }
                return Some(out);
            }
            out[offset] = ((accumulator >> bits) & 0xff) as u8;
            offset += 1;
        }
    }
    if offset != out.len() {
        return None;
    }
    if bits > 0 && accumulator & ((1_u32 << bits) - 1) != 0 {
        return None;
    }
    Some(out)
}

fn contains_nul_control_or_whitespace(value: &str) -> bool {
    value.bytes().any(|b| b == 0 || b < 0x21 || b == 0x7f)
}

fn truncated(value: &str) -> String {
    const MAX: usize = 128;
    if value.len() <= MAX {
        value.to_owned()
    } else {
        format!("{}…", &value[..MAX])
    }
}

/// Bounded static alias table with duplicate and conflict rejection.
///
/// The table maps validated `.i2p` aliases to validated destination
/// references. It performs no lookup; resolution remains a
/// daemon/client composition operation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StaticAliasTable {
    entries: Vec<(String, DestinationRef)>,
}

impl StaticAliasTable {
    /// Creates an empty alias table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts one alias mapping, rejecting duplicates, conflicts,
    /// and malformed inputs.
    pub fn insert(
        &mut self,
        alias: &str,
        target: DestinationRef,
    ) -> Result<(), ServiceTunnelError> {
        validate_static_alias(alias)?;
        if self.entries.len() >= crate::config::MAX_STATIC_ALIASES {
            return Err(ServiceTunnelError::ExceedsCeiling {
                field: "alias_count",
                reason: "exceeds the static alias ceiling",
            });
        }
        if self.entries.iter().any(|(existing, _)| existing == alias) {
            return Err(ServiceTunnelError::DuplicateAlias {
                value: alias.to_owned(),
            });
        }
        // An alias target that is itself an alias must be a valid
        // alias spelling; malformed alias targets are rejected here
        // so configuration cannot smuggle an invalid reference
        // through the table.
        if let DestinationRef::StaticAlias(inner) = &target {
            validate_static_alias(inner).map_err(|_| ServiceTunnelError::InvalidAlias {
                value: truncated(inner),
                reason: "alias target is malformed",
            })?;
        }
        self.entries.push((alias.to_owned(), target));
        Ok(())
    }

    /// Returns the destination reference for one alias, if present.
    pub fn get(&self, alias: &str) -> Option<&DestinationRef> {
        self.entries
            .iter()
            .find(|(existing, _)| existing == alias)
            .map(|(_, target)| target)
    }

    /// Returns the number of configured aliases.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` when no aliases are configured.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterates over alias mappings in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &DestinationRef)> {
        self.entries
            .iter()
            .map(|(alias, target)| (alias.as_str(), target))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canonical_b32() -> String {
        format!("{}{B32_SUFFIX}", "a".repeat(B32_LABEL_LEN))
    }

    #[test]
    fn canonical_b32_parses() {
        let value = canonical_b32();
        let parsed = DestinationRef::parse(&value).expect("canonical b32 must parse");
        assert!(matches!(parsed, DestinationRef::Base32Hash { .. }));
        assert_eq!(parsed.canonical_string(), value);
    }

    #[test]
    fn b32_wrong_length_rejected() {
        let short = format!("{}{B32_SUFFIX}", "a".repeat(51));
        assert!(DestinationRef::parse(&short).is_err());
        let long = format!("{}{B32_SUFFIX}", "a".repeat(53));
        assert!(DestinationRef::parse(&long).is_err());
    }

    #[test]
    fn b32_wrong_alphabet_rejected() {
        // `0`, `1`, `8`, `9`, upper-case, and hyphen are outside
        // the canonical `a-z2-7` alphabet.
        for bad in ["0", "1", "8", "9", "A", "-", "~"] {
            let label: String = std::iter::repeat_n(bad, B32_LABEL_LEN).collect::<String>();
            let value = format!("{label}{B32_SUFFIX}");
            assert!(
                DestinationRef::parse(&value).is_err(),
                "alphabet byte {bad:?} must be rejected"
            );
        }
    }

    #[test]
    fn mixed_suffix_tricks_rejected() {
        assert!(DestinationRef::parse("example.i2p.example.com").is_err());
        assert!(DestinationRef::parse("example.i2p:8080").is_err());
        assert!(DestinationRef::parse("a.i2p./evil").is_err());
    }

    #[test]
    fn nul_control_whitespace_rejected() {
        assert!(DestinationRef::parse("exam\0ple.i2p").is_err());
        assert!(DestinationRef::parse("exam ple.i2p").is_err());
        assert!(DestinationRef::parse("exam\nple.i2p").is_err());
        assert!(DestinationRef::parse("exam\tple.i2p").is_err());
    }

    #[test]
    fn ip_literals_rejected() {
        assert!(DestinationRef::parse("127.0.0.1").is_err());
        assert!(DestinationRef::parse("::1").is_err());
        assert!(DestinationRef::parse("1.2.3.4").is_err());
    }

    #[test]
    fn overlong_alias_rejected() {
        let label = "a".repeat(MAX_STATIC_ALIAS_LABEL_LEN + 1);
        let value = format!("{label}.i2p");
        assert!(validate_static_alias(&value).is_err());
        assert!(DestinationRef::parse(&value).is_err());
    }

    #[test]
    fn duplicate_aliases_rejected() {
        let mut table = StaticAliasTable::new();
        let target = DestinationRef::parse(&canonical_b32()).expect("b32");
        table.insert("example.i2p", target.clone()).expect("first");
        let err = table.insert("example.i2p", target).expect_err("duplicate");
        assert!(matches!(err, ServiceTunnelError::DuplicateAlias { .. }));
    }

    #[test]
    fn alias_pointing_to_malformed_reference_rejected() {
        let mut table = StaticAliasTable::new();
        let malformed = DestinationRef::StaticAlias("BAD_UPPER.i2p".to_owned());
        assert!(table.insert("good.i2p", malformed).is_err());
    }

    #[test]
    fn alias_table_ceiling_enforced() {
        let mut table = StaticAliasTable::new();
        let target = DestinationRef::parse(&canonical_b32()).expect("b32");
        for index in 0..crate::config::MAX_STATIC_ALIASES {
            let alias = format!("host{index}.i2p");
            table
                .insert(&alias, target.clone())
                .expect("within ceiling");
        }
        let err = table
            .insert("overflow.i2p", target)
            .expect_err("ceiling must hold");
        assert!(matches!(err, ServiceTunnelError::ExceedsCeiling { .. }));
    }
}
