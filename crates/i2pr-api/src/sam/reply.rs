//! Typed SAM v3.1 reply model.
//!
//! The reply model centralises encoding so the runtime never
//! hand-formats arbitrary strings. Plan 136 supports the minimum
//! vocabulary required by the Milestone 7 baseline; later plans extend
//! it as additional commands are implemented.

use core::fmt;

use crate::sam::{base64, private_destination::SamPrivateDestination, version::SamVersion};

use super::{MAX_SAM_OPTION_VALUE_BYTES, MAX_SAM_PRIV_TEXT_BYTES, MAX_SAM_PUB_TEXT_BYTES};

/// Result vocabulary for SAM replies.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplyResult {
    /// `RESULT=OK`.
    Ok,
    /// `RESULT=NOVERSION`.
    NoVersion,
    /// `RESULT=INVALID_KEY`.
    InvalidKey,
    /// `RESULT=KEY_NOT_FOUND`.
    KeyNotFound,
    /// `RESULT=DUPLICATED_ID`.
    DuplicatedId,
    /// `RESULT=DUPLICATED_DESTINATION`.
    DuplicatedDestination,
    /// `RESULT=INVALID_ID`.
    InvalidId,
    /// `RESULT=INVALID_NAME`.
    InvalidName,
    /// `RESULT=TIMEOUT`.
    Timeout,
    /// `RESULT=CANT_REACH_PEER`.
    CantReachPeer,
    /// `RESULT=CONNECTION_REFUSED`.
    ConnectionRefused,
    /// `RESULT=NOT_IMPLEMENTED`.
    NotImplemented,
    /// `RESULT=I2P_ERROR`.
    I2pError,
}

impl ReplyResult {
    /// Returns the canonical wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::NoVersion => "NOVERSION",
            Self::InvalidKey => "INVALID_KEY",
            Self::KeyNotFound => "KEY_NOT_FOUND",
            Self::DuplicatedId => "DUPLICATED_ID",
            Self::DuplicatedDestination => "DUPLICATED_DESTINATION",
            Self::InvalidId => "INVALID_ID",
            Self::InvalidName => "INVALID_NAME",
            Self::Timeout => "TIMEOUT",
            Self::CantReachPeer => "CANT_REACH_PEER",
            Self::ConnectionRefused => "CONNECTION_REFUSED",
            Self::NotImplemented => "NOT_IMPLEMENTED",
            Self::I2pError => "I2P_ERROR",
        }
    }
}

impl fmt::Display for ReplyResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A typed SAM reply line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplyLine {
    kind: ReplyKind,
    result: ReplyResult,
    options: Vec<(String, String)>,
}

impl ReplyLine {
    /// Constructs a reply line with no options.
    pub fn new(kind: ReplyKind, result: ReplyResult) -> Self {
        Self {
            kind,
            result,
            options: Vec::new(),
        }
    }

    /// Adds an option pair to the reply.
    pub fn with_option(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.options.push((key.into(), value.into()));
        self
    }

    /// Returns the reply kind.
    pub const fn kind(&self) -> ReplyKind {
        self.kind
    }

    /// Returns the result.
    pub const fn result(&self) -> ReplyResult {
        self.result
    }

    /// Returns the reply options in canonical insertion order.
    pub fn options(&self) -> &[(String, String)] {
        &self.options
    }

    /// Encodes the reply line as `KIND RESULT=... [KEY=VALUE]*\n`.
    pub fn encode(&self) -> String {
        let mut out = String::new();
        out.push_str(self.kind.as_str());
        out.push(' ');
        out.push_str("RESULT=");
        out.push_str(self.result.as_str());
        for (key, value) in &self.options {
            out.push(' ');
            out.push_str(key);
            out.push('=');
            push_encoded_value(&mut out, value);
        }
        out.push('\n');
        out
    }
}

/// The reply-line prefix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplyKind {
    /// `HELLO REPLY`.
    HelloReply,
    /// `DEST REPLY`.
    DestReply,
    /// `SESSION STATUS`.
    SessionStatus,
    /// `STREAM STATUS`.
    StreamStatus,
    /// `NAMING REPLY`.
    NamingReply,
    /// `PONG`.
    Pong,
}

impl ReplyKind {
    /// Returns the canonical wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HelloReply => "HELLO REPLY",
            Self::DestReply => "DEST REPLY",
            Self::SessionStatus => "SESSION STATUS",
            Self::StreamStatus => "STREAM STATUS",
            Self::NamingReply => "NAMING REPLY",
            Self::Pong => "PONG",
        }
    }
}

