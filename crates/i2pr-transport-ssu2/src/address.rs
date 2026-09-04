//! Strict, runtime-neutral SSU2 v2 RouterAddress validation.
//!
//! This module owns protocol address values, not name resolution,
//! socket creation, publication policy, or reachability claims. A
//! parsed address never implies reachability or publication approval;
//! see [`Ssu2RouterAddress::address_class`].
//!
//! Normative traceability: `specs/protocols/09-ssu2.md` and the Plan
//! 155 source refresh. The SSU2 specification page defines the packet
//! layer; RouterAddress option shapes follow the deployed `SSU2`
//! address conventions shared with NTCP2 (`v`, `s`, `i`, `host`,
//! `port`, `mtu`, `caps`) plus SSU2 introducer groups
//! (`ihostN`/`iportN`/`ikeyN`/`itagN`). Where the spec leaves
//! publication details implementation-defined, this parser is strict:
//! unknown options are rejected (no deployed extra option is
//! currently accepted; any future one needs an explicit allowlist
//! entry, following the NTCP2 `pq` precedent), hostnames are refused,
//! and PQ-hybrid `v=3`/`v=4` values are classified as unsupported,
//! never as malformed v2.

use std::{
    fmt,
    net::{IpAddr, SocketAddr},
    str::FromStr,
};

use i2pr_proto::{Date, RouterAddress};
use i2pr_transport::AddressFamily;
use thiserror::Error;

use crate::constants;

/// The lowest UDP port accepted by the SSU2 address parser.
pub const SSU2_MIN_PORT: u16 = 1;
/// The highest UDP port accepted by the SSU2 address parser.
pub const SSU2_MAX_PORT: u16 = u16::MAX;
/// The exact binary length of an SSU2 static public key (`s`).
pub const SSU2_STATIC_PUBLIC_KEY_LENGTH: usize = constants::KEY_LENGTH;
/// The exact binary length of an SSU2 introduction key (`i`).
pub const SSU2_INTRO_KEY_LENGTH: usize = constants::KEY_LENGTH;
/// The current SSU2 RouterAddress version.
pub const SSU2_ROUTER_ADDRESS_VERSION: u8 = constants::SSU2_VERSION;

const STATIC_KEY_OPTION: &str = "s";
const INTRO_KEY_OPTION: &str = "i";
const HOST_OPTION: &str = "host";
const PORT_OPTION: &str = "port";
const VERSION_OPTION: &str = "v";
const CAPS_OPTION: &str = "caps";
const MTU_OPTION: &str = "mtu";

/// A bounded failure while parsing or constructing SSU2 address data.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum Ssu2AddressError {
    /// The RouterAddress transport style is not SSU2 (SSU1 excluded).
    #[error("unsupported SSU2 RouterAddress transport style")]
    UnsupportedTransportStyle,
    /// The RouterAddress version is not the supported v2 value.
    /// PQ-hybrid v3/v4 and unknown future versions land here, never
    /// in the malformed-v2 path.
    #[error("unsupported SSU2 RouterAddress version {version}")]
    UnsupportedVersion {
        /// The exact version string carried by the address.
        version: String,
    },
    /// An option key is not part of the SSU2 address vocabulary.
    #[error("unknown SSU2 RouterAddress option")]
    UnknownOption,
    /// An option occurred more than once in an option-entry sequence.
    #[error("duplicate SSU2 RouterAddress option {option}")]
    DuplicateOption {
        /// The fixed option category that was repeated.
        option: &'static str,
    },
    /// A required option is absent.
    #[error("missing SSU2 RouterAddress option {option}")]
    MissingOption {
        /// The fixed option category that was absent.
        option: &'static str,
    },
    /// Two options have an invalid presence or value relationship.
    #[error("conflicting SSU2 RouterAddress options {first} and {second}")]
    ConflictingOptions {
        /// The first fixed option category in the conflict.
        first: &'static str,
        /// The second fixed option category in the conflict.
        second: &'static str,
    },
    /// An option uses the right key but has a malformed value.
    #[error("invalid SSU2 RouterAddress option {option}")]
    InvalidOptionValue {
        /// The fixed option category whose value was rejected.
        option: &'static str,
    },
    /// A host value was not a literal IPv4 or IPv6 address.
    #[error("SSU2 RouterAddress host must be a literal IP address")]
    HostnameNotAllowed,
    /// A port was not a canonical decimal value.
    #[error("SSU2 RouterAddress port is not a canonical decimal value")]
    InvalidPort,
    /// A port was outside the protocol's accepted range.
    #[error("SSU2 RouterAddress port is outside 1..=65535")]
    PortOutOfRange,
    /// More introducer groups were published than the parser retains.
    #[error("SSU2 RouterAddress publishes too many introducers")]
    TooManyIntroducers,
    /// One introducer group has a malformed field.
    #[error("invalid SSU2 RouterAddress introducer {index}")]
    InvalidIntroducer {
        /// The zero-based introducer group index.
        index: usize,
    },
    /// One introducer group is missing a required field.
    #[error("missing SSU2 RouterAddress introducer field {option}{index}")]
    MissingIntroducerField {
        /// The introducer field prefix (`ihost`, `iport`, `ikey`, `itag`).
        option: &'static str,
        /// The zero-based introducer group index.
        index: usize,
    },
    /// An endpoint-dependent operation was attempted without host and port.
    #[error("SSU2 RouterAddress has no complete endpoint")]
    MissingEndpoint,
    /// A resolved endpoint did not match the parsed literal RouterAddress.
    #[error("resolved SSU2 dial target does not match RouterAddress endpoint")]
    EndpointMismatch,
    /// The static public key was malformed or an all-zero value.
    #[error("invalid SSU2 static public key")]
    InvalidStaticPublicKey,
    /// The introduction key was malformed or an all-zero value.
    #[error("invalid SSU2 introduction key")]
    InvalidIntroKey,
}

/// The published SSU2 transport style identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Ssu2TransportStyle {
    /// The address supports SSU2 (classical v2).
    Ssu2,
}

impl Ssu2TransportStyle {
    /// Parses the exact published SSU2 transport style identifier.
    pub fn parse(value: &str) -> Result<Self, Ssu2AddressError> {
        match value {
            "SSU2" => Ok(Self::Ssu2),
            _ => Err(Ssu2AddressError::UnsupportedTransportStyle),
        }
    }

    /// Returns the exact RouterAddress transport style identifier.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ssu2 => "SSU2",
        }
    }
}

/// A validated SSU2 static X25519 public key, with redacted diagnostics.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct StaticPublicKey([u8; SSU2_STATIC_PUBLIC_KEY_LENGTH]);

