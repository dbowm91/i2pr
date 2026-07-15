//! Strict, runtime-neutral NTCP2 RouterAddress validation.
//!
//! This module owns protocol address values, not name resolution, socket
//! creation, publication policy, or reachability claims. A RouterAddress may
//! contain only the static-key/version pair for an unpublished NTCP2 address;
//! the configured-listener and resolved-dial-target types require a complete
//! literal endpoint.

use std::{
    fmt,
    net::{IpAddr, SocketAddr},
    str::FromStr,
};

use i2pr_proto::{Date, RouterAddress};
use i2pr_transport::AddressFamily;
use thiserror::Error;

use crate::{constants, crypto::PublicKeyBytes};

/// The lowest TCP port accepted by the current NTCP address specification.
pub const NTCP2_MIN_PORT: u16 = 1;
/// The highest TCP port accepted by the current NTCP address specification.
pub const NTCP2_MAX_PORT: u16 = u16::MAX;
/// The exact binary length of an NTCP2 static public key.
pub const NTCP2_STATIC_PUBLIC_KEY_LENGTH: usize = constants::KEY_LENGTH;
/// The exact binary length of an NTCP2 AES obfuscation IV.
pub const NTCP2_OBFUSCATION_IV_LENGTH: usize = constants::AES_BLOCK_LENGTH;
/// The current NTCP2 RouterAddress version.
pub const NTCP2_ROUTER_ADDRESS_VERSION: u8 = 2;

const STATIC_KEY_OPTION: &str = "s";
const IV_OPTION: &str = "i";
const HOST_OPTION: &str = "host";
const PORT_OPTION: &str = "port";
const VERSION_OPTION: &str = "v";
const CAPS_OPTION: &str = "caps";

/// A bounded failure while parsing or constructing NTCP2 address data.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum Ntcp2AddressError {
    /// The RouterAddress transport style is not NTCP or NTCP2.
    #[error("unsupported NTCP2 RouterAddress transport style")]
    UnsupportedTransportStyle,
    /// An option key is not part of the current NTCP2 address vocabulary.
    #[error("unknown NTCP2 RouterAddress option")]
    UnknownOption,
    /// An option occurred more than once in an option-entry sequence.
    #[error("duplicate NTCP2 RouterAddress option {option}")]
    DuplicateOption {
        /// The fixed option category that was repeated.
        option: &'static str,
    },
    /// A required option is absent.
    #[error("missing NTCP2 RouterAddress option {option}")]
    MissingOption {
        /// The fixed option category that was absent.
        option: &'static str,
    },
    /// Two options have an invalid presence or value relationship.
    #[error("conflicting NTCP2 RouterAddress options {first} and {second}")]
    ConflictingOptions {
        /// The first fixed option category in the conflict.
        first: &'static str,
        /// The second fixed option category in the conflict.
        second: &'static str,
    },
    /// An option uses the right key but has a malformed value.
    #[error("invalid NTCP2 RouterAddress option {option}")]
    InvalidOptionValue {
        /// The fixed option category whose value was rejected.
        option: &'static str,
    },
    /// A host value was not a literal IPv4 or IPv6 address.
    #[error("NTCP2 RouterAddress host must be a literal IP address")]
    HostnameNotAllowed,
    /// A port was not a canonical decimal value.
    #[error("NTCP2 RouterAddress port is not a canonical decimal value")]
    InvalidPort,
    /// A port was outside the protocol's accepted range.
    #[error("NTCP2 RouterAddress port is outside 1..=65535")]
    PortOutOfRange,
    /// An endpoint-dependent operation was attempted without host and port.
    #[error("NTCP2 RouterAddress has no complete endpoint")]
    MissingEndpoint,
    /// A resolved endpoint did not match the parsed literal RouterAddress.
    #[error("resolved NTCP2 dial target does not match RouterAddress endpoint")]
    EndpointMismatch,
    /// The static public key was malformed or an invalid low-order value.
    #[error("invalid NTCP2 static public key")]
    InvalidStaticPublicKey,
}

/// Whether an address was published for NTCP or only NTCP2.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Ntcp2TransportStyle {
    /// The address supports the legacy NTCP/NTCP2 shared-port form.
    Ntcp,
    /// The address supports NTCP2 only.
    Ntcp2,
}

impl Ntcp2TransportStyle {
    /// Parses one of the exact published NTCP transport style identifiers.
    pub fn parse(value: &str) -> Result<Self, Ntcp2AddressError> {
        match value {
            "NTCP" => Ok(Self::Ntcp),
            "NTCP2" => Ok(Self::Ntcp2),
            _ => Err(Ntcp2AddressError::UnsupportedTransportStyle),
        }
    }

    /// Returns the exact RouterAddress transport style identifier.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ntcp => "NTCP",
            Self::Ntcp2 => "NTCP2",
        }
    }
}