fn push_encoded_value(out: &mut String, value: &str) {
    // Per the SAM 3.1 specification, values containing whitespace,
    // a backslash, or a double quote must be enclosed in double
    // quotes; everything else (including the I2P Base64 alphabet
    // and its `=` padding) is emitted unquoted because SAM parsers
    // split tokens on whitespace, not on `=`. The previous
    // implementation quoted any value containing `=`, which made the
    // `DESTINATION=<priv>` and `DESTINATION=<pub>` fields
    // unparseable by libsam3, i2psam, and any other SAM bridge
    // that consumes the value without stripping surrounding quotes.
    let needs_quoting = value
        .bytes()
        .any(|byte| byte.is_ascii_whitespace() || byte == b'"' || byte == b'\\');
    if !needs_quoting {
        out.push_str(value);
        return;
    }
    out.push('"');
    for byte in value.bytes() {
        if byte == b'"' || byte == b'\\' {
            out.push('\\');
        }
        out.push(byte as char);
    }
    out.push('"');
}

/// A high-level `Reply` enum carrying the typed payload for each
/// reply kind.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Reply {
    /// `HELLO REPLY`.
    Hello(HelloReply),
    /// `DEST REPLY`.
    Dest(DestReply),
    /// `SESSION STATUS`.
    Session(SessionStatus),
    /// `STREAM STATUS`.
    Stream(StreamStatus),
    /// `NAMING REPLY`.
    Naming(NamingReply),
    /// `PONG`.
    Pong(PongReply),
}

impl Reply {
    /// Encodes the reply as a wire line.
    pub fn encode(&self) -> String {
        match self {
            Self::Hello(reply) => reply.encode(),
            Self::Dest(reply) => reply.encode(),
            Self::Session(reply) => reply.encode(),
            Self::Stream(reply) => reply.encode(),
            Self::Naming(reply) => reply.encode(),
            Self::Pong(reply) => reply.encode(),
        }
    }
}

/// `HELLO REPLY RESULT=... [VERSION=...] [MESSAGE=...]`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HelloReply {
    result: ReplyResult,
    version: Option<SamVersion>,
    message: Option<String>,
}

impl HelloReply {
    /// Constructs a successful HELLO REPLY with the negotiated version.
    pub fn ok(version: SamVersion) -> Self {
        Self {
            result: ReplyResult::Ok,
            version: Some(version),
            message: None,
        }
    }

    /// Constructs a NOVERSION REPLY carrying an optional diagnostic
    /// message.
    pub fn no_version(message: Option<String>) -> Self {
        Self {
            result: ReplyResult::NoVersion,
            version: None,
            message,
        }
    }

    /// Returns the reply result.
    pub const fn result(&self) -> ReplyResult {
        self.result
    }

    /// Returns the negotiated version, if any.
    pub const fn version(&self) -> Option<SamVersion> {
        self.version
    }

    /// Returns the diagnostic message, if any.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// Encodes the reply as a wire line.
    pub fn encode(&self) -> String {
        let mut line = ReplyLine::new(ReplyKind::HelloReply, self.result);
        if let Some(version) = self.version {
            line = line.with_option("VERSION", version.to_string());
        }
        if let Some(message) = &self.message {
            line = line.with_option("MESSAGE", message.clone());
        }
        line.encode()
    }
}

/// `DEST REPLY RESULT=... [PUB=...] [PRIV=...] [MESSAGE=...]`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DestReply {
    result: ReplyResult,
    public_destination: Option<String>,
    private_destination: Option<String>,
    message: Option<String>,
}

impl DestReply {
    /// Constructs a successful DEST REPLY carrying the public and
    /// private destination representations.
    pub fn ok(destination: &SamPrivateDestination) -> Self {
        let public_destination = Some(base64::encode(destination.public_bytes()));
        let private_destination = Some(base64::encode(destination.private_bytes()));
        Self {
            result: ReplyResult::Ok,
            public_destination,
            private_destination,
            message: None,
        }
    }

    /// Constructs a public-only DEST REPLY carrying only the SAM
    /// public destination text. Used by the daemon's `DEST GENERATE`
    /// handler so the secret-bearing [`SamPrivateDestination`] can
    /// be zeroized before the reply line is sent.
    pub fn ok_public(public_destination: String) -> Self {
        Self {
            result: ReplyResult::Ok,
            public_destination: Some(public_destination),
            private_destination: None,
            message: None,
        }
    }