impl StaticPublicKey {
    /// Constructs a key, rejecting the all-zero low-order encoding.
    pub fn new(bytes: [u8; SSU2_STATIC_PUBLIC_KEY_LENGTH]) -> Result<Self, Ssu2AddressError> {
        if bytes.iter().all(|byte| *byte == 0) {
            return Err(Ssu2AddressError::InvalidStaticPublicKey);
        }
        Ok(Self(bytes))
    }

    /// Borrows the exact wire bytes.
    pub const fn as_bytes(&self) -> &[u8; SSU2_STATIC_PUBLIC_KEY_LENGTH] {
        &self.0
    }
}

impl fmt::Debug for StaticPublicKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("StaticPublicKey")
            .field(&"<redacted>")
            .finish()
    }
}

/// A validated SSU2 introduction key, with redacted diagnostics.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct IntroKey([u8; SSU2_INTRO_KEY_LENGTH]);

impl IntroKey {
    /// Constructs a key, rejecting the all-zero value that would void
    /// header-protection binding.
    pub fn new(bytes: [u8; SSU2_INTRO_KEY_LENGTH]) -> Result<Self, Ssu2AddressError> {
        if bytes.iter().all(|byte| *byte == 0) {
            return Err(Ssu2AddressError::InvalidIntroKey);
        }
        Ok(Self(bytes))
    }

    /// Borrows the exact wire bytes.
    pub const fn as_bytes(&self) -> &[u8; SSU2_INTRO_KEY_LENGTH] {
        &self.0
    }
}

impl fmt::Debug for IntroKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("IntroKey")
            .field(&"<redacted>")
            .finish()
    }
}

/// The SSU2 static/intro key material required by a complete address.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct Ssu2AddressMaterial {
    static_public_key: StaticPublicKey,
    intro_key: IntroKey,
}

impl Ssu2AddressMaterial {
    /// Validates exact key widths and constructs address material.
    pub fn from_bytes(
        static_public_key: [u8; SSU2_STATIC_PUBLIC_KEY_LENGTH],
        intro_key: [u8; SSU2_INTRO_KEY_LENGTH],
    ) -> Result<Self, Ssu2AddressError> {
        Ok(Self::from_parts(
            StaticPublicKey::new(static_public_key)?,
            IntroKey::new(intro_key)?,
        ))
    }

    /// Constructs material from already validated keys.
    pub const fn from_parts(static_public_key: StaticPublicKey, intro_key: IntroKey) -> Self {
        Self {
            static_public_key,
            intro_key,
        }
    }

    /// Returns the validated static public key.
    pub const fn static_public_key(&self) -> StaticPublicKey {
        self.static_public_key
    }

    /// Returns the validated introduction key.
    pub const fn intro_key(&self) -> IntroKey {
        self.intro_key
    }
}

impl fmt::Debug for Ssu2AddressMaterial {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Ssu2AddressMaterial")
            .field("static_public_key", &"<redacted>")
            .field("intro_key", &"<redacted>")
            .finish()
    }
}

/// A literal IP/UDP endpoint used by the runtime-neutral listen/dial types.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct Ssu2Endpoint {
    ip: IpAddr,
    port: u16,
}

impl Ssu2Endpoint {
    /// Constructs an endpoint after enforcing the SSU2 port range.
    pub fn new(ip: IpAddr, port: u16) -> Result<Self, Ssu2AddressError> {
        validate_port(port)?;
        Ok(Self { ip, port })
    }

    /// Constructs an endpoint from a resolved standard-library address value.
    pub fn from_socket_addr(address: SocketAddr) -> Result<Self, Ssu2AddressError> {
        Self::new(address.ip(), address.port())
    }

    /// Returns the literal IP address.
    pub const fn ip(self) -> IpAddr {
        self.ip
    }

    /// Returns the validated UDP port.
    pub const fn port(self) -> u16 {
        self.port
    }

    /// Returns the address-family classification without exposing an
    /// endpoint in a transport snapshot.
    pub const fn family(self) -> AddressFamily {
        match self.ip {
            IpAddr::V4(_) => AddressFamily::Ipv4,
            IpAddr::V6(_) => AddressFamily::Ipv6,
        }
    }

    /// Converts the value to the standard-library resolved endpoint type.
    pub const fn socket_addr(self) -> SocketAddr {
        SocketAddr::new(self.ip, self.port)
    }
}

impl fmt::Debug for Ssu2Endpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Ssu2Endpoint")
            .field("family", &self.family())
            .field("endpoint", &"<redacted>")
            .finish()
    }
}

/// Capability flags carried by the optional SSU2 `caps` option.
///
/// `4`/`6` advertise IPv4/IPv6 reachability families; `B` advertises
/// the peer-test role; `C` advertises the relay/introducer role.
/// Other graphic characters are accepted and ignored for forward
/// compatibility; the raw string is retained for canonical handling.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ssu2Capabilities {
    raw: String,
    bits: u8,
}

impl Ssu2Capabilities {
    const IPV4: u8 = 0b0001;
    const IPV6: u8 = 0b0010;
    const PEER_TEST: u8 = 0b0100;
    const RELAY: u8 = 0b1000;

    /// Parses a `caps` option value with the strict production rules
    /// (nonempty, bounded graphic string, no duplicate known flags).
    pub fn parse(value: &str) -> Result<Self, Ssu2AddressError> {
        parse_capabilities(value)
    }

    /// Returns no advertised capabilities.
    pub fn empty() -> Self {
        Self {
            raw: String::new(),
            bits: 0,
        }
    }

    /// Returns the exact published option value.
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    /// Returns whether outbound IPv4 capability was advertised.
    pub fn supports_ipv4(&self) -> bool {
        self.bits & Self::IPV4 != 0
    }

    /// Returns whether outbound IPv6 capability was advertised.
    pub fn supports_ipv6(&self) -> bool {
        self.bits & Self::IPV6 != 0
    }

    /// Returns whether the peer-test role was advertised (`B`).
    pub fn supports_peer_test(&self) -> bool {
        self.bits & Self::PEER_TEST != 0
    }

    /// Returns whether the relay/introducer role was advertised (`C`).
    pub fn supports_relay(&self) -> bool {
        self.bits & Self::RELAY != 0
    }
}

/// One bounded introducer group (`ihostN`/`iportN`/`ikeyN`/`itagN`).
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct Ssu2Introducer {
    endpoint: Ssu2Endpoint,
    intro_key: IntroKey,
    relay_tag: u32,
}

impl Ssu2Introducer {
    /// Constructs an introducer from validated parts.
    pub fn new(
        endpoint: Ssu2Endpoint,
        intro_key: IntroKey,
        relay_tag: u32,
    ) -> Result<Self, Ssu2AddressError> {
        if relay_tag == 0 {
            return Err(Ssu2AddressError::InvalidIntroducer { index: 0 });
        }
        Ok(Self {
            endpoint,
            intro_key,
            relay_tag,
        })
    }

