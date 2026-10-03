//! Plan 292 server inbound peer allow/deny policy (runtime-neutral).
//!
//! `access_list` and `white_list` unite into the allow set;
//! `black_list` is the deny set. Entries are canonical Base32
//! destination hashes (52 characters, with or without the
//! `.b32.i2p` suffix): the control boundary cannot resolve `.i2p`
//! names or full destinations synchronously, so anything else is
//! rejected with a static reason instead of being resolved late or
//! guessed. Matching is exact on the 32-byte peer hash observed at
//! inbound accept: denied hashes always lose, an empty allow set
//! admits everyone not denied, and a non-empty allow set admits
//! only its members.

#![forbid(unsafe_code)]

use crate::destination::B32_SUFFIX;
use crate::errors::ServiceTunnelError;

/// Maximum entries in one allow/deny list.
pub const MAX_ACCESS_LIST_ENTRIES: usize = 64;

/// Inbound peer destination-hash policy for server tunnels.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ServerAccessPolicy {
    /// Allowed peer hashes (`access_list` union `white_list`).
    pub allow: Vec<[u8; 32]>,
    /// Denied peer hashes (`black_list`).
    pub deny: Vec<[u8; 32]>,
}

impl ServerAccessPolicy {
    /// Whether both lists are empty (admits everyone).
    pub fn is_empty(&self) -> bool {
        self.allow.is_empty() && self.deny.is_empty()
    }

    /// Decides one inbound peer: denied hashes always lose; an
    /// empty allow set admits everyone else; otherwise only members
    /// are admitted.
    pub fn allows(&self, peer: &[u8; 32]) -> bool {
        if self.deny.contains(peer) {
            return false;
        }
        if self.allow.is_empty() {
            return true;
        }
        self.allow.contains(peer)
    }

    /// Parses allow entries (`access_list` plus `white_list` values)
    /// and deny entries (`black_list` values) into a policy. Empty
    /// items are skipped; every other item must be a canonical
    /// Base32 destination hash.
    pub fn parse(allow_values: &[&str], deny_values: &[&str]) -> Result<Self, ServiceTunnelError> {
        let mut allow = Vec::new();
        for value in allow_values {
            parse_entries(value, &mut allow)?;
        }
        let mut deny = Vec::new();
        for value in deny_values {
            parse_entries(value, &mut deny)?;
        }
        Ok(Self { allow, deny })
    }
}

/// Splits one option value on commas and whitespace and parses every
/// non-empty item as a destination hash.
fn parse_entries(value: &str, out: &mut Vec<[u8; 32]>) -> Result<(), ServiceTunnelError> {
    for item in value.split([',', ' ', '\t', '\r', '\n']) {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        if out.len() >= MAX_ACCESS_LIST_ENTRIES {
            return Err(ServiceTunnelError::ExceedsCeiling {
                field: "access_list",
                reason: "access lists exceed the per-tunnel entry ceiling",
            });
        }
        out.push(parse_hash_entry(item)?);
    }
    Ok(())
}

/// Parses one entry as a canonical Base32 destination hash, with or
/// without the `.b32.i2p` suffix. Never echoes the entry: values
/// stay out of errors, logs, and control output.
fn parse_hash_entry(item: &str) -> Result<[u8; 32], ServiceTunnelError> {
    let label = item.strip_suffix(B32_SUFFIX).unwrap_or(item);
    crate::destination::decode_base32_label(label).ok_or(ServiceTunnelError::InvalidTarget {
        value: truncated_entry(),
        reason: "access entries must be canonical base32 destination hashes",
    })
}

/// Fixed placeholder so entry values never reach errors.
fn truncated_entry() -> String {
    String::from("<redacted-entry>")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Canonical 52-char label with a distinct payload char
    /// (the final char stays `a` so the trailing padding bits are
    /// zero, as canonical 32-byte encodings require).
    fn b32(byte: u8) -> String {
        format!(
            "{}{}a.b32.i2p",
            "a".repeat(50),
            (b'b' + (byte % 20)) as char
        )
    }

    #[test]
    fn allow_deny_semantics() {
        let policy = ServerAccessPolicy::parse(&[&b32(0)], &[&b32(1)]).expect("parse");
        let allowed = crate::destination::decode_base32_label(
            b32(0).strip_suffix(B32_SUFFIX).expect("suffix"),
        )
        .expect("hash");
        let denied = crate::destination::decode_base32_label(
            b32(1).strip_suffix(B32_SUFFIX).expect("suffix"),
        )
        .expect("hash");
        assert!(policy.allows(&allowed));
        assert!(!policy.allows(&denied));
        assert!(!policy.allows(&[9_u8; 32]));
        let open = ServerAccessPolicy::parse(&[], &[]).expect("parse");
        assert!(open.is_empty());
        assert!(open.allows(&[9_u8; 32]));
        // Deny wins over allow for the same hash.
        let conflict = ServerAccessPolicy::parse(&[&b32(0)], &[&b32(0)]).expect("parse");
        assert!(!conflict.allows(&allowed));
    }

    #[test]
    fn separators_and_suffix_forms() {
        let suffixed = b32(2);
        let bare = suffixed
            .strip_suffix(B32_SUFFIX)
            .expect("suffix")
            .to_owned();
        let policy = ServerAccessPolicy::parse(&[&format!("{suffixed}, {bare}\n {bare}")], &[])
            .expect("parse");
        assert_eq!(policy.allow.len(), 3);
    }

    #[test]
    fn non_hash_entries_rejected_without_echo() {
        let bad_label = "a".repeat(52).replace('a', "8");
        for bad in [
            "example.i2p",
            "127.0.0.1",
            "AAAA",
            bad_label.as_str(),
            "not a hash at all, with spaces and commas,,",
        ] {
            let error = ServerAccessPolicy::parse(&[bad], &[]).expect_err("must fail");
            let text = format!("{error}");
            assert!(!text.contains("example.i2p"), "value leaked: {text}");
            assert!(!text.contains("127.0.0.1"), "value leaked: {text}");
        }
    }

    #[test]
    fn entry_ceiling_enforced() {
        let many = (0..=MAX_ACCESS_LIST_ENTRIES)
            .map(|_| b32(3))
            .collect::<Vec<_>>()
            .join(",");
        assert!(ServerAccessPolicy::parse(&[&many], &[]).is_err());
    }
}