    /// Constructs a successful DEST REPLY carrying both the SAM
    /// public and private destination Base64 texts. Used by the
    /// daemon's `DEST GENERATE` handler after extracting the texts
    /// from the secret-bearing [`SamPrivateDestination`] and
    /// zeroizing the wrapper.
    pub fn ok_pub_priv(public_destination: String, private_destination: String) -> Self {
        Self {
            result: ReplyResult::Ok,
            public_destination: Some(public_destination),
            private_destination: Some(private_destination),
            message: None,
        }
    }

    /// Constructs an error DEST REPLY carrying an optional message.
    pub fn error(result: ReplyResult, message: Option<String>) -> Self {
        Self {
            result,
            public_destination: None,
            private_destination: None,
            message,
        }
    }

    /// Returns the public destination Base64 text, if any.
    pub fn public_destination(&self) -> Option<&str> {
        self.public_destination.as_deref()
    }

    /// Returns the private destination Base64 text, if any.
    pub fn private_destination(&self) -> Option<&str> {
        self.private_destination.as_deref()
    }

    /// Returns the diagnostic message, if any.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// Encodes the reply as a wire line.
    pub fn encode(&self) -> String {
        let mut line = ReplyLine::new(ReplyKind::DestReply, self.result);
        if let Some(public) = &self.public_destination {
            assert!(
                public.len() <= MAX_SAM_PUB_TEXT_BYTES,
                "public destination text exceeds the SAM byte ceiling",
            );
            line = line.with_option("PUB", public.clone());
        }
        if let Some(private) = &self.private_destination {
            assert!(
                private.len() <= MAX_SAM_PRIV_TEXT_BYTES,
                "private destination text exceeds the SAM byte ceiling",
            );
            line = line.with_option("PRIV", private.clone());
        }
        if let Some(message) = &self.message {
            line = line.with_option("MESSAGE", message.clone());
        }
        line.encode()
    }
}

/// `SESSION STATUS RESULT=... [DESTINATION=...] [MESSAGE=...]`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionStatus {
    result: ReplyResult,
    destination: Option<String>,
    message: Option<String>,
}

impl SessionStatus {
    /// Constructs a successful SESSION STATUS reply carrying the
    /// private destination.
    ///
    /// Per the SAM 3.1 specification (and what every Java I2P /
    /// i2pd / libsam3 / i2psam SAM bridge expects), the
    /// `DESTINATION=` field of `SESSION STATUS RESULT=OK` carries
    /// the **private** destination so the client can persist it
    /// for reconnect. Callers obtain the matching public key via
    /// `NAMING LOOKUP NAME=ME` after the session is created.
    ///
    /// Plan 149 originally emitted the public destination here so
    /// the black-box self-composed test could pass it straight to
    /// `STREAM CONNECT` without an extra round-trip; that
    /// convenience was a deviation from the spec and was rejected
    /// by the Plan 150 external-client matrix (libsam3 and i2psam
    /// both expect the private key in this field).
    pub fn ok(private_destination: &str) -> Self {
        assert!(
            private_destination.len() <= MAX_SAM_PRIV_TEXT_BYTES,
            "private destination text exceeds the SAM byte ceiling",
        );
        Self {
            result: ReplyResult::Ok,
            destination: Some(private_destination.to_owned()),
            message: None,
        }
    }

    /// Constructs an error SESSION STATUS reply carrying an optional
    /// diagnostic message.
    pub fn error(result: ReplyResult, message: Option<String>) -> Self {
        Self {
            result,
            destination: None,
            message,
        }
    }

    /// Returns the result code.
    pub const fn result(&self) -> ReplyResult {
        self.result
    }

    /// Returns the public destination text, if any.
    pub fn destination(&self) -> Option<&str> {
        self.destination.as_deref()
    }

    /// Returns the diagnostic message, if any.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// Encodes the reply as a wire line.
    pub fn encode(&self) -> String {
        let mut line = ReplyLine::new(ReplyKind::SessionStatus, self.result);
        if let Some(destination) = &self.destination {
            line = line.with_option("DESTINATION", destination.clone());
        }
        if let Some(message) = &self.message {
            line = line.with_option("MESSAGE", message.clone());
        }
        line.encode()
    }
}

/// `STREAM STATUS RESULT=... [MESSAGE=...]`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamStatus {
    result: ReplyResult,
    message: Option<String>,
}

impl StreamStatus {
    /// Constructs a successful STREAM STATUS reply.
    pub fn ok() -> Self {
        Self {
            result: ReplyResult::Ok,
            message: None,
        }
    }

    /// Constructs an error STREAM STATUS reply.
    pub fn error(result: ReplyResult, message: Option<String>) -> Self {
        Self { result, message }
    }