/// The public NTCP2 obfuscation IV, with redacted default diagnostics.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct Ntcp2ObfuscationIv([u8; NTCP2_OBFUSCATION_IV_LENGTH]);

impl Ntcp2ObfuscationIv {
    /// Constructs an IV with the exact protocol width.
    pub const fn from_bytes(bytes: [u8; NTCP2_OBFUSCATION_IV_LENGTH]) -> Self {
        Self(bytes)
    }

    /// Borrows the exact bytes used by the NTCP2 AES obfuscation state.
    pub const fn as_bytes(&self) -> &[u8; NTCP2_OBFUSCATION_IV_LENGTH] {
        &self.0
    }
}

impl fmt::Debug for Ntcp2ObfuscationIv {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("Ntcp2ObfuscationIv")
            .field(&"<redacted>")
            .finish()
    }
}

/// The NTCP2 static public key and IV required by a complete endpoint.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct Ntcp2AddressMaterial {
    static_public_key: PublicKeyBytes,
    obfuscation_iv: Ntcp2ObfuscationIv,
}

impl Ntcp2AddressMaterial {
    /// Validates exact public-key and IV widths and constructs address
    /// material. The all-zero X25519 public encoding is rejected.
    pub fn from_bytes(
        static_public_key: [u8; NTCP2_STATIC_PUBLIC_KEY_LENGTH],
        obfuscation_iv: [u8; NTCP2_OBFUSCATION_IV_LENGTH],
    ) -> Result<Self, Ntcp2AddressError> {
        let static_public_key = PublicKeyBytes::new(static_public_key)
            .map_err(|_| Ntcp2AddressError::InvalidStaticPublicKey)?;
        Ok(Self::from_parts(
            static_public_key,
            Ntcp2ObfuscationIv::from_bytes(obfuscation_iv),
        ))
    }

    /// Constructs material from already validated protocol-specific values.
    pub const fn from_parts(
        static_public_key: PublicKeyBytes,
        obfuscation_iv: Ntcp2ObfuscationIv,
    ) -> Self {
        Self {
            static_public_key,
            obfuscation_iv,
        }
    }

    /// Returns the validated static public key.
    pub const fn static_public_key(&self) -> PublicKeyBytes {
        self.static_public_key
    }

    /// Returns the validated obfuscation IV.
    pub const fn obfuscation_iv(&self) -> &Ntcp2ObfuscationIv {
        &self.obfuscation_iv
    }
}

impl fmt::Debug for Ntcp2AddressMaterial {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Ntcp2AddressMaterial")
            .field("static_public_key", &"<redacted>")
            .field("obfuscation_iv", &"<redacted>")
            .finish()
    }
}

/// A literal IP endpoint used by the runtime-neutral listen and dial types.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct Ntcp2Endpoint {
    ip: IpAddr,
    port: u16,
}

impl Ntcp2Endpoint {
    /// Constructs an endpoint after enforcing the NTCP port range.
    pub fn new(ip: IpAddr, port: u16) -> Result<Self, Ntcp2AddressError> {
        validate_port(port)?;
        Ok(Self { ip, port })
    }

    /// Constructs an endpoint from a resolved standard-library address value.
    pub fn from_socket_addr(address: SocketAddr) -> Result<Self, Ntcp2AddressError> {
        Self::new(address.ip(), address.port())
    }

    /// Returns the literal IP address.
    pub const fn ip(self) -> IpAddr {
        self.ip
    }

    /// Returns the validated TCP port.
    pub const fn port(self) -> u16 {
        self.port
    }

    /// Returns the address-family classification without exposing an endpoint
    /// in a transport snapshot.
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

impl fmt::Debug for Ntcp2Endpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Ntcp2Endpoint")
            .field("family", &self.family())
            .field("endpoint", &"<redacted>")
            .finish()
    }
}

/// A strictly parsed NTCP2-capability RouterAddress.
#[derive(Clone, Eq, PartialEq)]
pub struct Ntcp2RouterAddress {
    cost: u8,
    expiration: Date,
    transport_style: Ntcp2TransportStyle,
    endpoint: Option<Ntcp2Endpoint>,
    static_public_key: PublicKeyBytes,
    obfuscation_iv: Option<Ntcp2ObfuscationIv>,
    capabilities: Ntcp2Capabilities,
}