    /// Returns the introducer's literal endpoint.
    pub const fn endpoint(self) -> Ssu2Endpoint {
        self.endpoint
    }

    /// Returns the introducer's literal IP address.
    pub const fn ip(self) -> IpAddr {
        self.endpoint.ip()
    }

    /// Returns the introducer's UDP port.
    pub const fn port(self) -> u16 {
        self.endpoint.port()
    }

    /// Returns the introducer's introduction key.
    pub const fn intro_key(self) -> IntroKey {
        self.intro_key
    }

    /// Returns the nonzero relay tag (`itagN`).
    pub const fn relay_tag(self) -> u32 {
        self.relay_tag
    }
}

impl fmt::Debug for Ssu2Introducer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Ssu2Introducer")
            .field("family", &self.endpoint.family())
            .field("endpoint", &"<redacted>")
            .field("intro_key", &"<redacted>")
            .field("relay_tag", &"<redacted>")
            .finish()
    }
}

/// Structural classification of a parsed SSU2 address.
///
/// This describes which contact material is present. It never implies
/// reachability or publication approval.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ssu2AddressClass {
    /// A direct endpoint with no introducer material.
    Direct,
    /// A direct endpoint that additionally publishes introducers.
    DirectWithIntroducers,
    /// No direct endpoint; contact only via published introducers.
    IntroducerOnly,
    /// Static-key-only form with no endpoint and no introducers.
    UnpublishedStatic,
}

/// A strictly parsed SSU2 RouterAddress.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ssu2RouterAddress {
    cost: u8,
    expiration: Date,
    transport_style: Ssu2TransportStyle,
    endpoint: Option<Ssu2Endpoint>,
    static_public_key: StaticPublicKey,
    intro_key: Option<IntroKey>,
    mtu: Option<u16>,
    capabilities: Ssu2Capabilities,
    introducers: Vec<Ssu2Introducer>,
}

impl Ssu2RouterAddress {
    /// Parses a structural RouterAddress and strictly validates its
    /// SSU2 options.
    pub fn parse(address: &RouterAddress) -> Result<Self, Ssu2AddressError> {
        let transport_style = Ssu2TransportStyle::parse(address.transport_style())?;
        let entries = address
            .options()
            .entries()
            .iter()
            .map(|entry| (entry.key(), entry.value()));
        let options = ParsedOptions::from_entries(entries)?;
        Self::from_parsed(
            address.cost(),
            address.expiration(),
            transport_style,
            options,
        )
    }

    /// Alias for [`Self::parse`] that makes the RouterAddress boundary
    /// explicit at call sites.
    pub fn from_router_address(address: &RouterAddress) -> Result<Self, Ssu2AddressError> {
        Self::parse(address)
    }

    /// Parses raw option entries for callers that need duplicate
    /// detection before constructing the shared canonical Mapping
    /// type. Uses cost zero and an undefined expiration because it is
    /// a pure option-validation entry point.
    pub fn from_option_entries(
        transport_style: &str,
        entries: &[(&str, &str)],
    ) -> Result<Self, Ssu2AddressError> {
        let transport_style = Ssu2TransportStyle::parse(transport_style)?;
        let options = ParsedOptions::from_entries(entries.iter().copied())?;
        Self::from_parsed(0, Date::from_millis(0), transport_style, options)
    }

    /// Returns the RouterAddress relative cost.
    pub const fn cost(&self) -> u8 {
        self.cost
    }

    /// Returns the structural RouterAddress expiration date.
    pub const fn expiration(&self) -> Date {
        self.expiration
    }

    /// Returns the SSU2 transport style.
    pub const fn transport_style(&self) -> Ssu2TransportStyle {
        self.transport_style
    }

    /// Returns the supported SSU2 RouterAddress version.
    pub const fn version(&self) -> u8 {
        SSU2_ROUTER_ADDRESS_VERSION
    }

    /// Returns the literal direct endpoint, if published.
    pub const fn endpoint(&self) -> Option<Ssu2Endpoint> {
        self.endpoint
    }

    /// Returns the literal host, if present.
    pub fn host(&self) -> Option<IpAddr> {
        self.endpoint.map(Ssu2Endpoint::ip)
    }

    /// Returns the validated UDP port, if present.
    pub fn port(&self) -> Option<u16> {
        self.endpoint.map(Ssu2Endpoint::port)
    }

    /// Returns the IPv4/IPv6 classification, if an endpoint is present.
    pub fn family(&self) -> Option<AddressFamily> {
        self.endpoint.map(Ssu2Endpoint::family)
    }

    /// Returns the validated static public key.
    pub const fn static_public_key(&self) -> StaticPublicKey {
        self.static_public_key
    }

    /// Returns the introduction key, if published.
    pub const fn intro_key(&self) -> Option<IntroKey> {
        self.intro_key
    }

    /// Returns the validated MTU, if published.
    pub const fn mtu(&self) -> Option<u16> {
        self.mtu
    }

    /// Returns the validated capability flags.
    pub fn capabilities(&self) -> &Ssu2Capabilities {
        &self.capabilities
    }

    /// Returns the bounded introducer groups.
    pub fn introducers(&self) -> &[Ssu2Introducer] {
        &self.introducers
    }

    /// Classifies which contact material is present. This never
    /// implies reachability or publication approval.
    pub fn address_class(&self) -> Ssu2AddressClass {
        match (self.endpoint, self.introducers.is_empty()) {
            (Some(_), true) => Ssu2AddressClass::Direct,
            (Some(_), false) => Ssu2AddressClass::DirectWithIntroducers,
            (None, false) => Ssu2AddressClass::IntroducerOnly,
            (None, true) => Ssu2AddressClass::UnpublishedStatic,
        }
    }

    /// Returns the complete key material, requiring a published intro key.
    pub fn address_material(&self) -> Result<Ssu2AddressMaterial, Ssu2AddressError> {
        let intro_key = self.intro_key.ok_or(Ssu2AddressError::MissingOption {
            option: INTRO_KEY_OPTION,
        })?;
        Ok(Ssu2AddressMaterial::from_parts(
            self.static_public_key,
            intro_key,
        ))
    }

    /// Converts this address into a configured literal listener.
    pub fn configured_listen(&self) -> Result<ConfiguredListenAddress, Ssu2AddressError> {
        let endpoint = self.endpoint.ok_or(Ssu2AddressError::MissingEndpoint)?;
        ConfiguredListenAddress::new(endpoint, self.address_material()?)
    }

