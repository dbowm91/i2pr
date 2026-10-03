//! Canonical `.i2p` hostname validation.
//!
//! Policy: ASCII lowercase, one optional trailing dot (stripped), `.i2p`
//! suffix required, labels of `a-z 0-9 -` with no empty label and no
//! leading/trailing hyphen, total length within [`MAX_HOSTNAME_LEN`].
//! Non-`.i2p` names (operator aliases, `localhost`, IP literals) are not
//! address-book names; they resolve through the static alias table or
//! fail before reaching this owner.

use crate::error::AddressBookError;

/// Maximum hostname length in bytes (mirrors the wire ceiling).
pub const MAX_HOSTNAME_LEN: usize = 255;
/// Maximum length of one hostname label in bytes.
pub const MAX_LABEL_LEN: usize = 63;

/// A validated canonical `.i2p` hostname.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Hostname(String);

impl Hostname {
    /// Canonical text (lowercase, no trailing dot).
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for Hostname {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Canonicalizes and validates `raw` into a [`Hostname`].
pub fn parse_hostname(raw: &str) -> Result<Hostname, AddressBookError> {
    if raw.is_empty() || raw.len() > MAX_HOSTNAME_LEN {
        return Err(AddressBookError::InvalidHostname);
    }
    if !raw.is_ascii() {
        return Err(AddressBookError::InvalidHostname);
    }
    let lower = raw.to_ascii_lowercase();
    let bare = lower.strip_suffix('.').unwrap_or(&lower);
    if bare.is_empty() || bare.len() > MAX_HOSTNAME_LEN {
        return Err(AddressBookError::InvalidHostname);
    }
    let Some(i2p) = bare.strip_suffix(".i2p") else {
        return Err(AddressBookError::InvalidHostname);
    };
    if i2p.is_empty() {
        return Err(AddressBookError::InvalidHostname);
    }
    for label in i2p.split('.') {
        if label.is_empty()
            || label.len() > MAX_LABEL_LEN
            || label.starts_with('-')
            || label.ends_with('-')
            || !label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(AddressBookError::InvalidHostname);
        }
    }
    Ok(Hostname(format!("{i2p}.i2p")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_names_pass() {
        for raw in [
            "example.i2p",
            "Example.I2P",
            "example.i2p.",
            "a.b.c.i2p",
            "x1-y2.i2p",
            "0.i2p",
        ] {
            let parsed = parse_hostname(raw).expect("valid hostname");
            assert!(parsed.as_str().ends_with(".i2p"));
            assert!(!parsed.as_str().ends_with('.'));
            assert_eq!(parsed.as_str(), &parsed.as_str().to_ascii_lowercase());
        }
        assert_eq!(
            parse_hostname("Example.I2P").expect("case").as_str(),
            "example.i2p"
        );
    }

    #[test]
    fn non_names_fail() {
        for raw in [
            "",
            "i2p",
            ".i2p",
            "example.com",
            "localhost",
            "127.0.0.1",
            "foo.i2p bar.i2p",
            "-lead.i2p",
            "trail-.i2p",
            "emp..ty.i2p",
            "snowman☃.i2p",
            "UPPER.i2P.",
        ] {
            if raw == "UPPER.i2P." {
                // Trailing-dot canonicalization still requires a valid body.
                continue;
            }
            assert!(parse_hostname(raw).is_err(), "must reject {raw:?}");
        }
        assert_eq!(
            parse_hostname("UPPER.i2P.").expect("trailing dot").as_str(),
            "upper.i2p"
        );
    }

    #[test]
    fn length_bounds_hold() {
        let label63 = "a".repeat(63);
        let ok = format!("{label63}.i2p");
        assert!(parse_hostname(&ok).is_ok());
        let label64 = "a".repeat(64);
        assert!(parse_hostname(&format!("{label64}.i2p")).is_err());
        let big = format!("{}.i2p", "a".repeat(252));
        assert!(big.len() > MAX_HOSTNAME_LEN);
        assert!(parse_hostname(&big).is_err());
    }
}