impl Ntcp2RouterAddress {
    /// Parses a structural RouterAddress and strictly validates its NTCP2
    /// options. The static-key-only unpublished form is accepted; use
    /// [`Self::configured_listen`] or [`Self::resolved_dial_target`] when an
    /// actual endpoint is required.
    pub fn parse(address: &RouterAddress) -> Result<Self, Ntcp2AddressError> {
        let transport_style = Ntcp2TransportStyle::parse(address.transport_style())?;
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
    pub fn from_router_address(address: &RouterAddress) -> Result<Self, Ntcp2AddressError> {
        Self::parse(address)
    }

    /// Parses raw option entries for callers that need duplicate detection
    /// before constructing the shared canonical Mapping type.
    ///
    /// This helper uses cost zero and an undefined expiration because it is a
    /// pure option-validation entry point. Complete RouterAddress values
    /// should be passed to [`Self::parse`].
    pub fn from_option_entries(
        transport_style: &str,
        entries: &[(&str, &str)],
    ) -> Result<Self, Ntcp2AddressError> {
        let transport_style = Ntcp2TransportStyle::parse(transport_style)?;
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

    /// Returns the exact NTCP or NTCP2 transport style.
    pub const fn transport_style(&self) -> Ntcp2TransportStyle {
        self.transport_style
    }

    /// Returns the supported current NTCP2 RouterAddress version.
    pub const fn version(&self) -> u8 {
        NTCP2_ROUTER_ADDRESS_VERSION
    }

    /// Returns the literal endpoint, if this is a published address.
    pub const fn endpoint(&self) -> Option<Ntcp2Endpoint> {
        self.endpoint
    }

    /// Returns the literal host, if present.
    pub fn host(&self) -> Option<IpAddr> {
        self.endpoint.map(Ntcp2Endpoint::ip)
    }

    /// Returns the validated TCP port, if present.
    pub fn port(&self) -> Option<u16> {
        self.endpoint.map(Ntcp2Endpoint::port)
    }

    /// Returns the IPv4/IPv6 classification, if an endpoint is present.
    pub fn family(&self) -> Option<AddressFamily> {
        self.endpoint.map(Ntcp2Endpoint::family)
    }

    /// Returns the validated static public key.
    pub const fn static_public_key(&self) -> PublicKeyBytes {
        self.static_public_key
    }

    /// Returns the IV for a published endpoint, or `None` for the permitted
    /// unpublished static-key-only form.
    pub fn obfuscation_iv(&self) -> Option<&Ntcp2ObfuscationIv> {
        self.obfuscation_iv.as_ref()
    }

    /// Returns the validated capability flags.
    pub const fn capabilities(&self) -> Ntcp2Capabilities {
        self.capabilities
    }

    /// Converts this address into a configured literal listener.
    pub fn configured_listen(&self) -> Result<ConfiguredListenAddress, Ntcp2AddressError> {
        let endpoint = self.endpoint.ok_or(Ntcp2AddressError::MissingEndpoint)?;
        let iv = self
            .obfuscation_iv
            .ok_or(Ntcp2AddressError::MissingOption { option: IV_OPTION })?;
        ConfiguredListenAddress::new(
            endpoint,
            Ntcp2AddressMaterial::from_parts(self.static_public_key, iv),
        )
    }

    /// Converts this address into a resolved dial target after checking that
    /// the supplied endpoint exactly matches its literal host and port.
    pub fn resolved_dial_target(
        &self,
        resolved: SocketAddr,
    ) -> Result<ResolvedDialTarget, Ntcp2AddressError> {
        let endpoint = self.endpoint.ok_or(Ntcp2AddressError::MissingEndpoint)?;
        let resolved = Ntcp2Endpoint::from_socket_addr(resolved)?;
        if endpoint != resolved {
            return Err(Ntcp2AddressError::EndpointMismatch);
        }
        let iv = self
            .obfuscation_iv
            .ok_or(Ntcp2AddressError::MissingOption { option: IV_OPTION })?;
        ResolvedDialTarget::new(
            resolved,
            Ntcp2AddressMaterial::from_parts(self.static_public_key, iv),
        )
    }

    fn from_parsed(
        cost: u8,
        expiration: Date,
        transport_style: Ntcp2TransportStyle,
        options: ParsedOptions<'_>,
    ) -> Result<Self, Ntcp2AddressError> {
        let static_public_key = decode_static_public_key(options.static_public_key.ok_or(
            Ntcp2AddressError::MissingOption {
                option: STATIC_KEY_OPTION,
            },
        )?)?;
        let version = options.version.ok_or(Ntcp2AddressError::MissingOption {
            option: VERSION_OPTION,
        })?;
        if version != "2" {
            return Err(Ntcp2AddressError::InvalidOptionValue {
                option: VERSION_OPTION,
            });
        }

        let endpoint = match (options.host, options.port) {
            (None, None) => None,
            (Some(_), None) | (None, Some(_)) => {
                return Err(Ntcp2AddressError::ConflictingOptions {
                    first: HOST_OPTION,
                    second: PORT_OPTION,
                });
            }
            (Some(host), Some(port)) => Some(parse_endpoint(host, port)?),
        };

        let obfuscation_iv = match (endpoint, options.obfuscation_iv) {
            (None, None) => None,
            (None, Some(_)) => {
                return Err(Ntcp2AddressError::ConflictingOptions {
                    first: IV_OPTION,
                    second: HOST_OPTION,
                });
            }
            (Some(_), None) => {
                return Err(Ntcp2AddressError::MissingOption { option: IV_OPTION });
            }
            (Some(_), Some(value)) => Some(decode_obfuscation_iv(value)?),
        };

        Ok(Self {
            cost,
            expiration,
            transport_style,
            endpoint,
            static_public_key,
            obfuscation_iv,
            capabilities: options
                .capabilities
                .map(parse_capabilities)
                .transpose()?
                .unwrap_or_else(Ntcp2Capabilities::empty),
        })
    }
}

impl fmt::Debug for Ntcp2RouterAddress {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Ntcp2RouterAddress")
            .field("cost", &self.cost)
            .field("expiration", &self.expiration)
            .field("transport_style", &self.transport_style)
            .field("family", &self.family())
            .field("endpoint", &"<redacted>")
            .field("static_public_key", &"<redacted>")
            .field("obfuscation_iv", &"<redacted>")
            .field("version", &self.version())
            .field("capabilities", &self.capabilities)
            .finish()
    }
}

impl TryFrom<&RouterAddress> for Ntcp2RouterAddress {
    type Error = Ntcp2AddressError;