    /// Converts this address into a resolved dial target after checking
    /// that the supplied endpoint exactly matches its literal host/port.
    pub fn resolved_dial_target(
        &self,
        resolved: SocketAddr,
    ) -> Result<ResolvedDialTarget, Ssu2AddressError> {
        let endpoint = self.endpoint.ok_or(Ssu2AddressError::MissingEndpoint)?;
        let resolved = Ssu2Endpoint::from_socket_addr(resolved)?;
        if endpoint != resolved {
            return Err(Ssu2AddressError::EndpointMismatch);
        }
        ResolvedDialTarget::new(resolved, self.address_material()?)
    }

    fn from_parsed(
        cost: u8,
        expiration: Date,
        transport_style: Ssu2TransportStyle,
        options: ParsedOptions<'_>,
    ) -> Result<Self, Ssu2AddressError> {
        let static_public_key = decode_static_public_key(options.static_public_key.ok_or(
            Ssu2AddressError::MissingOption {
                option: STATIC_KEY_OPTION,
            },
        )?)?;
        let version = options.version.ok_or(Ssu2AddressError::MissingOption {
            option: VERSION_OPTION,
        })?;
        if version != "2" {
            return Err(Ssu2AddressError::UnsupportedVersion {
                version: version.to_owned(),
            });
        }

        let endpoint = match (options.host, options.port) {
            (None, None) => None,
            (Some(_), None) | (None, Some(_)) => {
                return Err(Ssu2AddressError::ConflictingOptions {
                    first: HOST_OPTION,
                    second: PORT_OPTION,
                });
            }
            (Some(host), Some(port)) => Some(parse_endpoint(host, port)?),
        };

        let introducers = parse_introducers(&options.introducer_fields)?;

        let intro_key = match (endpoint, introducers.is_empty(), options.intro_key) {
            (None, true, None) => None,
            (None, true, Some(_)) => {
                return Err(Ssu2AddressError::ConflictingOptions {
                    first: INTRO_KEY_OPTION,
                    second: HOST_OPTION,
                });
            }
            (_, _, None) => {
                return Err(Ssu2AddressError::MissingOption {
                    option: INTRO_KEY_OPTION,
                });
            }
            (_, _, Some(value)) => Some(decode_intro_key(value)?),
        };

        let mtu = options.mtu.map(parse_mtu).transpose()?;
        let capabilities = options
            .capabilities
            .map(parse_capabilities)
            .transpose()?
            .unwrap_or_else(Ssu2Capabilities::empty);

        Ok(Self {
            cost,
            expiration,
            transport_style,
            endpoint,
            static_public_key,
            intro_key,
            mtu,
            capabilities,
            introducers,
        })
    }
}

impl TryFrom<&RouterAddress> for Ssu2RouterAddress {
    type Error = Ssu2AddressError;

    fn try_from(address: &RouterAddress) -> Result<Self, Self::Error> {
        Self::parse(address)
    }
}

impl TryFrom<RouterAddress> for Ssu2RouterAddress {
    type Error = Ssu2AddressError;

    fn try_from(address: RouterAddress) -> Result<Self, Self::Error> {
        Self::parse(&address)
    }
}

/// A configured literal SSU2 listen address.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ConfiguredListenAddress {
    endpoint: Ssu2Endpoint,
    material: Ssu2AddressMaterial,
}

impl ConfiguredListenAddress {
    /// Constructs a listener endpoint without opening or binding a socket.
    pub fn new(
        endpoint: Ssu2Endpoint,
        material: Ssu2AddressMaterial,
    ) -> Result<Self, Ssu2AddressError> {
        validate_port(endpoint.port())?;
        Ok(Self { endpoint, material })
    }

    /// Constructs a listener from a parsed RouterAddress.
    pub fn from_router_address(address: &Ssu2RouterAddress) -> Result<Self, Ssu2AddressError> {
        address.configured_listen()
    }

    /// Returns the literal endpoint without any socket ownership.
    pub const fn endpoint(self) -> Ssu2Endpoint {
        self.endpoint
    }

    /// Returns the literal IP address.
    pub const fn ip(self) -> IpAddr {
        self.endpoint.ip()
    }

    /// Returns the validated UDP port.
    pub const fn port(self) -> u16 {
        self.endpoint.port()
    }

    /// Returns the IPv4/IPv6 classification.
    pub const fn family(self) -> AddressFamily {
        self.endpoint.family()
    }

    /// Returns the local static public key.
    pub const fn static_public_key(self) -> StaticPublicKey {
        self.material.static_public_key()
    }

    /// Returns the local introduction key.
    pub const fn intro_key(self) -> IntroKey {
        self.material.intro_key()
    }
}

impl fmt::Debug for ConfiguredListenAddress {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConfiguredListenAddress")
            .field("family", &self.family())
            .field("endpoint", &"<redacted>")
            .field("material", &"<redacted>")
            .finish()
    }
}

/// A resolved SSU2 dial target. Resolution is supplied by the caller;
/// this type performs no DNS lookup and owns no socket or runtime
/// resource.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ResolvedDialTarget {
    endpoint: Ssu2Endpoint,
    expected_peer_material: Ssu2AddressMaterial,
}

impl ResolvedDialTarget {
    /// Constructs a resolved dial target from a validated endpoint and
    /// the RouterAddress material expected from the peer.
    pub fn new(
        endpoint: Ssu2Endpoint,
        expected_peer_material: Ssu2AddressMaterial,
    ) -> Result<Self, Ssu2AddressError> {
        validate_port(endpoint.port())?;
        Ok(Self {
            endpoint,
            expected_peer_material,
        })
    }

    /// Constructs a resolved target from a parsed RouterAddress and a
    /// caller-supplied resolved endpoint, requiring an exact literal match.
    pub fn from_router_address(
        address: &Ssu2RouterAddress,
        resolved: SocketAddr,
    ) -> Result<Self, Ssu2AddressError> {
        address.resolved_dial_target(resolved)
    }

    /// Returns the resolved endpoint value without socket ownership.
    pub const fn endpoint(self) -> Ssu2Endpoint {
        self.endpoint
    }

    /// Returns the resolved IP address.
    pub const fn ip(self) -> IpAddr {
        self.endpoint.ip()
    }

    /// Returns the resolved UDP port.
    pub const fn port(self) -> u16 {
        self.endpoint.port()
    }

    /// Returns the IPv4/IPv6 classification.
    pub const fn family(self) -> AddressFamily {
        self.endpoint.family()
    }

    /// Returns the standard-library address value for a runtime adapter.
    pub const fn socket_addr(self) -> SocketAddr {
        self.endpoint.socket_addr()
    }

    /// Returns the expected peer static public key.
    pub const fn expected_static_public_key(self) -> StaticPublicKey {
        self.expected_peer_material.static_public_key()
    }

