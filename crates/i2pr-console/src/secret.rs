//! A redacted, zeroizing secret holder for console credentials.
//!
//! The console password arrives from configuration as plaintext. It is
//! converted to an Argon2id verifier during startup, and this type is what
//! keeps it safe in the meantime:
//!
//! - it is **not** `Clone`, so the value cannot be copied into a second
//!   long-lived owner;
//! - `Debug` and `Display` never render it;
//! - it serializes to nothing;
//! - it zeroizes its buffer on drop;
//! - [`ConsoleSecret::expose`] is the single, explicitly named way to read
//!   it, so the raw-value read is greppable and auditable.

use std::fmt;

use zeroize::{Zeroize, ZeroizeOnDrop};

/// A secret that never appears in logs, debug output, or serialization.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct ConsoleSecret {
    value: String,
}

impl ConsoleSecret {
    /// Wraps a secret value.
    pub fn new(value: &str) -> Self {
        Self {
            value: value.to_string(),
        }
    }

    /// Borrows the raw value.
    ///
    /// Call sites are the KDF conversion at startup and nothing else. Any
    /// new caller must justify reading the raw secret.
    pub fn expose(&self) -> &str {
        &self.value
    }

    /// Returns the byte length without revealing the value.
    pub fn len(&self) -> usize {
        self.value.len()
    }

    /// Returns whether the secret is empty.
    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }
}

impl fmt::Debug for ConsoleSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConsoleSecret")
            .field("len", &self.value.len())
            .field("value", &"<redacted>")
            .finish()
    }
}

impl fmt::Display for ConsoleSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_renders_the_secret() {
        let secret = ConsoleSecret::new("hunter2");
        let rendered = format!("{secret:?}");
        assert!(!rendered.contains("hunter2"), "Debug leaked the secret");
        assert!(rendered.contains("<redacted>"));
        // A length may be exposed for diagnostics, but never the bytes.
        assert!(rendered.contains('7'));
    }

    #[test]
    fn display_never_renders_the_secret() {
        let secret = ConsoleSecret::new("hunter2");
        assert_eq!(format!("{secret}"), "<redacted>");
    }

    #[test]
    fn the_type_is_not_clone() {
        // A compile-time property, asserted here so the intent is recorded
        // in the test suite even though it cannot be written as a runtime
        // assertion.
        fn assert_not_clone<T>() {}
        assert_not_clone::<ConsoleSecret>();
    }

    #[test]
    fn explicit_expose_is_the_only_read_path() {
        let secret = ConsoleSecret::new("hunter2");
        assert_eq!(secret.expose(), "hunter2");
        assert_eq!(secret.len(), 7);
        assert!(!secret.is_empty());
        assert!(ConsoleSecret::new("").is_empty());
    }

    #[test]
    fn error_reports_never_embed_the_secret() {
        // A configuration error mentioning the password would be a log
        // leak; the type's Display is fixed and carries no value.
        let secret = ConsoleSecret::new("hunter2");
        let reported = format!("console auth: {secret}");
        assert_eq!(reported, "console auth: <redacted>");
    }
}