    fn try_from(address: &RouterAddress) -> Result<Self, Self::Error> {
        Self::parse(address)
    }
}

impl TryFrom<RouterAddress> for Ntcp2RouterAddress {
    type Error = Ntcp2AddressError;

    fn try_from(address: RouterAddress) -> Result<Self, Self::Error> {
        Self::parse(&address)
    }
}

/// Capabilities carried by the optional NTCP2 RouterAddress `caps` option.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Ntcp2Capabilities(u8);

impl Ntcp2Capabilities {
    const IPV4: u8 = 0b01;
    const IPV6: u8 = 0b10;

    /// Returns no advertised outbound-family capabilities.
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Returns whether outbound IPv4 capability was advertised.
    pub const fn supports_ipv4(self) -> bool {
        self.0 & Self::IPV4 != 0
    }

    /// Returns whether outbound IPv6 capability was advertised.
    pub const fn supports_ipv6(self) -> bool {
        self.0 & Self::IPV6 != 0
    }

    /// Returns the two-bit capability representation used by this parser.
    pub const fn bits(self) -> u8 {
        self.0
    }
}

/// A configured literal NTCP2 listen address.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ConfiguredListenAddress {
    endpoint: Ntcp2Endpoint,
    material: Ntcp2AddressMaterial,
}

impl ConfiguredListenAddress {
    /// Constructs a listener endpoint without opening or binding a socket.
    pub fn new(
        endpoint: Ntcp2Endpoint,
        material: Ntcp2AddressMaterial,
    ) -> Result<Self, Ntcp2AddressError> {
        validate_port(endpoint.port())?;
        Ok(Self { endpoint, material })
    }

    /// Constructs a listener endpoint from exact raw key and IV material.
    pub fn from_parts(
        ip: IpAddr,
        port: u16,
        static_public_key: [u8; NTCP2_STATIC_PUBLIC_KEY_LENGTH],
        obfuscation_iv: [u8; NTCP2_OBFUSCATION_IV_LENGTH],
    ) -> Result<Self, Ntcp2AddressError> {
        Self::new(
            Ntcp2Endpoint::new(ip, port)?,
            Ntcp2AddressMaterial::from_bytes(static_public_key, obfuscation_iv)?,
        )
    }

    /// Constructs a listener from a parsed RouterAddress.
    pub fn from_router_address(address: &Ntcp2RouterAddress) -> Result<Self, Ntcp2AddressError> {
        address.configured_listen()
    }

    /// Returns the literal endpoint without any socket ownership.
    pub const fn endpoint(self) -> Ntcp2Endpoint {
        self.endpoint
    }

    /// Returns the literal IP address.
    pub const fn ip(self) -> IpAddr {
        self.endpoint.ip()
    }

    /// Returns the validated TCP port.
    pub const fn port(self) -> u16 {
        self.endpoint.port()
    }

    /// Returns the IPv4/IPv6 classification.
    pub const fn family(self) -> AddressFamily {
        self.endpoint.family()
    }

    /// Returns the local static public key.
    pub const fn static_public_key(self) -> PublicKeyBytes {
        self.material.static_public_key()
    }