    /// Returns the expected peer introduction key.
    pub const fn expected_intro_key(self) -> IntroKey {
        self.expected_peer_material.intro_key()
    }
}

impl fmt::Debug for ResolvedDialTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResolvedDialTarget")
            .field("family", &self.family())
            .field("endpoint", &"<redacted>")
            .field("expected_peer_material", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Copy, Default)]
struct ParsedOptions<'a> {
    host: Option<&'a str>,
    port: Option<&'a str>,
    static_public_key: Option<&'a str>,
    intro_key: Option<&'a str>,
    version: Option<&'a str>,
    capabilities: Option<&'a str>,
    mtu: Option<&'a str>,
    introducer_fields: IntroducerFields<'a>,
}

#[derive(Clone, Copy, Default)]
struct IntroducerFields<'a> {
    hosts: [Option<&'a str>; constants::MAX_SSU2_INTRODUCERS],
    ports: [Option<&'a str>; constants::MAX_SSU2_INTRODUCERS],
    keys: [Option<&'a str>; constants::MAX_SSU2_INTRODUCERS],
    tags: [Option<&'a str>; constants::MAX_SSU2_INTRODUCERS],
    overflow: bool,
}

impl<'a> ParsedOptions<'a> {
    fn from_entries<I>(entries: I) -> Result<Self, Ssu2AddressError>
    where
        I: IntoIterator<Item = (&'a str, &'a str)>,
    {
        let mut parsed = Self::default();
        for (key, value) in entries {
            if let Some(index) = parse_introducer_index(key) {
                parsed.introducer_fields.store(index, value)?;
                continue;
            }
            match key {
                HOST_OPTION => store(&mut parsed.host, value, HOST_OPTION)?,
                PORT_OPTION => store(&mut parsed.port, value, PORT_OPTION)?,
                STATIC_KEY_OPTION => {
                    store(&mut parsed.static_public_key, value, STATIC_KEY_OPTION)?
                }
                INTRO_KEY_OPTION => store(&mut parsed.intro_key, value, INTRO_KEY_OPTION)?,
                VERSION_OPTION => store(&mut parsed.version, value, VERSION_OPTION)?,
                CAPS_OPTION => store(&mut parsed.capabilities, value, CAPS_OPTION)?,
                MTU_OPTION => store(&mut parsed.mtu, value, MTU_OPTION)?,
                _ => return Err(Ssu2AddressError::UnknownOption),
            }
        }
        Ok(parsed)
    }
}

const IHOST_PREFIX: &str = "ihost";
const IPORT_PREFIX: &str = "iport";
const IKEY_PREFIX: &str = "ikey";
const ITAG_PREFIX: &str = "itag";

fn parse_introducer_index(key: &str) -> Option<(usize, &'static str)> {
    for prefix in [IHOST_PREFIX, IPORT_PREFIX, IKEY_PREFIX, ITAG_PREFIX] {
        if let Some(suffix) = key.strip_prefix(prefix)
            && suffix.len() == 1
            && suffix.bytes().all(|byte| byte.is_ascii_digit())
        {
            let index = usize::from(suffix.bytes().next()? - b'0');
            return Some((index, prefix));
        }
    }
    None
}

impl<'a> IntroducerFields<'a> {
    fn store(
        &mut self,
        indexed: (usize, &'static str),
        value: &'a str,
    ) -> Result<(), Ssu2AddressError> {
        let (index, prefix) = indexed;
        if index >= constants::MAX_SSU2_INTRODUCERS {
            self.overflow = true;
            return Err(Ssu2AddressError::TooManyIntroducers);
        }
        let slot = match prefix {
            IHOST_PREFIX => &mut self.hosts[index],
            IPORT_PREFIX => &mut self.ports[index],
            IKEY_PREFIX => &mut self.keys[index],
            ITAG_PREFIX => &mut self.tags[index],
            _ => return Err(Ssu2AddressError::UnknownOption),
        };
        if slot.replace(value).is_some() {
            return Err(Ssu2AddressError::InvalidIntroducer { index });
        }
        Ok(())
    }
}

fn store<'a>(
    slot: &mut Option<&'a str>,
    value: &'a str,
    option: &'static str,
) -> Result<(), Ssu2AddressError> {
    if slot.replace(value).is_some() {
        return Err(Ssu2AddressError::DuplicateOption { option });
    }
    Ok(())
}

fn parse_introducers(
    fields: &IntroducerFields<'_>,
) -> Result<Vec<Ssu2Introducer>, Ssu2AddressError> {
    if fields.overflow {
        return Err(Ssu2AddressError::TooManyIntroducers);
    }
    let mut introducers = Vec::new();
    let mut ended = false;
    for index in 0..constants::MAX_SSU2_INTRODUCERS {
        let group = (
            fields.hosts[index],
            fields.ports[index],
            fields.keys[index],
            fields.tags[index],
        );
        match group {
            (None, None, None, None) => {
                ended = true;
            }
            (Some(host), Some(port), Some(key), Some(tag)) => {
                if ended {
                    return Err(Ssu2AddressError::ConflictingOptions {
                        first: IHOST_PREFIX,
                        second: HOST_OPTION,
                    });
                }
                introducers.push(parse_introducer(index, host, port, key, tag)?);
            }
            _ => {
                let (field, _) = [
                    (IHOST_PREFIX, fields.hosts[index]),
                    (IPORT_PREFIX, fields.ports[index]),
                    (IKEY_PREFIX, fields.keys[index]),
                    (ITAG_PREFIX, fields.tags[index]),
                ]
                .into_iter()
                .find(|(_, value)| value.is_none())
                .expect("partial group has a missing field");
                return Err(Ssu2AddressError::MissingIntroducerField {
                    option: field,
                    index,
                });
            }
        }
    }
    Ok(introducers)
}

fn parse_introducer(
    index: usize,
    host: &str,
    port: &str,
    key: &str,
    tag: &str,
) -> Result<Ssu2Introducer, Ssu2AddressError> {
    let ip = IpAddr::from_str(host).map_err(|_| Ssu2AddressError::InvalidIntroducer { index })?;
    let port = parse_port(port).map_err(|_| Ssu2AddressError::InvalidIntroducer { index })?;
    let endpoint =
        Ssu2Endpoint::new(ip, port).map_err(|_| Ssu2AddressError::InvalidIntroducer { index })?;
    let intro_key =
        decode_intro_key(key).map_err(|_| Ssu2AddressError::InvalidIntroducer { index })?;
    let tag = parse_relay_tag(tag).ok_or(Ssu2AddressError::InvalidIntroducer { index })?;
    Ssu2Introducer::new(endpoint, intro_key, tag)
        .map_err(|_| Ssu2AddressError::InvalidIntroducer { index })
}

fn parse_relay_tag(value: &str) -> Option<u32> {
    if value.is_empty() || value.len() > 10 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if value.len() > 1 && value.starts_with('0') {
        return None;
    }
    let tag = value.parse::<u32>().ok()?;
    if tag == 0 { None } else { Some(tag) }
}

fn parse_endpoint(host: &str, port: &str) -> Result<Ssu2Endpoint, Ssu2AddressError> {
    let ip = IpAddr::from_str(host).map_err(|_| Ssu2AddressError::HostnameNotAllowed)?;
    let port = parse_port(port)?;
    Ssu2Endpoint::new(ip, port)
}

fn parse_port(value: &str) -> Result<u16, Ssu2AddressError> {
    if value.is_empty()
        || value.len() > 5
        || value.len() > 1 && value.starts_with('0')
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(Ssu2AddressError::InvalidPort);
    }
    let port = value
        .parse::<u32>()
        .map_err(|_| Ssu2AddressError::InvalidPort)?;
    let port = u16::try_from(port).map_err(|_| Ssu2AddressError::PortOutOfRange)?;
    validate_port(port)?;
    Ok(port)
}

fn validate_port(port: u16) -> Result<(), Ssu2AddressError> {
    if !(SSU2_MIN_PORT..=SSU2_MAX_PORT).contains(&port) {
        return Err(Ssu2AddressError::PortOutOfRange);
    }
    Ok(())
}

fn parse_mtu(value: &str) -> Result<u16, Ssu2AddressError> {
    if value.is_empty()
        || value.len() > 4
        || value.len() > 1 && value.starts_with('0')
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(Ssu2AddressError::InvalidOptionValue { option: MTU_OPTION });
    }
    let mtu = value
        .parse::<u16>()
        .map_err(|_| Ssu2AddressError::InvalidOptionValue { option: MTU_OPTION })?;
    if !(constants::SSU2_MIN_MTU..=constants::SSU2_MAX_MTU).contains(&mtu) {
        return Err(Ssu2AddressError::InvalidOptionValue { option: MTU_OPTION });
    }
    Ok(mtu)
}

fn decode_static_public_key(value: &str) -> Result<StaticPublicKey, Ssu2AddressError> {
    let bytes = decode_i2p_base64::<SSU2_STATIC_PUBLIC_KEY_LENGTH>(value, STATIC_KEY_OPTION)?;
    StaticPublicKey::new(bytes).map_err(|_| Ssu2AddressError::InvalidStaticPublicKey)
}

fn decode_intro_key(value: &str) -> Result<IntroKey, Ssu2AddressError> {
    let bytes = decode_i2p_base64::<SSU2_INTRO_KEY_LENGTH>(value, INTRO_KEY_OPTION)?;
    IntroKey::new(bytes).map_err(|_| Ssu2AddressError::InvalidIntroKey)
}

pub(crate) fn decode_i2p_base64<const N: usize>(
    value: &str,
    option: &'static str,
) -> Result<[u8; N], Ssu2AddressError> {
    let expected_length = 4 * N.div_ceil(3);
    let padding = match N % 3 {
        0 => 0,
        1 => 2,
        _ => 1,
    };
    if value.len() != expected_length {
        return Err(Ssu2AddressError::InvalidOptionValue { option });
    }
    let bytes = value.as_bytes();
    let data_length = expected_length - padding;
    if bytes[data_length..].iter().any(|byte| *byte != b'=') || bytes[..data_length].contains(&b'=')
    {
        return Err(Ssu2AddressError::InvalidOptionValue { option });
    }

    let mut output = [0_u8; N];
    let mut output_index = 0;
    let mut accumulator = 0_u16;
    let mut bits = 0_u8;
    for byte in &bytes[..data_length] {
        let digit =
            i2p_base64_digit(*byte).ok_or(Ssu2AddressError::InvalidOptionValue { option })?;
        accumulator = (accumulator << 6) | u16::from(digit);
        bits += 6;
        while bits >= 8 {
            bits -= 8;
            if output_index == N {
                return Err(Ssu2AddressError::InvalidOptionValue { option });
            }
            output[output_index] = ((accumulator >> bits) & 0xff) as u8;
            output_index += 1;
            accumulator &= if bits == 0 { 0 } else { (1_u16 << bits) - 1 };
        }
    }
    if output_index != N || (bits > 0 && accumulator != 0) {
        return Err(Ssu2AddressError::InvalidOptionValue { option });
    }
    Ok(output)
}

fn i2p_base64_digit(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'-' => Some(62),
        b'~' => Some(63),
        _ => None,
    }
}

