//! Typed address-book errors (no `anyhow`).
//!
//! Every variant carries no caller-supplied material: hostnames,
//! destinations, URLs, and config values never enter an error. Control
//! surfaces may render these errors verbatim without a redaction pass.

use thiserror::Error;

/// Errors emitted by the canonical address-book owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
pub enum AddressBookError {
    /// A hostname failed canonicalization or validation.
    #[error("invalid hostname")]
    InvalidHostname,
    /// A destination text failed structural validation.
    #[error("invalid destination")]
    InvalidDestination,
    /// An entry operation targeted a hostname absent from the book.
    #[error("unknown hostname")]
    UnknownHostname,
    /// A book already holds the maximum entry count.
    #[error("book is full")]
    BookFull,
    /// A request mixed incompatible operation shapes.
    #[error("incompatible operation shapes in one request")]
    MixedShapes,
    /// A required field is missing or has the wrong JSON shape.
    #[error("malformed field")]
    MalformedField,
    /// A subscription URL failed validation.
    #[error("invalid subscription URL")]
    InvalidSubscription,
    /// The subscription set exceeds its bound.
    #[error("too many subscriptions")]
    TooManySubscriptions,
    /// A subscription body exceeds its bound.
    #[error("subscription body over bound")]
    BodyOverBound,
    /// A subscription body holds too many entries.
    #[error("subscription list over bound")]
    ListOverBound,
    /// A config key is unknown.
    #[error("unknown config key")]
    UnknownConfigKey,
    /// A config value failed its typed parser.
    #[error("invalid config value")]
    InvalidConfigValue,
    /// A config path escapes the administrative root.
    #[error("config path escapes its root")]
    PathEscape,
    /// A generation failed version or structural validation.
    #[error("invalid generation")]
    InvalidGeneration,
}