    /// Returns the local published obfuscation IV.
    pub fn obfuscation_iv(&self) -> &Ntcp2ObfuscationIv {
        self.material.obfuscation_iv()
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

/// A resolved NTCP2 dial target. Resolution is supplied by the caller; this
/// type performs no DNS lookup and owns no socket or runtime resource.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ResolvedDialTarget {
    endpoint: Ntcp2Endpoint,
    expected_peer_material: Ntcp2AddressMaterial,
}

impl ResolvedDialTarget {
    /// Constructs a resolved dial target from a validated endpoint and the
    /// RouterAddress material expected from the peer.
    pub fn new(
        endpoint: Ntcp2Endpoint,
        expected_peer_material: Ntcp2AddressMaterial,
    ) -> Result<Self, Ntcp2AddressError> {
        validate_port(endpoint.port())?;
        Ok(Self {
            endpoint,
            expected_peer_material,
        })
    }

    /// Constructs a resolved target from exact raw key and IV material.
    pub fn from_parts(
        address: SocketAddr,
        static_public_key: [u8; NTCP2_STATIC_PUBLIC_KEY_LENGTH],
        obfuscation_iv: [u8; NTCP2_OBFUSCATION_IV_LENGTH],
    ) -> Result<Self, Ntcp2AddressError> {
        Self::new(
            Ntcp2Endpoint::from_socket_addr(address)?,
            Ntcp2AddressMaterial::from_bytes(static_public_key, obfuscation_iv)?,
        )
    }

    /// Constructs a resolved target from a parsed RouterAddress and a caller-
    /// supplied resolved endpoint, requiring an exact literal match.
    pub fn from_router_address(
        address: &Ntcp2RouterAddress,
        resolved: SocketAddr,
    ) -> Result<Self, Ntcp2AddressError> {
        address.resolved_dial_target(resolved)
    }

    /// Returns the resolved endpoint value without socket ownership.
    pub const fn endpoint(self) -> Ntcp2Endpoint {
        self.endpoint
    }

    /// Returns the resolved IP address.
    pub const fn ip(self) -> IpAddr {
        self.endpoint.ip()
    }

    /// Returns the resolved TCP port.
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
    pub const fn expected_static_public_key(self) -> PublicKeyBytes {
        self.expected_peer_material.static_public_key()
    }

    /// Returns the expected peer obfuscation IV.
    pub fn obfuscation_iv(&self) -> &Ntcp2ObfuscationIv {
        self.expected_peer_material.obfuscation_iv()
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
    obfuscation_iv: Option<&'a str>,
    version: Option<&'a str>,
    capabilities: Option<&'a str>,
}

impl<'a> ParsedOptions<'a> {
    fn from_entries<I>(entries: I) -> Result<Self, Ntcp2AddressError>
    where
        I: IntoIterator<Item = (&'a str, &'a str)>,
    {
        let mut parsed = Self::default();
        for (key, value) in entries {
            match key {
                HOST_OPTION => store(&mut parsed.host, value, HOST_OPTION)?,
                PORT_OPTION => store(&mut parsed.port, value, PORT_OPTION)?,
                STATIC_KEY_OPTION => {
                    store(&mut parsed.static_public_key, value, STATIC_KEY_OPTION)?
                }
                IV_OPTION => store(&mut parsed.obfuscation_iv, value, IV_OPTION)?,
                VERSION_OPTION => store(&mut parsed.version, value, VERSION_OPTION)?,
                CAPS_OPTION => store(&mut parsed.capabilities, value, CAPS_OPTION)?,
                _ => return Err(Ntcp2AddressError::UnknownOption),
            }
        }
        Ok(parsed)
    }
}

fn store<'a>(
    slot: &mut Option<&'a str>,
    value: &'a str,
    option: &'static str,
) -> Result<(), Ntcp2AddressError> {
    if slot.replace(value).is_some() {
        return Err(Ntcp2AddressError::DuplicateOption { option });
    }
    Ok(())
}

fn parse_endpoint(host: &str, port: &str) -> Result<Ntcp2Endpoint, Ntcp2AddressError> {
    let ip = IpAddr::from_str(host).map_err(|_| Ntcp2AddressError::HostnameNotAllowed)?;
    let port = parse_port(port)?;
    Ntcp2Endpoint::new(ip, port)
}

fn parse_port(value: &str) -> Result<u16, Ntcp2AddressError> {
    if value.is_empty()
        || value.len() > 5
        || value.len() > 1 && value.starts_with('0')
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(Ntcp2AddressError::InvalidPort);
    }
    let port = value
        .parse::<u32>()
        .map_err(|_| Ntcp2AddressError::InvalidPort)?;
    let port = u16::try_from(port).map_err(|_| Ntcp2AddressError::PortOutOfRange)?;
    validate_port(port)?;
    Ok(port)
}

fn validate_port(port: u16) -> Result<(), Ntcp2AddressError> {
    if !(NTCP2_MIN_PORT..=NTCP2_MAX_PORT).contains(&port) {
        return Err(Ntcp2AddressError::PortOutOfRange);
    }
    Ok(())
}