    /// Returns the result code.
    pub const fn result(&self) -> ReplyResult {
        self.result
    }

    /// Returns the diagnostic message, if any.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// Encodes the reply as a wire line.
    pub fn encode(&self) -> String {
        let mut line = ReplyLine::new(ReplyKind::StreamStatus, self.result);
        if let Some(message) = &self.message {
            line = line.with_option("MESSAGE", message.clone());
        }
        line.encode()
    }
}

/// `NAMING REPLY RESULT=... [VALUE=...] [MESSAGE=...]`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamingReply {
    result: ReplyResult,
    value: Option<String>,
    message: Option<String>,
}

impl NamingReply {
    /// Constructs a successful NAMING REPLY carrying the resolved
    /// destination value.
    pub fn ok(value: String) -> Self {
        assert!(
            value.len() <= MAX_SAM_OPTION_VALUE_BYTES,
            "NAMING REPLY value exceeds the SAM byte ceiling",
        );
        Self {
            result: ReplyResult::Ok,
            value: Some(value),
            message: None,
        }
    }

    /// Constructs an error NAMING REPLY.
    pub fn error(result: ReplyResult, message: Option<String>) -> Self {
        Self {
            result,
            value: None,
            message,
        }
    }

    /// Returns the resolved destination value, if any.
    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }

    /// Returns the diagnostic message, if any.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// Encodes the reply as a wire line.
    pub fn encode(&self) -> String {
        let mut line = ReplyLine::new(ReplyKind::NamingReply, self.result);
        if let Some(value) = &self.value {
            line = line.with_option("VALUE", value.clone());
        }
        if let Some(message) = &self.message {
            line = line.with_option("MESSAGE", message.clone());
        }
        line.encode()
    }
}

/// `PONG <payload>\n`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PongReply {
    payload: Option<String>,
}

impl PongReply {
    /// Constructs a PONG reply that echoes the supplied payload.
    pub fn echo(payload: String) -> Self {
        assert!(
            payload.len() <= MAX_SAM_OPTION_VALUE_BYTES,
            "PONG payload exceeds the SAM byte ceiling",
        );
        Self {
            payload: Some(payload),
        }
    }

    /// Constructs a PONG reply without a payload.
    pub fn empty() -> Self {
        Self { payload: None }
    }

    /// Returns the payload, if any.
    pub fn payload(&self) -> Option<&str> {
        self.payload.as_deref()
    }

    /// Encodes the reply as a wire line.
    pub fn encode(&self) -> String {
        let mut out = String::from("PONG");
        if let Some(payload) = &self.payload {
            out.push(' ');
            out.push_str(payload);
        }
        out.push('\n');
        out
    }
}

/// Re-exports for convenience. [`Reply`] owns all reply kinds.
pub use crate::sam::reply::Reply as _Reply;
pub use crate::sam::reply::ReplyLine as _ReplyLine;
pub use crate::sam::reply::ReplyResult as _ReplyResult;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_ok_encodes_with_version() {
        let reply = HelloReply::ok(SamVersion::const_new(3, 1));
        let encoded = reply.encode();
        assert_eq!(encoded, "HELLO REPLY RESULT=OK VERSION=3.1\n");
    }

    #[test]
    fn hello_no_version_carries_message() {
        let reply = HelloReply::no_version(Some("client requested 3.3 only".to_owned()));
        let encoded = reply.encode();
        assert_eq!(
            encoded,
            "HELLO REPLY RESULT=NOVERSION MESSAGE=\"client requested 3.3 only\"\n"
        );
    }

    #[test]
    fn pong_reply_carries_payload() {
        let reply = PongReply::echo("hello".to_owned());
        assert_eq!(reply.encode(), "PONG hello\n");
    }

    #[test]
    fn stream_status_error_carries_message() {
        let reply = StreamStatus::error(ReplyResult::CantReachPeer, Some("unreachable".to_owned()));
        assert_eq!(
            reply.encode(),
            "STREAM STATUS RESULT=CANT_REACH_PEER MESSAGE=unreachable\n"
        );
    }

    #[test]
    fn dest_reply_ok_carries_pub_and_priv() {
        let bytes = vec![0x42_u8; 455];
        let destination = SamPrivateDestination::from_raw_for_test(bytes);
        let reply = DestReply::ok(&destination);
        let encoded = reply.encode();
        assert!(encoded.starts_with("DEST REPLY RESULT=OK PUB="));
        assert!(encoded.contains(" PRIV="));
    }
}