/// Encodes bytes with the I2P base64 alphabet (`A-Za-z0-9-~`, `=` pad).
///
/// Used by the Plan 159 publication snapshot builder to emit canonical
/// `s`/`i`/`ikeyN` option values; parsing stays strict in
/// [`decode_i2p_base64`].
pub(crate) fn encode_i2p_base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-~";
    let mut output = String::new();
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = *chunk.get(1).unwrap_or(&0);
        let c = *chunk.get(2).unwrap_or(&0);
        output.push(ALPHABET[(a >> 2) as usize] as char);
        output.push(ALPHABET[((a & 0x03) << 4 | b >> 4) as usize] as char);
        output.push(if chunk.len() > 1 {
            ALPHABET[((b & 0x0f) << 2 | c >> 6) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            ALPHABET[(c & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    output
}

fn parse_capabilities(value: &str) -> Result<Ssu2Capabilities, Ssu2AddressError> {
    if value.is_empty() || value.len() > constants::MAX_SSU2_CAPS_BYTES {
        return Err(Ssu2AddressError::InvalidOptionValue {
            option: CAPS_OPTION,
        });
    }
    if !value.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err(Ssu2AddressError::InvalidOptionValue {
            option: CAPS_OPTION,
        });
    }
    let mut bits = 0;
    for byte in value.bytes() {
        let flag = match byte {
            b'4' => Ssu2Capabilities::IPV4,
            b'6' => Ssu2Capabilities::IPV6,
            b'B' => Ssu2Capabilities::PEER_TEST,
            b'C' => Ssu2Capabilities::RELAY,
            _ => continue,
        };
        if bits & flag != 0 {
            return Err(Ssu2AddressError::InvalidOptionValue {
                option: CAPS_OPTION,
            });
        }
        bits |= flag;
    }
    Ok(Ssu2Capabilities {
        raw: value.to_owned(),
        bits,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_proto::Mapping;

    /// Deterministic test-only static key. Never operational: the
    /// repeating byte pattern is not router key material.
    const TEST_STATIC_KEY: [u8; SSU2_STATIC_PUBLIC_KEY_LENGTH] =
        [0x42; SSU2_STATIC_PUBLIC_KEY_LENGTH];
    /// Deterministic test-only intro key. Never operational.
    const TEST_INTRO_KEY: [u8; SSU2_INTRO_KEY_LENGTH] = [0x24; SSU2_INTRO_KEY_LENGTH];

    fn encode_i2p_base64(bytes: &[u8]) -> String {
        super::encode_i2p_base64(bytes)
    }

    fn direct_entries() -> Vec<(&'static str, String)> {
        vec![
            (HOST_OPTION, "192.0.2.1".to_owned()),
            (PORT_OPTION, "12345".to_owned()),
            (STATIC_KEY_OPTION, encode_i2p_base64(&TEST_STATIC_KEY)),
            (INTRO_KEY_OPTION, encode_i2p_base64(&TEST_INTRO_KEY)),
            (VERSION_OPTION, "2".to_owned()),
            (CAPS_OPTION, "46BC".to_owned()),
            (MTU_OPTION, "1280".to_owned()),
        ]
    }

    fn borrowed<'x>(entries: &'x [(&'x str, String)]) -> Vec<(&'x str, &'x str)> {
        entries
            .iter()
            .map(|(key, value)| (*key, value.as_str()))
            .collect()
    }

    fn parse(entries: &[(&str, String)]) -> Result<Ssu2RouterAddress, Ssu2AddressError> {
        Ssu2RouterAddress::from_option_entries("SSU2", &borrowed(entries))
    }

    fn structural(entries: &[(&str, &str)]) -> RouterAddress {
        let options = Mapping::from_entries(
            entries
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect(),
        )
        .expect("canonical options");
        RouterAddress::new(10, Date::from_millis(0), "SSU2".to_owned(), options)
            .expect("RouterAddress")
    }

    #[test]
    fn parses_direct_ipv4_address_with_full_material() {
        let entries = direct_entries();
        let parsed = parse(&entries).expect("direct IPv4 address");
        assert_eq!(parsed.host(), Some("192.0.2.1".parse().unwrap()));
        assert_eq!(parsed.port(), Some(12345));
        assert_eq!(parsed.family(), Some(AddressFamily::Ipv4));
        assert_eq!(parsed.static_public_key().as_bytes(), &TEST_STATIC_KEY);
        assert_eq!(parsed.intro_key().unwrap().as_bytes(), &TEST_INTRO_KEY);
        assert_eq!(parsed.mtu(), Some(1280));
        assert!(parsed.capabilities().supports_ipv4());
        assert!(parsed.capabilities().supports_ipv6());
        assert!(parsed.capabilities().supports_peer_test());
        assert!(parsed.capabilities().supports_relay());
        assert_eq!(parsed.address_class(), Ssu2AddressClass::Direct);
        assert!(parsed.introducers().is_empty());
        let listen = parsed.configured_listen().expect("listen");
        assert_eq!(listen.port(), 12345);
        let dial = parsed
            .resolved_dial_target("192.0.2.1:12345".parse().unwrap())
            .expect("dial");
        assert_eq!(dial.socket_addr().port(), 12345);
    }

    #[test]
    fn parses_ipv6_and_structural_router_address() {
        let mut entries = direct_entries();
        entries[0].1 = "2001:db8::1".to_owned();
        let parsed = parse(&entries).expect("IPv6 address");
        assert_eq!(parsed.family(), Some(AddressFamily::Ipv6));
        assert_eq!(parsed.address_class(), Ssu2AddressClass::Direct);

        let pairs = entries
            .iter()
            .map(|(key, value)| (*key, value.as_str()))
            .collect::<Vec<_>>();
        let router_address = structural(&pairs);
        let parsed = Ssu2RouterAddress::parse(&router_address).expect("structural");
        assert_eq!(parsed.cost(), 10);
        assert_eq!(parsed.endpoint().unwrap().socket_addr().port(), 12345);
    }

    #[test]
    fn parses_firewalled_introducer_only_address() {
        let entries = vec![
            (STATIC_KEY_OPTION, encode_i2p_base64(&TEST_STATIC_KEY)),
            (INTRO_KEY_OPTION, encode_i2p_base64(&TEST_INTRO_KEY)),
            (VERSION_OPTION, "2".to_owned()),
            (CAPS_OPTION, "BC".to_owned()),
            ("ihost0", "192.0.2.10".to_owned()),
            ("iport0", "23456".to_owned()),
            ("ikey0", encode_i2p_base64(&TEST_INTRO_KEY)),
            ("itag0", "987654321".to_owned()),
        ];
        let parsed = parse(&entries).expect("introducer-only address");
        assert_eq!(parsed.endpoint(), None);
        assert_eq!(parsed.address_class(), Ssu2AddressClass::IntroducerOnly);
        assert_eq!(parsed.introducers().len(), 1);
        let introducer = parsed.introducers()[0];
        assert_eq!(introducer.ip(), "192.0.2.10".parse::<IpAddr>().unwrap());
        assert_eq!(introducer.port(), 23456);
        assert_eq!(introducer.relay_tag(), 987654321);
        assert_eq!(
            parsed.configured_listen(),
            Err(Ssu2AddressError::MissingEndpoint)
        );
    }

    #[test]
    fn parses_direct_with_introducers_and_unpublished_static() {
        let mut entries = direct_entries();
        entries.push(("ihost0", "192.0.2.10".to_owned()));
        entries.push(("iport0", "23456".to_owned()));
        entries.push(("ikey0", encode_i2p_base64(&TEST_INTRO_KEY)));
        entries.push(("itag0", "7".to_owned()));
        let parsed = parse(&entries).expect("direct with introducers");
        assert_eq!(
            parsed.address_class(),
            Ssu2AddressClass::DirectWithIntroducers
        );

        let unpublished = vec![
            (STATIC_KEY_OPTION, encode_i2p_base64(&TEST_STATIC_KEY)),
            (VERSION_OPTION, "2".to_owned()),
        ];
        let parsed = parse(&unpublished).expect("unpublished static");
        assert_eq!(parsed.address_class(), Ssu2AddressClass::UnpublishedStatic);
        assert_eq!(parsed.intro_key(), None);
        assert_eq!(
            parsed.configured_listen(),
            Err(Ssu2AddressError::MissingEndpoint)
        );
    }

    #[test]
    fn rejects_unsupported_versions_distinctly() {
        for version in ["3", "4", "5", "1", "02", "2,3", ""] {
            let mut entries = direct_entries();
            entries[4].1 = version.to_owned();
            assert_eq!(
                parse(&entries),
                Err(Ssu2AddressError::UnsupportedVersion {
                    version: version.to_owned(),
                }),
                "version {version:?}"
            );
        }
    }

    #[test]
    fn rejects_duplicate_conflicting_unknown_and_bad_transport() {
        let entries = direct_entries();
        let duplicate = vec![
            (HOST_OPTION, entries[0].1.as_str()),
            (HOST_OPTION, "192.0.2.2"),
        ];
        assert_eq!(
            Ssu2RouterAddress::from_option_entries("SSU2", &duplicate),
            Err(Ssu2AddressError::DuplicateOption {
                option: HOST_OPTION
            })
        );

        let missing_port = vec![
            (HOST_OPTION, "192.0.2.1"),
            (STATIC_KEY_OPTION, entries[2].1.as_str()),
            (VERSION_OPTION, "2"),
        ];
        assert_eq!(
            Ssu2RouterAddress::from_option_entries("SSU2", &missing_port),
            Err(Ssu2AddressError::ConflictingOptions {
                first: HOST_OPTION,
                second: PORT_OPTION,
            })
        );

        let mut unknown = borrowed(&entries);
        unknown.push(("hostname", "example.invalid"));
        assert_eq!(
            Ssu2RouterAddress::from_option_entries("SSU2", &unknown),
            Err(Ssu2AddressError::UnknownOption)
        );
        assert_eq!(
            Ssu2RouterAddress::from_option_entries("SSU", &borrowed(&entries)),
            Err(Ssu2AddressError::UnsupportedTransportStyle)
        );
    }

    #[test]
    fn rejects_invalid_hosts_ports_keys_mtu_caps_and_introducers() {
        let mut entries = direct_entries();
        entries[0].1 = "router.example.invalid".to_owned();
        assert_eq!(parse(&entries), Err(Ssu2AddressError::HostnameNotAllowed));

        for invalid_port in ["", "0", "65536", "01234", "12x4"] {
            let mut entries = direct_entries();
            entries[1].1 = invalid_port.to_owned();
            assert!(
                matches!(
                    parse(&entries),
                    Err(Ssu2AddressError::InvalidPort | Ssu2AddressError::PortOutOfRange)
                ),
                "port {invalid_port:?}"
            );
        }

        let mut bad_key = direct_entries();
        bad_key[2].1 = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA====".to_owned();
        assert!(matches!(
            parse(&bad_key),
            Err(Ssu2AddressError::InvalidOptionValue {
                option: STATIC_KEY_OPTION
            })
        ));

        let mut zero_key = direct_entries();
        zero_key[2].1 = encode_i2p_base64(&[0; SSU2_STATIC_PUBLIC_KEY_LENGTH]);
        assert_eq!(
            parse(&zero_key),
            Err(Ssu2AddressError::InvalidStaticPublicKey)
        );

        let mut zero_intro = direct_entries();
        zero_intro[3].1 = encode_i2p_base64(&[0; SSU2_INTRO_KEY_LENGTH]);
        assert_eq!(parse(&zero_intro), Err(Ssu2AddressError::InvalidIntroKey));

        for bad_mtu in ["", "1279", "9001", "01280", "jumbo"] {
            let mut entries = direct_entries();
            entries[6].1 = bad_mtu.to_owned();
            assert_eq!(
                parse(&entries),
                Err(Ssu2AddressError::InvalidOptionValue { option: MTU_OPTION }),
                "mtu {bad_mtu:?}"
            );
        }

        let mut bad_caps = direct_entries();
        bad_caps[5].1 = "446".to_owned();
        assert_eq!(
            parse(&bad_caps),
            Err(Ssu2AddressError::InvalidOptionValue {
                option: CAPS_OPTION
            })
        );

        // Partial introducer group: missing itag0.
        let partial = vec![
            (STATIC_KEY_OPTION, encode_i2p_base64(&TEST_STATIC_KEY)),
            (INTRO_KEY_OPTION, encode_i2p_base64(&TEST_INTRO_KEY)),
            (VERSION_OPTION, "2".to_owned()),
            ("ihost0", "192.0.2.10".to_owned()),
            ("iport0", "23456".to_owned()),
            ("ikey0", encode_i2p_base64(&TEST_INTRO_KEY)),
        ];
        assert_eq!(
            parse(&partial),
            Err(Ssu2AddressError::MissingIntroducerField {
                option: ITAG_PREFIX,
                index: 0,
            })
        );

        // Zero relay tag is rejected.
        let mut zero_tag = direct_entries();
        zero_tag.push(("ihost0", "192.0.2.10".to_owned()));
        zero_tag.push(("iport0", "23456".to_owned()));
        zero_tag.push(("ikey0", encode_i2p_base64(&TEST_INTRO_KEY)));
        zero_tag.push(("itag0", "0".to_owned()));
        assert_eq!(
            parse(&zero_tag),
            Err(Ssu2AddressError::InvalidIntroducer { index: 0 })
        );

        // Index beyond the bound is rejected.
        let mut overflow = direct_entries();
        overflow.push(("ihost3", "192.0.2.10".to_owned()));
        overflow.push(("iport3", "23456".to_owned()));
        overflow.push(("ikey3", encode_i2p_base64(&TEST_INTRO_KEY)));
        overflow.push(("itag3", "7".to_owned()));
        assert_eq!(parse(&overflow), Err(Ssu2AddressError::TooManyIntroducers));
    }

    #[test]
    fn rejects_endpoint_mismatch_and_missing_intro_for_endpoint() {
        let entries = direct_entries();
        let parsed = parse(&entries).expect("direct");
        assert_eq!(
            parsed.resolved_dial_target("192.0.2.2:12345".parse().unwrap()),
            Err(Ssu2AddressError::EndpointMismatch)
        );

        let no_intro = vec![
            (HOST_OPTION, "192.0.2.1".to_owned()),
            (PORT_OPTION, "12345".to_owned()),
            (STATIC_KEY_OPTION, encode_i2p_base64(&TEST_STATIC_KEY)),
            (VERSION_OPTION, "2".to_owned()),
        ];
        assert_eq!(
            parse(&no_intro),
            Err(Ssu2AddressError::MissingOption {
                option: INTRO_KEY_OPTION
            })
        );
    }

    #[test]
    fn debug_redacts_endpoint_and_key_material() {
        let entries = direct_entries();
        let parsed = parse(&entries).expect("address");
        let debug = format!("{parsed:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("192.0.2.1"));
        assert!(!debug.contains("12345"));
        assert!(!format!("{:?}", parsed.configured_listen().unwrap()).contains("192.0.2.1"));
        assert!(
            !format!(
                "{:?}",
                parsed
                    .resolved_dial_target("192.0.2.1:12345".parse().unwrap())
                    .unwrap()
            )
            .contains("12345")
        );
    }
}