fn decode_static_public_key(value: &str) -> Result<PublicKeyBytes, Ntcp2AddressError> {
    let bytes = decode_i2p_base64::<NTCP2_STATIC_PUBLIC_KEY_LENGTH>(value, STATIC_KEY_OPTION)?;
    PublicKeyBytes::new(bytes).map_err(|_| Ntcp2AddressError::InvalidStaticPublicKey)
}

fn decode_obfuscation_iv(value: &str) -> Result<Ntcp2ObfuscationIv, Ntcp2AddressError> {
    Ok(Ntcp2ObfuscationIv::from_bytes(decode_i2p_base64::<
        NTCP2_OBFUSCATION_IV_LENGTH,
    >(value, IV_OPTION)?))
}

fn decode_i2p_base64<const N: usize>(
    value: &str,
    option: &'static str,
) -> Result<[u8; N], Ntcp2AddressError> {
    let expected_length = 4 * N.div_ceil(3);
    let padding = match N % 3 {
        0 => 0,
        1 => 2,
        _ => 1,
    };
    if value.len() != expected_length {
        return Err(Ntcp2AddressError::InvalidOptionValue { option });
    }
    let bytes = value.as_bytes();
    let data_length = expected_length - padding;
    if bytes[data_length..].iter().any(|byte| *byte != b'=') || bytes[..data_length].contains(&b'=')
    {
        return Err(Ntcp2AddressError::InvalidOptionValue { option });
    }

    let mut output = [0_u8; N];
    let mut output_index = 0;
    let mut accumulator = 0_u16;
    let mut bits = 0_u8;
    for byte in &bytes[..data_length] {
        let digit =
            i2p_base64_digit(*byte).ok_or(Ntcp2AddressError::InvalidOptionValue { option })?;
        accumulator = (accumulator << 6) | u16::from(digit);
        bits += 6;
        while bits >= 8 {
            bits -= 8;
            if output_index == N {
                return Err(Ntcp2AddressError::InvalidOptionValue { option });
            }
            output[output_index] = ((accumulator >> bits) & 0xff) as u8;
            output_index += 1;
            accumulator &= if bits == 0 { 0 } else { (1_u16 << bits) - 1 };
        }
    }
    if output_index != N || (bits > 0 && accumulator != 0) {
        return Err(Ntcp2AddressError::InvalidOptionValue { option });
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

fn parse_capabilities(value: &str) -> Result<Ntcp2Capabilities, Ntcp2AddressError> {
    if value.is_empty() {
        return Err(Ntcp2AddressError::InvalidOptionValue {
            option: CAPS_OPTION,
        });
    }
    let mut bits = 0;
    for byte in value.bytes() {
        let flag = match byte {
            b'4' => Ntcp2Capabilities::IPV4,
            b'6' => Ntcp2Capabilities::IPV6,
            _ => {
                return Err(Ntcp2AddressError::InvalidOptionValue {
                    option: CAPS_OPTION,
                });
            }
        };
        if bits & flag != 0 {
            return Err(Ntcp2AddressError::InvalidOptionValue {
                option: CAPS_OPTION,
            });
        }
        bits |= flag;
    }
    Ok(Ntcp2Capabilities(bits))
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_proto::Mapping;

    const KEY: [u8; NTCP2_STATIC_PUBLIC_KEY_LENGTH] = [0x42; NTCP2_STATIC_PUBLIC_KEY_LENGTH];
    const IV: [u8; NTCP2_OBFUSCATION_IV_LENGTH] = [0x24; NTCP2_OBFUSCATION_IV_LENGTH];

    fn encode_i2p_base64(bytes: &[u8]) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-~";
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

    fn address(style: &str, entries: &[(&str, &str)]) -> RouterAddress {
        let options = Mapping::from_entries(
            entries
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect(),
        )
        .expect("canonical options");
        RouterAddress::new(10, Date::from_millis(0), style.to_owned(), options)
            .expect("RouterAddress")
    }

    fn complete_entries() -> Vec<(&'static str, String)> {
        vec![
            (HOST_OPTION, "192.0.2.1".to_owned()),
            (PORT_OPTION, "12345".to_owned()),
            (STATIC_KEY_OPTION, encode_i2p_base64(&KEY)),
            (IV_OPTION, encode_i2p_base64(&IV)),
            (VERSION_OPTION, "2".to_owned()),
            (CAPS_OPTION, "64".to_owned()),
        ]
    }

    fn borrowed<'a>(entries: &'a [(&'a str, String)]) -> Vec<(&'a str, &'a str)> {
        entries
            .iter()
            .map(|(key, value)| (*key, value.as_str()))
            .collect()
    }

    #[test]
    fn parses_complete_ipv4_and_ipv6_addresses_and_material() {
        let entries = complete_entries();
        let parsed = Ntcp2RouterAddress::from_option_entries("NTCP2", &borrowed(&entries))
            .expect("IPv4 address");
        assert_eq!(parsed.host(), Some("192.0.2.1".parse().unwrap()));
        assert_eq!(parsed.port(), Some(12345));
        assert_eq!(parsed.family(), Some(AddressFamily::Ipv4));
        assert_eq!(parsed.static_public_key().as_bytes(), &KEY);
        assert_eq!(parsed.obfuscation_iv().unwrap().as_bytes(), &IV);
        assert!(parsed.capabilities().supports_ipv4());
        assert!(parsed.capabilities().supports_ipv6());

        let mut ipv6 = entries;
        ipv6[0].1 = "2001:db8::1".to_owned();
        let parsed = Ntcp2RouterAddress::from_option_entries("NTCP", &borrowed(&ipv6))
            .expect("IPv6 address");
        assert_eq!(parsed.family(), Some(AddressFamily::Ipv6));
        assert_eq!(parsed.transport_style(), Ntcp2TransportStyle::Ntcp);
    }

    #[test]
    fn parses_structural_router_address_and_preserves_cost_and_expiration() {
        let entries = complete_entries();
        let pairs = entries
            .iter()
            .map(|(key, value)| (*key, value.as_str()))
            .collect::<Vec<_>>();
        let router_address = address("NTCP2", &pairs);
        let parsed = Ntcp2RouterAddress::parse(&router_address).expect("parsed address");
        assert_eq!(parsed.cost(), 10);
        assert_eq!(parsed.expiration(), Date::from_millis(0));
        assert_eq!(parsed.endpoint().unwrap().socket_addr().port(), 12345);
    }

    #[test]
    fn static_key_only_address_is_not_a_listen_or_dial_endpoint() {
        let entries = vec![
            (STATIC_KEY_OPTION, encode_i2p_base64(&KEY)),
            (VERSION_OPTION, "2".to_owned()),
        ];
        let parsed = Ntcp2RouterAddress::from_option_entries("NTCP2", &borrowed(&entries))
            .expect("unpublished address");
        assert_eq!(parsed.endpoint(), None);
        assert_eq!(parsed.obfuscation_iv(), None);
        assert_eq!(
            parsed.configured_listen(),
            Err(Ntcp2AddressError::MissingEndpoint)
        );
        assert_eq!(
            parsed.resolved_dial_target("192.0.2.1:12345".parse().unwrap()),
            Err(Ntcp2AddressError::MissingEndpoint)
        );
    }

    #[test]
    fn configured_and_resolved_types_are_distinct_and_runtime_neutral() {
        let material = Ntcp2AddressMaterial::from_bytes(KEY, IV).expect("material");
        let endpoint = Ntcp2Endpoint::new("127.0.0.1".parse().unwrap(), 12345).expect("endpoint");
        let listen = ConfiguredListenAddress::new(endpoint, material).expect("listener");
        let dial = ResolvedDialTarget::new(endpoint, material).expect("dial target");
        assert_eq!(listen.family(), AddressFamily::Ipv4);
        assert_eq!(
            dial.socket_addr().ip(),
            "127.0.0.1".parse::<IpAddr>().unwrap()
        );
        assert_eq!(dial.expected_static_public_key().as_bytes(), &KEY);
    }

    #[test]
    fn rejects_duplicate_conflicting_and_unknown_options() {
        let entries = complete_entries();
        let duplicate = vec![
            (HOST_OPTION, entries[0].1.as_str()),
            (HOST_OPTION, "192.0.2.2"),
        ];
        assert_eq!(
            Ntcp2RouterAddress::from_option_entries("NTCP2", &duplicate),
            Err(Ntcp2AddressError::DuplicateOption {
                option: HOST_OPTION
            })
        );

        let missing_port = vec![
            (HOST_OPTION, "192.0.2.1"),
            (STATIC_KEY_OPTION, entries[2].1.as_str()),
            (VERSION_OPTION, "2"),
        ];
        assert_eq!(
            Ntcp2RouterAddress::from_option_entries("NTCP2", &missing_port),
            Err(Ntcp2AddressError::ConflictingOptions {
                first: HOST_OPTION,
                second: PORT_OPTION,
            })
        );

        let mut unknown = entries
            .iter()
            .map(|(key, value)| (*key, value.as_str()))
            .collect::<Vec<_>>();
        unknown.push(("hostname", "example.invalid"));
        assert_eq!(
            Ntcp2RouterAddress::from_option_entries("NTCP2", &unknown),
            Err(Ntcp2AddressError::UnknownOption)
        );
    }

    #[test]
    fn rejects_invalid_hosts_ports_material_and_encodings() {
        let mut entries = complete_entries();
        entries[0].1 = "router.example.invalid".to_owned();
        assert_eq!(
            Ntcp2RouterAddress::from_option_entries("NTCP2", &borrowed(&entries)),
            Err(Ntcp2AddressError::HostnameNotAllowed)
        );

        for invalid_port in ["", "0", "65536", "01234", "12x4"] {
            let mut entries = complete_entries();
            entries[1].1 = invalid_port.to_owned();
            assert!(
                matches!(
                    Ntcp2RouterAddress::from_option_entries("NTCP2", &borrowed(&entries)),
                    Err(Ntcp2AddressError::InvalidPort | Ntcp2AddressError::PortOutOfRange)
                ),
                "port {invalid_port:?}"
            );
        }

        let mut bad_key = complete_entries();
        bad_key[2].1 = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA====".to_owned();
        assert!(matches!(
            Ntcp2RouterAddress::from_option_entries("NTCP2", &borrowed(&bad_key)),
            Err(Ntcp2AddressError::InvalidOptionValue {
                option: STATIC_KEY_OPTION
            })
        ));

        let mut bad_iv = complete_entries();
        bad_iv[3].1 = encode_i2p_base64(&[0x24; 15]);
        assert!(matches!(
            Ntcp2RouterAddress::from_option_entries("NTCP2", &borrowed(&bad_iv)),
            Err(Ntcp2AddressError::InvalidOptionValue { option: IV_OPTION })
        ));

        let mut zero_key = complete_entries();
        zero_key[2].1 = encode_i2p_base64(&[0; NTCP2_STATIC_PUBLIC_KEY_LENGTH]);
        assert_eq!(
            Ntcp2RouterAddress::from_option_entries("NTCP2", &borrowed(&zero_key)),
            Err(Ntcp2AddressError::InvalidStaticPublicKey)
        );
    }

    #[test]
    fn rejects_non_i2p_alphabet_noncanonical_padding_and_bad_versions() {
        let mut standard_alphabet = complete_entries();
        standard_alphabet[2].1 = encode_i2p_base64(&[0xfb; NTCP2_STATIC_PUBLIC_KEY_LENGTH]);
        standard_alphabet[2].1 = standard_alphabet[2].1.replace('~', "/");
        assert!(matches!(
            Ntcp2RouterAddress::from_option_entries("NTCP2", &borrowed(&standard_alphabet)),
            Err(Ntcp2AddressError::InvalidOptionValue {
                option: STATIC_KEY_OPTION
            })
        ));

        let mut unpadded = complete_entries();
        unpadded[2].1.pop();
        assert!(matches!(
            Ntcp2RouterAddress::from_option_entries("NTCP2", &borrowed(&unpadded)),
            Err(Ntcp2AddressError::InvalidOptionValue {
                option: STATIC_KEY_OPTION
            })
        ));

        for version in ["", "1", "2,3", "02"] {
            let mut entries = complete_entries();
            entries[4].1 = version.to_owned();
            assert_eq!(
                Ntcp2RouterAddress::from_option_entries("NTCP2", &borrowed(&entries)),
                Err(Ntcp2AddressError::InvalidOptionValue {
                    option: VERSION_OPTION
                }),
                "version {version:?}"
            );
        }
    }

    #[test]
    fn rejects_invalid_caps_and_endpoint_mismatch() {
        let mut entries = complete_entries();
        entries[5].1 = "44".to_owned();
        assert!(matches!(
            Ntcp2RouterAddress::from_option_entries("NTCP2", &borrowed(&entries)),
            Err(Ntcp2AddressError::InvalidOptionValue {
                option: CAPS_OPTION
            })
        ));
        entries[5].1 = "7".to_owned();
        assert!(matches!(
            Ntcp2RouterAddress::from_option_entries("NTCP2", &borrowed(&entries)),
            Err(Ntcp2AddressError::InvalidOptionValue {
                option: CAPS_OPTION
            })
        ));

        entries[5].1 = "46".to_owned();
        let parsed = Ntcp2RouterAddress::from_option_entries("NTCP2", &borrowed(&entries))
            .expect("complete address");
        assert_eq!(
            parsed.resolved_dial_target("192.0.2.2:12345".parse().unwrap()),
            Err(Ntcp2AddressError::EndpointMismatch)
        );
    }

    #[test]
    fn debug_redacts_endpoint_and_material() {
        let entries = complete_entries();
        let parsed =
            Ntcp2RouterAddress::from_option_entries("NTCP2", &borrowed(&entries)).expect("address");
        let debug = format!("{parsed:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("192.0.2.1"));
        assert!(!debug.contains("12345"));
        assert!(!debug.contains("42"));
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
