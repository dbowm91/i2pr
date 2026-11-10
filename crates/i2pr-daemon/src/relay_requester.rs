//! Plan 531 — the router reaches a peer through the peer's introducer.
//!
//! The tree implements the SSU2 relay protocol's **introducer** side and
//! the **target's** tag acquisition, but the **requester** side is
//! unwired. The transport's requester state machine, the runtime's
//! relay-request seam and the runtime's hole-punch handler all exist and
//! all had no production caller, so a router could not reach a peer that
//! is only reachable through an introducer — and the transport's
//! contradicting *relay-firewalled* class had no producer at all.
//!
//! This module owns the **policy** half of the requester role. It:
//!
//! - refuses every request while the operator's opt-in is off;
//! - examines a **bounded**, rotating window of the peers this router
//!   holds a session with, and never names a peer it has no session with;
//! - reads the candidate's **published** introducer set out of its
//!   **validated** RouterInfo, and never parses unvalidated bytes;
//! - resolves the named introducer **by hash**, from the same validated
//!   store, and never reaches outside it;
//! - draws a **nonzero** nonce that is never reused while the request is
//!   live;
//! - signs the specification's RelayRequest preimage with the router's
//!   **own** key;
//! - and counts every outcome under its own reason, so a reader observes
//!   what happened instead of inferring it.
//!
//! The owner holds **no** task, **no** timer, **no** listener and **no**
//! socket. The daemon's product tick drives it, exactly as it drives
//! Plan 524's own relay-tag request, and the runtime owns the sockets,
//! the session, the bounded tables and the transport's verification.
//!
//! # What a completed exchange means
//!
//! A completed exchange is **not** a corroboration. The transport's
//! `RelayFirewalledSignal` is corroboration class 4 and **contradicts**
//! reachability: a router that reaches a peer only through a relay is
//! not directly reachable. The owner therefore records the fact through
//! the runtime's own recorder and never through a peer profile. A local
//! condition is never written into a peer's profile.
//!
//! # What this module never does
//!
//! - It never advertises anything. The requester is opt-in behind
//!   `ssu2.relay_requester` and stays non-advertised.
//! - It never logs a peer hash, an address, a tag value, a key or a
//!   payload. The counters carry counts only.
//! - It never holds an unbounded table. Every table has a fixed capacity
//!   and evicts deterministically.

use std::collections::{BTreeMap, BTreeSet};

use i2pr_transport::{AddressFamily, PeerId};
use rand_core::RngCore;

/// The maximum number of peers one cadence window examines.
///
/// The window is a **bound**, not a target: a router with more peers than
/// this examines the rest over the following windows, so the work one
/// window does never grows with the router's peer population.
pub const MAX_RELAY_REQUEST_CANDIDATES: usize = 16;

/// The cadence between two relay requests, in milliseconds.
///
/// One request leaves the router per window at most, so a router that
/// always finds an eligible target asks at most once a minute.
pub const RELAY_REQUEST_INTERVAL_MS: u64 = 60_000;

/// The cadence after a window that asked nobody, in milliseconds.
///
/// A window that found no eligible target is retried sooner, so a router
/// that learns of a published introducer set does not wait a full
/// interval for it.
pub const RELAY_REQUEST_RETRY_MS: u64 = 15_000;

/// The maximum number of live relay requests this router holds.
///
/// The transport's own requester table has its own global and per-peer
/// ceilings; this is the daemon's own view of the same requests, and it
/// is bounded independently.
pub const MAX_LIVE_RELAY_REQUESTS: usize = 8;

/// How long a live relay request stays in the owner's table, in
/// milliseconds.
///
/// The transport's requester table expires its own entries on the same
/// scale (`RELAY_REQUEST_TIMEOUT_MS`); the owner's table is the daemon's
/// view of the same requests and must not outlive them.
pub const RELAY_REQUEST_LIFETIME_MS: u64 = 10_000;

/// The margin an introducer entry's own expiry is pulled back by.
///
/// A published entry whose expiry is within this margin is not usable:
/// the tag would be at the end of its life before the peers that read it
/// could use it.
pub const PUBLISHED_INTRODUCER_EXPIRY_MARGIN_SECS: u64 = 600;

/// The maximum number of hole punches the owner holds while it waits for
/// their accept (Plan 533).
///
/// A hole punch travels **one** hop and the accept travels **two**, so on
/// loopback the punch reaches the requester's handoff first and the
/// transport refuses it until the accept is correlated. The owner holds
/// the punch instead of losing it, and the hold is bounded: a router
/// that is flooded with early punches refuses them under their own
/// reason rather than growing a table.
pub const MAX_DEFERRED_PUNCHES: usize = 8;

/// The maximum number of establishments the owner holds for its task
/// (Plan 533).
///
/// The owner publishes one establishment per completed exchange, and the
/// task takes them. The bound is the owner's own view and is enforced
/// independently of the handoff's own capacity.
pub const MAX_PENDING_ESTABLISHMENTS: usize = 8;

/// The number of attempts a nonce draw makes before it gives up.
///
/// The draw is bounded: a router whose entropy source cannot produce a
/// fresh nonzero nonce refuses rather than reuse one.
pub const MAX_NONCE_ATTEMPTS: usize = 8;

/// One published introducer entry (`ihN`/`iexpN`/`itagN`).
///
/// The tuple is the introducer's router hash, its expiry and its relay
/// tag. The SSU 1 tuple — a host, a port and an introduction key — is
/// not used for SSU2: a peer resolves an introducer by hash through the
/// network database.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct PublishedIntroducer {
    /// The introducer's router hash (`ihN`).
    pub peer_hash: [u8; 32],
    /// The nonzero relay tag (`itagN`).
    pub relay_tag: u32,
    /// The entry's expiry in seconds since the epoch (`iexpN`). Zero
    /// means the expiry was not published.
    pub expires_secs: u64,
}

impl std::fmt::Debug for PublishedIntroducer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PublishedIntroducer")
            .field("peer_hash", &"<redacted>")
            .field("relay_tag", &"<redacted>")
            .field("expires_secs", &"<redacted>")
            .finish()
    }
}

/// The published introducer set of one peer, read from its validated
/// RouterInfo.
///
/// Every refusal is a distinct variant: a peer whose published set
/// cannot be read is **not** a target, and never a peer that is merely
/// unknown.
#[derive(Clone, Eq, PartialEq)]
pub enum PublishedIntroducerSet {
    /// The peer has no validated record in the authoritative store.
    NoStoredRecord,
    /// The peer's validated record carries no SSU2 address.
    NoSsu2Address,
    /// The peer's SSU2 address did not parse under the transport's
    /// strict parser.
    UnparsableAddress,
    /// The peer's SSU2 address parses but publishes no introducer set.
    NoIntroducers,
    /// The peer's SSU2 address publishes a bounded introducer set.
    Published(Vec<PublishedIntroducer>),
}

impl std::fmt::Debug for PublishedIntroducerSet {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoStoredRecord => formatter.write_str("NoStoredRecord"),
            Self::NoSsu2Address => formatter.write_str("NoSsu2Address"),
            Self::UnparsableAddress => formatter.write_str("UnparsableAddress"),
            Self::NoIntroducers => formatter.write_str("NoIntroducers"),
            Self::Published(entries) => formatter
                .debug_struct("Published")
                .field("entries", &entries.len())
                .finish(),
        }
    }
}

impl PublishedIntroducerSet {
    /// Whether the peer publishes an introducer set at all.
    pub const fn is_published(&self) -> bool {
        matches!(self, Self::Published(_))
    }

    /// The published entries, or an empty slice.
    pub fn entries(&self) -> &[PublishedIntroducer] {
        match self {
            Self::Published(entries) => entries,
            _ => &[],
        }
    }
}

/// The endpoint of an introducer, resolved by hash from the validated
/// store.
///
/// The published set names the introducer by **hash**, never by
/// endpoint, so the requester must resolve it. Every refusal is a
/// distinct variant.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum IntroducerEndpoint {
    /// The introducer's validated record resolved to a direct endpoint.
    Resolved(i2pr_runtime::Ssu2Endpoint),
    /// The introducer has no validated record in the authoritative store.
    NoStoredRecord,
    /// The introducer's validated record carries no SSU2 address.
    NoSsu2Address,
    /// The introducer's SSU2 address did not parse under the strict
    /// parser.
    UnparsableAddress,
    /// The introducer's SSU2 address names no direct endpoint, so the
    /// introducer itself is only reachable through another introducer.
    NoDirectEndpoint,
}

impl std::fmt::Debug for IntroducerEndpoint {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Resolved(_) => formatter
                .debug_struct("Resolved")
                .field("endpoint", &"<redacted>")
                .finish(),
            Self::NoStoredRecord => formatter.write_str("NoStoredRecord"),
            Self::NoSsu2Address => formatter.write_str("NoSsu2Address"),
            Self::UnparsableAddress => formatter.write_str("UnparsableAddress"),
            Self::NoDirectEndpoint => formatter.write_str("NoDirectEndpoint"),
        }
    }
}

/// Reads one peer's **published** introducer set from the authoritative
/// store.
///
/// The bytes are the ones the netdb **validated on ingest** — the
/// signature, the identity hash, the static-key binding and the
/// freshness were all checked before the record reached the store — and
/// the address is parsed with the transport's own **strict** parser. A
/// peer with no stored record, with no SSU2 address, with an unparsable
/// address, or with an address that publishes no introducer set is
/// **not** a target, and each is a distinct refusal — never a silent
/// absence.
///
/// The read never mutates the store and never reaches outside it: a peer
/// this router has not learned about is simply not a target.
pub fn read_published_introducers(
    store: &i2pr_netdb::RouterInfoStore,
    peer: PeerId,
) -> PublishedIntroducerSet {
    let key = i2pr_netdb::RouterHash::from_hash(peer.hash());
    let Some(record) = store.get(&key) else {
        return PublishedIntroducerSet::NoStoredRecord;
    };
    let Some(address) = record
        .router_info()
        .addresses()
        .iter()
        .find(|address| address.transport_style() == "SSU2")
    else {
        return PublishedIntroducerSet::NoSsu2Address;
    };
    let Ok(parsed) = i2pr_runtime::Ssu2RouterAddress::parse(address) else {
        return PublishedIntroducerSet::UnparsableAddress;
    };
    let entries: Vec<PublishedIntroducer> = parsed
        .introducers()
        .iter()
        .map(|entry| PublishedIntroducer {
            peer_hash: *entry.peer_hash(),
            relay_tag: entry.relay_tag(),
            expires_secs: entry.expires_secs(),
        })
        .collect();
    if entries.is_empty() {
        return PublishedIntroducerSet::NoIntroducers;
    }
    PublishedIntroducerSet::Published(entries)
}

/// Resolves one introducer's direct endpoint by hash, from the
/// authoritative store.
///
/// The resolution is a store read, never a network lookup: the published
/// set names the introducer by hash, and the hash is the only key the
/// requester may use.
pub fn read_introducer_endpoint(
    store: &i2pr_netdb::RouterInfoStore,
    introducer: [u8; 32],
) -> IntroducerEndpoint {
    let key = i2pr_netdb::RouterHash::from_bytes(introducer);
    let Some(record) = store.get(&key) else {
        return IntroducerEndpoint::NoStoredRecord;
    };
    let Some(address) = record
        .router_info()
        .addresses()
        .iter()
        .find(|address| address.transport_style() == "SSU2")
    else {
        return IntroducerEndpoint::NoSsu2Address;
    };
    let Ok(parsed) = i2pr_runtime::Ssu2RouterAddress::parse(address) else {
        return IntroducerEndpoint::UnparsableAddress;
    };
    match parsed.endpoint() {
        Some(endpoint) => IntroducerEndpoint::Resolved(endpoint),
        None => IntroducerEndpoint::NoDirectEndpoint,
    }
}

/// Why the router did not start a relay request, or did not act on an
/// inbound relay event.
///
/// Every refusal is a distinct variant and is counted under its own
/// reason. None of them writes anything into a peer's profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RelayRequesterRefusal {
    /// The operator left the opt-in off.
    Disabled,
    /// The router names no bound endpoint, so it cannot present one.
    NoLocalEndpoint,
    /// No peer this router holds a session with publishes an introducer
    /// set.
    NoEligibleTarget,
    /// The published set names no live introducer.
    NoLiveIntroducer,
    /// The named introducer is unknown to the authoritative store.
    IntroducerUnknown,
    /// The named introducer resolves to no direct endpoint.
    IntroducerUnreachable,
    /// This router holds no session with the named introducer.
    NoSession,
    /// No fresh nonzero nonce could be drawn.
    NoNonce,
    /// The request could not be signed with the router's own key.
    SigningRefused,
    /// The transport refused the request.
    TransportRefused,
    /// The carriage refused to put the request on a session.
    CarriageRefused,
    /// The inbound response correlated with no live request.
    UnknownCorrelation,
    /// The inbound response came from a peer that is not the named
    /// introducer.
    SenderMismatch,
    /// The inbound event's signature did not verify.
    InvalidSignature,
    /// The inbound event's timestamp was outside the permitted skew.
    StaleTimestamp,
    /// The target's signing key is unknown to the validated store, so the
    /// inbound event's signature cannot be verified.
    TargetUnknown,
    /// The deferred-punch buffer is full (Plan 533).
    ///
    /// A hole punch that arrived before its accept could not be held, so
    /// the exchange for that nonce is lost. The loss is counted and never
    /// silent.
    DeferralFull,
    /// The establishment could not be published (Plan 533).
    ///
    /// The establishment table is full, or the accept carried no endpoint
    /// or no token. Either way the router cannot reach the peer it was
    /// relayed to, and it says so under its own reason.
    EstablishmentUnavailable,
    /// The handoff to the establishment task is full (Plan 533).
    EstablishmentQueueFull,
}

/// The owner's counters, one per outcome.
///
/// Every refusal and every outcome is counted under its own reason, so a
/// caller can observe what happened instead of inferring it. The counters
/// are counts only: they name no peer, no address, no key and no tag.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RelayRequesterCounters {
    /// Relay requests started and carried toward an introducer.
    pub requests_started: u64,
    /// Candidates whose published introducer set was read.
    pub targets_published: u64,
    /// Reads refused because the peer has no validated record.
    pub targets_refused_no_record: u64,
    /// Reads refused because the record carries no SSU2 address.
    pub targets_refused_no_ssu2_address: u64,
    /// Reads refused because the SSU2 address did not parse.
    pub targets_refused_unparsable: u64,
    /// Reads refused because the address publishes no introducer set.
    pub targets_refused_no_introducers: u64,
    /// Relay responses correlated with a live request.
    pub responses_correlated: u64,
    /// Relay responses that accepted the request.
    pub responses_accepted: u64,
    /// Relay responses that refused the request.
    pub responses_refused: u64,
    /// Hole punches correlated with a live request.
    pub hole_punches_correlated: u64,
    /// Hole punches that completed the exchange.
    pub hole_punches_ready: u64,
    /// Relay-firewalled indications recorded.
    pub firewalled_recorded: u64,
    /// Hole punches held until their accept arrived (Plan 533).
    pub punches_deferred: u64,
    /// Establishments published for the task to perform (Plan 533).
    pub establishments_ready: u64,
    /// Refusals because the operator left the opt-in off.
    pub refusals_disabled: u64,
    /// Refusals because the router names no bound endpoint.
    pub refusals_no_local_endpoint: u64,
    /// Refusals because no eligible target was found.
    pub refusals_no_eligible_target: u64,
    /// Refusals because the published set names no live introducer.
    pub refusals_no_live_introducer: u64,
    /// Refusals because the introducer is unknown to the store.
    pub refusals_introducer_unknown: u64,
    /// Refusals because the introducer resolves to no endpoint.
    pub refusals_introducer_unreachable: u64,
    /// Refusals because the router holds no session with the introducer.
    pub refusals_no_session: u64,
    /// Refusals because no fresh nonzero nonce could be drawn.
    pub refusals_no_nonce: u64,
    /// Refusals because the request could not be signed.
    pub refusals_signing: u64,
    /// Refusals because the transport refused the request.
    pub refusals_transport: u64,
    /// Refusals because the carriage refused the request.
    pub refusals_carriage: u64,
    /// Refusals because an inbound response correlated with no request.
    pub refusals_unknown_correlation: u64,
    /// Refusals because an inbound response came from the wrong peer.
    pub refusals_sender_mismatch: u64,
    /// Refusals because an inbound signature did not verify.
    pub refusals_invalid_signature: u64,
    /// Refusals because an inbound timestamp was stale.
    pub refusals_stale_timestamp: u64,
    /// Refusals because the target's signing key is unknown.
    pub refusals_target_unknown: u64,
    /// Refusals because the deferred-punch buffer was full.
    pub refusals_deferral_full: u64,
    /// Refusals because the establishment could not be published.
    pub refusals_establishment_unavailable: u64,
    /// Refusals because the handoff to the establishment task was full.
    pub refusals_establishment_queue_full: u64,
}

/// The context of one live relay request.
///
/// It names the introducer the request went to, the target the request
/// named, the published tag and the address family of the router's own
/// endpoint. It is the daemon's view of the request; the transport's
/// table holds the transport's view.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct RelayRequestContext {
    /// The introducer (Bob) the request named.
    pub introducer: [u8; 32],
    /// The target (Charlie) the request named.
    pub target: [u8; 32],
    /// The published relay tag.
    pub tag: u32,
    /// The address family of the router's own endpoint.
    pub family: AddressFamily,
}

impl std::fmt::Debug for RelayRequestContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RelayRequestContext")
            .field("introducer", &"<redacted>")
            .field("target", &"<redacted>")
            .field("tag", &"<redacted>")
            .field("family", &self.family)
            .finish()
    }
}

/// The material of one establishment the requester is ready to perform
/// (Plan 533).
///
/// The establishment is the requester's **ordinary** session establishment
/// with the peer it reached through a relay. The accept carries the
/// target's endpoint and the establishment token the target's responder
/// issued, and both are needed to open the session.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct RelayEstablishment {
    /// The target (Charlie) the session is with.
    pub target: [u8; 32],
    /// The target's endpoint, from the accept.
    ///
    /// The accept's preimage carries the endpoint, so the target's own
    /// signature authenticates it.
    pub endpoint: i2pr_runtime::Ssu2Endpoint,
    /// The establishment token, from the accept.
    ///
    /// The token was issued by the target's responder and is bound to
    /// this router's exact source address. It is one-use.
    pub token: u64,
}

impl std::fmt::Debug for RelayEstablishment {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RelayEstablishment")
            .field("target", &"<redacted>")
            .field("endpoint", &"<redacted>")
            .field("token", &"<redacted>")
            .finish()
    }
}

/// One candidate target, with its published introducer resolved.
///
/// The tag and the target come from the **same** published entry, so a
/// request never names a target the tag does not identify.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct EligibleTarget {
    /// The target peer whose published set named the introducer.
    pub target: PeerId,
    /// The introducer the request goes to.
    pub introducer: PeerId,
    /// The published relay tag.
    pub tag: u32,
    /// The introducer's resolved direct endpoint.
    pub endpoint: i2pr_runtime::Ssu2Endpoint,
}

impl std::fmt::Debug for EligibleTarget {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EligibleTarget")
            .field("target", &"<redacted>")
            .field("introducer", &"<redacted>")
            .field("tag", &"<redacted>")
            .field("endpoint", &"<redacted>")
            .finish()
    }
}

/// What the owner decided for one window.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum RelayRequesterDecision {
    /// Ask this introducer to relay toward this target.
    Request(EligibleTarget),
    /// Ask nobody; this is why.
    Refuse(RelayRequesterRefusal),
}

impl std::fmt::Debug for RelayRequesterDecision {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Request(target) => formatter
                .debug_struct("Request")
                .field("target", target)
                .finish(),
            Self::Refuse(reason) => formatter
                .debug_struct("Refuse")
                .field("reason", reason)
                .finish(),
        }
    }
}

/// The typed outcome of one inbound requester event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RelayRequesterOutcome {
    /// The response was correlated and the peer accepted the request.
    ResponseAccepted,
    /// The response was correlated and the peer refused the request.
    ResponseRefused,
    /// The hole punch was correlated and the exchange completed.
    HolePunchReady,
    /// The hole punch was correlated but the exchange was not accepted.
    HolePunchNotReady,
    /// The hole punch arrived before its accept and was held (Plan 533).
    ///
    /// This is **not** a refusal. The transport requires the accept
    /// before a hole punch, and the punch travels one hop while the
    /// accept travels two, so the owner holds the punch and presents it
    /// again once the accept is correlated.
    PunchDeferred,
    /// The event was refused.
    Refused(RelayRequesterRefusal),
}

/// The router's own signing key, as the requester owner sees it.
///
/// The seed never leaves the bundle: the seam signs one preimage and
/// returns the signature bytes, so no caller can copy the seed into a
/// struct, a log or an assertion.
pub trait RelayRequestSigner: Send + Sync {
    /// Signs the specification's RelayRequest preimage.
    ///
    /// Returns `None` when the key cannot sign. A partial or malformed
    /// signature is never returned.
    fn sign_relay_request(&self, preimage: &[u8]) -> Option<Vec<u8>>;
}

/// The production signer: this router's own identity bundle.
pub struct LocalIdentitySigner {
    bundle: std::sync::Arc<i2pr_crypto::RouterIdentityBundle>,
}

impl LocalIdentitySigner {
    /// Wraps the router's own identity bundle.
    pub fn new(bundle: std::sync::Arc<i2pr_crypto::RouterIdentityBundle>) -> Self {
        Self { bundle }
    }

    /// Signs one preimage with the router's own identity key.
    ///
    /// The seed never leaves the bundle: the seam signs one preimage and
    /// returns the signature bytes, so no caller can copy the seed into a
    /// struct, a log or an assertion. Plan 532's target owner signs its
    /// accept through this same seam, so there is **one** implementation
    /// of the router's own signing.
    pub fn sign(&self, preimage: &[u8]) -> Option<Vec<u8>> {
        self.bundle
            .signing_key()
            .sign(preimage)
            .ok()
            .map(|value| value.as_bytes().to_vec())
    }
}

impl RelayRequestSigner for LocalIdentitySigner {
    fn sign_relay_request(&self, preimage: &[u8]) -> Option<Vec<u8>> {
        self.sign(preimage)
    }
}

/// The bounded owner of this router's relay requests.
///
/// The owner selects the target, resolves the introducer, draws the
/// nonce, starts the request through the runtime's seam and records every
/// outcome. It sends nothing by itself: the runtime owns the session
/// and the carriage.
pub struct RelayRequesterOwner {
    /// The operator's opt-in. A disabled owner refuses every window.
    enabled: bool,
    /// The runtime that owns the sessions, the tables and the transport's
    /// verification.
    runtime: i2pr_runtime::Ssu2RuntimeService,
    /// The router's own signing key.
    signer: std::sync::Arc<dyn RelayRequestSigner>,
    /// The peers this router already asked.
    ///
    /// The set is pruned against the live session peers on every window,
    /// so it never outgrows the router's own peer population.
    asked: BTreeSet<PeerId>,
    /// The rotating cursor: the last peer hash this owner examined.
    cursor: Option<[u8; 32]>,
    /// The local time the next window is due.
    next_at_ms: u64,
    /// The live requests, by nonce.
    live: BTreeMap<u32, LiveRelayRequest>,
    /// The hole punches that arrived before their accept (Plan 533).
    ///
    /// The transport's requester table refuses a hole punch until the
    /// accept is correlated, and the punch travels one hop while the
    /// accept travels two. The owner holds the punch here instead of
    /// losing it, and presents it again once the transport's
    /// precondition holds.
    deferred: BTreeMap<u32, DeferredPunch>,
    /// The establishments the owner has published (Plan 533).
    ///
    /// One per completed exchange. The owner publishes them and its task
    /// takes them; the owner holds no socket and dials nothing.
    establishments: BTreeMap<u32, RelayEstablishment>,
    counters: RelayRequesterCounters,
}

/// One live relay request, with its deadline.
#[derive(Clone, Copy)]
struct LiveRelayRequest {
    context: RelayRequestContext,
    deadline_ms: u64,
    /// The accept's endpoint, once the accept has been correlated.
    accept_endpoint: Option<i2pr_runtime::Ssu2Endpoint>,
    /// The accept's token, once the accept has been correlated.
    accept_token: Option<u64>,
}

/// One hole punch held until its accept arrives (Plan 533).
#[derive(Clone)]
struct DeferredPunch {
    /// The authenticated hole punch.
    message: i2pr_runtime::HolePunchMessage,
}

impl std::fmt::Debug for RelayRequesterOwner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RelayRequesterOwner")
            .field("enabled", &self.enabled)
            .field("live_requests", &self.live.len())
            .finish()
    }
}

impl RelayRequesterOwner {
    /// Creates an owner.
    ///
    /// `enabled` is the operator's opt-in. A disabled owner refuses every
    /// window, so a router that has not asked to reach a peer through an
    /// introducer never sends a relay request.
    pub fn new(
        enabled: bool,
        runtime: i2pr_runtime::Ssu2RuntimeService,
        signer: std::sync::Arc<dyn RelayRequestSigner>,
    ) -> Self {
        Self {
            enabled,
            runtime,
            signer,
            asked: BTreeSet::new(),
            cursor: None,
            next_at_ms: 0,
            live: BTreeMap::new(),
            deferred: BTreeMap::new(),
            establishments: BTreeMap::new(),
            counters: RelayRequesterCounters::default(),
        }
    }

    /// Whether the operator enabled this router's relay requests.
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    /// Returns the owner's counters.
    pub const fn counters(&self) -> RelayRequesterCounters {
        self.counters
    }

    /// Whether a window is due at `now_ms`.
    pub const fn due(&self, now_ms: u64) -> bool {
        now_ms >= self.next_at_ms
    }

    /// Returns the runtime that owns the sessions and the tables.
    pub const fn runtime(&self) -> &i2pr_runtime::Ssu2RuntimeService {
        &self.runtime
    }

    /// Returns the bounded window of peers this window may examine.
    ///
    /// `live` is the peers this router currently holds a session with. The
    /// window is drawn from `live` in a stable order and **rotated** past
    /// the peers the previous window examined, so every peer is seen over
    /// time. The `asked` set is pruned against `live` first, so a peer
    /// this router already asked is never asked again while its session
    /// lives.
    pub fn candidate_window(&mut self, live: &[PeerId], limit: usize) -> Vec<PeerId> {
        self.asked.retain(|peer| live.contains(peer));
        let mut ordered: Vec<PeerId> = live.to_vec();
        ordered.sort_by(|left, right| left.hash().as_bytes().cmp(right.hash().as_bytes()));
        let start = match self.cursor {
            Some(cursor) => ordered
                .iter()
                .position(|peer| peer.hash().as_bytes() > &cursor)
                .unwrap_or(0),
            None => 0,
        };
        let window: Vec<PeerId> = ordered
            .iter()
            .skip(start)
            .chain(ordered.iter().take(start))
            .filter(|peer| !self.asked.contains(peer))
            .take(limit)
            .copied()
            .collect();
        if let Some(last) = window.last() {
            self.cursor = Some(*last.hash().as_bytes());
        }
        window
    }

    /// Records one read of a peer's published introducer set.
    pub fn note_published(&mut self, set: &PublishedIntroducerSet) {
        match set {
            PublishedIntroducerSet::Published(_) => self.counters.targets_published += 1,
            PublishedIntroducerSet::NoStoredRecord => {
                self.counters.targets_refused_no_record += 1;
            }
            PublishedIntroducerSet::NoSsu2Address => {
                self.counters.targets_refused_no_ssu2_address += 1;
            }
            PublishedIntroducerSet::UnparsableAddress => {
                self.counters.targets_refused_unparsable += 1;
            }
            PublishedIntroducerSet::NoIntroducers => {
                self.counters.targets_refused_no_introducers += 1;
            }
        }
    }

    /// Decides whether this window starts a relay request.
    ///
    /// `eligible` holds the candidates of this window that publish a live
    /// introducer set, in the window's order, with the introducer
    /// resolved. At most **one** request leaves per window.
    pub fn decide(&mut self, eligible: &[EligibleTarget], now_ms: u64) -> RelayRequesterDecision {
        if !self.enabled {
            self.refuse(RelayRequesterRefusal::Disabled, now_ms);
            return RelayRequesterDecision::Refuse(RelayRequesterRefusal::Disabled);
        }
        let Some(candidate) = eligible.first().copied() else {
            self.refuse(RelayRequesterRefusal::NoEligibleTarget, now_ms);
            return RelayRequesterDecision::Refuse(RelayRequesterRefusal::NoEligibleTarget);
        };
        self.next_at_ms = now_ms.saturating_add(RELAY_REQUEST_INTERVAL_MS);
        RelayRequesterDecision::Request(candidate)
    }

    /// Draws a fresh nonzero nonce that no live request is using.
    ///
    /// The transport refuses a zero nonce and a duplicate correlation, so
    /// the owner draws a fresh one per request and never reuses one while
    /// the request is live. The draw is bounded: a router whose entropy
    /// source cannot produce one refuses rather than reuse one.
    pub fn draw_nonce<R: RngCore>(&mut self, rng: &mut R) -> Option<u32> {
        for _ in 0..MAX_NONCE_ATTEMPTS {
            let nonce = rng.next_u32();
            if nonce == 0 || self.live.contains_key(&nonce) {
                continue;
            }
            return Some(nonce);
        }
        None
    }

    /// Builds the specification's RelayRequest block for one candidate.
    ///
    /// The preimage binds the introducer's hash, the target's hash, the
    /// nonce, the published tag, the timestamp, the version and the
    /// router's own endpoint, and it is signed with the router's **own**
    /// key: the introducer verifies the requester's signature over that
    /// preimage before it forwards the introduction.
    pub fn build_request(
        &self,
        nonce: u32,
        candidate: &EligibleTarget,
        timestamp: u32,
        version: u8,
        local_endpoint: i2pr_runtime::Ssu2Endpoint,
    ) -> Result<i2pr_runtime::RelayRequestBlock, RelayRequesterRefusal> {
        let preimage = i2pr_runtime::relay_request_preimage(
            candidate.introducer.hash().as_bytes(),
            candidate.target.hash().as_bytes(),
            nonce,
            candidate.tag,
            timestamp,
            version,
            local_endpoint,
        );
        let signature = self
            .signer
            .sign_relay_request(&preimage)
            .ok_or(RelayRequesterRefusal::SigningRefused)?;
        i2pr_runtime::RelayRequestBlock::new(
            nonce,
            candidate.tag,
            timestamp,
            version,
            local_endpoint,
            signature,
        )
        .map_err(|_| RelayRequesterRefusal::TransportRefused)
    }

    /// Starts one relay request: records it in the transport's table and
    /// carries it onto the introducer's session.
    ///
    /// The transport's table owns the correlation, the per-peer quota and
    /// the global ceiling. A request that the transport refused leaves no
    /// state behind, and a request that could not be carried is
    /// cancelled, so the transport's quota is released.
    pub fn start_request(
        &mut self,
        nonce: u32,
        candidate: &EligibleTarget,
        block: &i2pr_runtime::RelayRequestBlock,
        local_endpoint: i2pr_runtime::Ssu2Endpoint,
        now_ms: u64,
    ) -> Result<RelayRequestContext, RelayRequesterRefusal> {
        let family = match local_endpoint.ip() {
            std::net::IpAddr::V4(_) => AddressFamily::Ipv4,
            std::net::IpAddr::V6(_) => AddressFamily::Ipv6,
        };
        let context = RelayRequestContext {
            introducer: *candidate.introducer.hash().as_bytes(),
            target: *candidate.target.hash().as_bytes(),
            tag: candidate.tag,
            family,
        };
        if self.live.len() >= MAX_LIVE_RELAY_REQUESTS {
            self.refuse(RelayRequesterRefusal::TransportRefused, now_ms);
            return Err(RelayRequesterRefusal::TransportRefused);
        }
        if self
            .runtime
            .start_relay_request(
                nonce,
                candidate.tag,
                context.introducer,
                context.target,
                local_endpoint,
                now_ms,
            )
            .is_err()
        {
            self.refuse(RelayRequesterRefusal::TransportRefused, now_ms);
            return Err(RelayRequesterRefusal::TransportRefused);
        }
        if self
            .runtime
            .queue_relay_request(candidate.introducer, block.clone())
            .is_err()
        {
            // The carriage refused the block, so the request never left
            // the router: the transport's quota is released and no state
            // is left behind.
            let _ = self.runtime.cancel_relay_request(nonce);
            self.refuse(RelayRequesterRefusal::CarriageRefused, now_ms);
            return Err(RelayRequesterRefusal::CarriageRefused);
        }
        self.live.insert(
            nonce,
            LiveRelayRequest {
                context,
                deadline_ms: now_ms.saturating_add(RELAY_REQUEST_LIFETIME_MS),
                accept_endpoint: None,
                accept_token: None,
            },
        );
        self.asked.insert(candidate.target);
        self.counters.requests_started += 1;
        Ok(context)
    }

    /// Returns the context of one live request.
    pub fn context_for(&self, nonce: u32) -> Option<RelayRequestContext> {
        self.live.get(&nonce).map(|entry| entry.context)
    }

    /// Whether this response belongs to one of the owner's live requests.
    ///
    /// The correlation is by nonce, exactly as the transport's own table
    /// correlates. A response that correlates with no live request is not
    /// the requester's, and the owner refuses it instead of guessing.
    pub fn owns_response(&self, nonce: u32) -> bool {
        self.live.contains_key(&nonce)
    }

    /// Consumes one inbound Relay Response for the requester's own
    /// request.
    ///
    /// The response is verified by the transport: the sender must be the
    /// named introducer, the signature must verify under the target's
    /// key, and the timestamp must be fresh. Every refusal is typed and
    /// leaves no state behind.
    pub fn on_relay_response(
        &mut self,
        block: &i2pr_runtime::RelayResponseBlock,
        sender: [u8; 32],
        signers: &dyn crate::ssu2_introducer::IntroducerSignerResolver,
        now_secs: u64,
        now_ms: u64,
    ) -> RelayRequesterOutcome {
        let nonce = block.nonce();
        let Some(entry) = self.live.get(&nonce).copied() else {
            self.refuse(RelayRequesterRefusal::UnknownCorrelation, now_ms);
            return RelayRequesterOutcome::Refused(RelayRequesterRefusal::UnknownCorrelation);
        };
        // The target's signing key comes from the **validated** store. A
        // target this router does not hold yields `None`, and the owner
        // refuses the event rather than guessing.
        let Some(charlie_signer) = signers.signing_key(&entry.context.target) else {
            self.refuse(RelayRequesterRefusal::TargetUnknown, now_ms);
            return RelayRequesterOutcome::Refused(RelayRequesterRefusal::TargetUnknown);
        };
        self.counters.responses_correlated += 1;
        match self.runtime.on_requester_relay_response(
            block,
            &sender,
            &entry.context.introducer,
            &charlie_signer,
            now_secs,
            now_ms,
        ) {
            Ok(i2pr_runtime::Ssu2RelayRequestState::AwaitingHolePunch) => {
                self.counters.responses_accepted += 1;
                // The accept carries the target's endpoint and the
                // establishment token the target's responder issued. The
                // accept's preimage carries the endpoint, so the target's
                // own signature authenticates it. Both are kept for the
                // establishment, which the exchange opens.
                if let Some(live) = self.live.get_mut(&nonce) {
                    live.accept_endpoint = block.endpoint();
                    live.accept_token = block.token();
                }
                RelayRequesterOutcome::ResponseAccepted
            }
            Ok(i2pr_runtime::Ssu2RelayRequestState::Refused) => {
                self.counters.responses_refused += 1;
                self.live.remove(&nonce);
                RelayRequesterOutcome::ResponseRefused
            }
            Ok(i2pr_runtime::Ssu2RelayRequestState::AwaitingResponse) => {
                self.refuse(RelayRequesterRefusal::TransportRefused, now_ms);
                RelayRequesterOutcome::Refused(RelayRequesterRefusal::TransportRefused)
            }
            Err(error) => {
                let reason = map_relay_error(error);
                self.refuse(reason, now_ms);
                RelayRequesterOutcome::Refused(reason)
            }
        }
    }

    /// Consumes one inbound HolePunch for the requester's own request.
    ///
    /// A ready punch means the exchange completed: the peer is reachable
    /// **through the relay**, and the router's own inbound reachability
    /// is contradicted. The owner records the fact through the runtime's
    /// own recorder — the daemon never writes into the reachability
    /// tracker itself — and never into a peer's profile.
    ///
    /// A punch that arrives **before** its accept is **held**, not
    /// refused (Plan 533). The transport's requester table requires the
    /// accept before a hole punch, and the punch travels one hop while
    /// the accept travels two, so on loopback the punch arrives first.
    /// Holding it is what lets the exchange complete at all; the hold is
    /// bounded and the punch is presented again once the accept is
    /// correlated.
    pub fn on_hole_punch(
        &mut self,
        message: &i2pr_runtime::HolePunchMessage,
        signers: &dyn crate::ssu2_introducer::IntroducerSignerResolver,
        now_secs: u64,
        now_ms: u64,
    ) -> RelayRequesterOutcome {
        self.handle_hole_punch(message, signers, now_secs, now_ms, true)
    }

    /// Holds one hole punch until its accept is correlated.
    ///
    /// The hold is bounded: a router that is flooded with early punches
    /// refuses them under their own reason rather than growing a table.
    /// The held punch is dropped with its request when the request
    /// expires.
    fn defer_hole_punch(
        &mut self,
        message: &i2pr_runtime::HolePunchMessage,
        now_ms: u64,
    ) -> RelayRequesterOutcome {
        let nonce = message.response.nonce();
        if self.deferred.len() >= MAX_DEFERRED_PUNCHES {
            self.refuse(RelayRequesterRefusal::DeferralFull, now_ms);
            return RelayRequesterOutcome::Refused(RelayRequesterRefusal::DeferralFull);
        }
        self.deferred.insert(
            nonce,
            DeferredPunch {
                message: message.clone(),
            },
        );
        self.counters.punches_deferred += 1;
        RelayRequesterOutcome::PunchDeferred
    }

    /// Presents a held hole punch again, now that its accept is
    /// correlated.
    ///
    /// Returns `None` when no punch was held for the nonce. The punch is
    /// presented through the transport's own seam, so the transport's
    /// verification runs again on it and its one-use guarantee is
    /// unchanged. A re-drive that the transport still refuses is a
    /// **refusal**, not another hold: the hold has already had its one
    /// chance.
    pub fn redrive_deferred(
        &mut self,
        nonce: u32,
        signers: &dyn crate::ssu2_introducer::IntroducerSignerResolver,
        now_secs: u64,
        now_ms: u64,
    ) -> Option<RelayRequesterOutcome> {
        let deferred = self.deferred.remove(&nonce)?;
        Some(self.handle_hole_punch(&deferred.message, signers, now_secs, now_ms, false))
    }

    /// Correlates one hole punch with the requester's own request.
    ///
    /// `allow_deferral` is true only for a punch that arrives from the
    /// handoff. The transport refuses a hole punch whose accept has not
    /// been correlated, and that refusal is the **deferral signal**: it
    /// is intercepted before the generic mapping, so a punch that merely
    /// arrived early is never counted as a transport refusal.
    fn handle_hole_punch(
        &mut self,
        message: &i2pr_runtime::HolePunchMessage,
        signers: &dyn crate::ssu2_introducer::IntroducerSignerResolver,
        now_secs: u64,
        now_ms: u64,
        allow_deferral: bool,
    ) -> RelayRequesterOutcome {
        let nonce = message.response.nonce();
        let Some(entry) = self.live.get(&nonce).copied() else {
            self.refuse(RelayRequesterRefusal::UnknownCorrelation, now_ms);
            return RelayRequesterOutcome::Refused(RelayRequesterRefusal::UnknownCorrelation);
        };
        let Some(charlie_signer) = signers.signing_key(&entry.context.target) else {
            self.refuse(RelayRequesterRefusal::TargetUnknown, now_ms);
            return RelayRequesterOutcome::Refused(RelayRequesterRefusal::TargetUnknown);
        };
        self.counters.hole_punches_correlated += 1;
        match self.runtime.on_requester_hole_punch(
            message,
            &entry.context.introducer,
            &charlie_signer,
            now_secs,
            now_ms,
        ) {
            Ok(true) => {
                self.counters.hole_punches_ready += 1;
                // The relay's success is a **contradiction**: the router is
                // reachable through the relay, not directly. The runtime
                // owns the router's reachability, so the runtime records
                // the indication.
                self.runtime.note_relay_firewalled(entry.context.family);
                self.counters.firewalled_recorded += 1;
                // The exchange is complete, so the establishment may now
                // be published: the accept's endpoint and token are the
                // material the requester's task dials with.
                self.publish_establishment(nonce, &entry, now_ms);
                self.live.remove(&nonce);
                RelayRequesterOutcome::HolePunchReady
            }
            Ok(false) => {
                self.live.remove(&nonce);
                RelayRequesterOutcome::HolePunchNotReady
            }
            Err(error) => {
                if allow_deferral && matches!(error, i2pr_runtime::RelayError::WrongRole) {
                    // The accept has not been correlated yet. The punch is
                    // held, not refused.
                    return self.defer_hole_punch(message, now_ms);
                }
                let reason = map_relay_error(error);
                self.refuse(reason, now_ms);
                RelayRequesterOutcome::Refused(reason)
            }
        }
    }

    /// Publishes the establishment for one completed exchange.
    ///
    /// The establishment is the requester's ordinary session
    /// establishment with the peer it reached through a relay: the
    /// accept's endpoint and the establishment token the target's
    /// responder issued. The owner publishes it and its task takes it —
    /// the owner holds no socket and dials nothing.
    ///
    /// An accept that carried no endpoint or no token is refused under
    /// its own reason: the router cannot reach the peer it was relayed
    /// to, and it says so rather than dialling something it cannot
    /// verify.
    fn publish_establishment(&mut self, nonce: u32, entry: &LiveRelayRequest, now_ms: u64) {
        let (Some(endpoint), Some(token)) = (entry.accept_endpoint, entry.accept_token) else {
            self.refuse(RelayRequesterRefusal::EstablishmentUnavailable, now_ms);
            return;
        };
        if self.establishments.len() >= MAX_PENDING_ESTABLISHMENTS {
            self.refuse(RelayRequesterRefusal::EstablishmentUnavailable, now_ms);
            return;
        }
        self.establishments.insert(
            nonce,
            RelayEstablishment {
                target: entry.context.target,
                endpoint,
                token,
            },
        );
        self.counters.establishments_ready += 1;
    }

    /// Takes the oldest published establishment, if any.
    ///
    /// The owner publishes and its task takes; the handoff between them
    /// is bounded, and a handoff that is full returns the material to
    /// the owner rather than dropping it.
    pub fn take_establishment(&mut self) -> Option<(u32, RelayEstablishment)> {
        let nonce = *self.establishments.keys().next()?;
        let establishment = self.establishments.remove(&nonce)?;
        Some((nonce, establishment))
    }

    /// Returns a published establishment the handoff could not take.
    ///
    /// The material goes back into the owner's table and the refusal is
    /// counted: an establishment is never dropped silently.
    pub fn return_establishment(
        &mut self,
        nonce: u32,
        establishment: RelayEstablishment,
        now_ms: u64,
    ) {
        self.refuse(RelayRequesterRefusal::EstablishmentQueueFull, now_ms);
        if self.establishments.len() < MAX_PENDING_ESTABLISHMENTS {
            self.establishments.insert(nonce, establishment);
        }
    }

    /// Records one refusal under its own reason.
    ///
    /// A refusal that follows a refused request leaves the cadence short,
    /// so a router that learns of a published introducer set does not
    /// wait a full interval for it.
    pub fn refuse(&mut self, reason: RelayRequesterRefusal, now_ms: u64) {
        match reason {
            RelayRequesterRefusal::Disabled => self.counters.refusals_disabled += 1,
            RelayRequesterRefusal::NoLocalEndpoint => {
                self.counters.refusals_no_local_endpoint += 1;
            }
            RelayRequesterRefusal::NoEligibleTarget => {
                self.counters.refusals_no_eligible_target += 1;
            }
            RelayRequesterRefusal::NoLiveIntroducer => {
                self.counters.refusals_no_live_introducer += 1;
            }
            RelayRequesterRefusal::IntroducerUnknown => {
                self.counters.refusals_introducer_unknown += 1;
            }
            RelayRequesterRefusal::IntroducerUnreachable => {
                self.counters.refusals_introducer_unreachable += 1;
            }
            RelayRequesterRefusal::NoSession => self.counters.refusals_no_session += 1,
            RelayRequesterRefusal::NoNonce => self.counters.refusals_no_nonce += 1,
            RelayRequesterRefusal::SigningRefused => self.counters.refusals_signing += 1,
            RelayRequesterRefusal::TransportRefused => self.counters.refusals_transport += 1,
            RelayRequesterRefusal::CarriageRefused => self.counters.refusals_carriage += 1,
            RelayRequesterRefusal::UnknownCorrelation => {
                self.counters.refusals_unknown_correlation += 1;
            }
            RelayRequesterRefusal::SenderMismatch => {
                self.counters.refusals_sender_mismatch += 1;
            }
            RelayRequesterRefusal::InvalidSignature => {
                self.counters.refusals_invalid_signature += 1;
            }
            RelayRequesterRefusal::StaleTimestamp => {
                self.counters.refusals_stale_timestamp += 1;
            }
            RelayRequesterRefusal::TargetUnknown => {
                self.counters.refusals_target_unknown += 1;
            }
            RelayRequesterRefusal::DeferralFull => {
                self.counters.refusals_deferral_full += 1;
            }
            RelayRequesterRefusal::EstablishmentUnavailable => {
                self.counters.refusals_establishment_unavailable += 1;
            }
            RelayRequesterRefusal::EstablishmentQueueFull => {
                self.counters.refusals_establishment_queue_full += 1;
            }
        }
        self.next_at_ms = now_ms.saturating_add(RELAY_REQUEST_RETRY_MS);
    }

    /// Drops the live requests whose deadline passed.
    ///
    /// The owner's table is the daemon's view of the transport's own
    /// table, and it must not outlive it: a request the transport has
    /// already forgotten is not the requester's any more.
    pub fn expire(&mut self, now_ms: u64) {
        let expired: Vec<u32> = self
            .live
            .iter()
            .filter(|(_, entry)| now_ms >= entry.deadline_ms)
            .map(|(nonce, _)| *nonce)
            .collect();
        for nonce in expired {
            self.live.remove(&nonce);
            // A held punch and a published establishment belong to the
            // request they were made for, and they must not outlive it.
            self.deferred.remove(&nonce);
            self.establishments.remove(&nonce);
        }
    }
}

/// Drives one requester window on the daemon's existing tick.
///
/// Plan 531. This is the daemon's **outbound** half of the requester
/// role: it reads the candidates' **published** introducer sets out of
/// the validated store, resolves the named introducer **by hash** from
/// the same store, draws a fresh nonzero nonce, signs the request with
/// the router's own key and starts it through the runtime's seam.
///
/// `live` holds the peers this router currently holds a session with.
/// The request rides one of those sessions -- the relay blocks are
/// session blocks -- so the named introducer must be among them.
///
/// `None` means the window was not due; no refusal is counted for a
/// window that did not run.
pub fn drive_relay_request(
    owner: &mut RelayRequesterOwner,
    store: &i2pr_netdb::RouterInfoStore,
    live: &[PeerId],
    rng: &mut impl RngCore,
    local_endpoint: Option<i2pr_runtime::Ssu2Endpoint>,
    now_ms: u64,
    now_secs: u64,
) -> Option<RelayRequesterDecision> {
    if !owner.enabled() {
        owner.refuse(RelayRequesterRefusal::Disabled, now_ms);
        return Some(RelayRequesterDecision::Refuse(
            RelayRequesterRefusal::Disabled,
        ));
    }
    if !owner.due(now_ms) {
        return None;
    }
    let Some(local_endpoint) = local_endpoint else {
        owner.refuse(RelayRequesterRefusal::NoLocalEndpoint, now_ms);
        return Some(RelayRequesterDecision::Refuse(
            RelayRequesterRefusal::NoLocalEndpoint,
        ));
    };
    // The candidate pool is the NetDB: a peer this router cannot reach
    // directly has no session with it, so the pool cannot be the live
    // sessions. The store is bounded, and the window is bounded on top
    // of it.
    let pool: Vec<PeerId> = store
        .iter()
        .map(|(hash, _)| PeerId::from_hash(*hash.as_hash()))
        .collect();
    let window = owner.candidate_window(&pool, MAX_RELAY_REQUEST_CANDIDATES);
    let mut eligible: Vec<EligibleTarget> = Vec::new();
    for target in window {
        let published = read_published_introducers(store, target);
        owner.note_published(&published);
        let PublishedIntroducerSet::Published(entries) = published else {
            continue;
        };
        // The published entry must be live: a tag whose own expiry is
        // within the margin would be at the end of its life before the
        // peers that read it could use it.
        let Some(entry) = entries.iter().find(|entry| {
            entry.relay_tag != 0
                && entry.expires_secs
                    > now_secs.saturating_add(PUBLISHED_INTRODUCER_EXPIRY_MARGIN_SECS)
        }) else {
            owner.refuse(RelayRequesterRefusal::NoLiveIntroducer, now_ms);
            continue;
        };
        let endpoint = match read_introducer_endpoint(store, entry.peer_hash) {
            IntroducerEndpoint::Resolved(endpoint) => endpoint,
            IntroducerEndpoint::NoStoredRecord => {
                owner.refuse(RelayRequesterRefusal::IntroducerUnknown, now_ms);
                continue;
            }
            _ => {
                owner.refuse(RelayRequesterRefusal::IntroducerUnreachable, now_ms);
                continue;
            }
        };
        eligible.push(EligibleTarget {
            target,
            introducer: PeerId::from_bytes(entry.peer_hash),
            tag: entry.relay_tag,
            endpoint,
        });
        break;
    }
    let decision = owner.decide(&eligible, now_ms);
    let RelayRequesterDecision::Request(candidate) = decision else {
        return Some(decision);
    };
    // The request rides an authenticated session: the relay blocks are
    // session blocks, and the router never names a peer it holds no
    // session with.
    if !live.contains(&candidate.introducer) {
        owner.refuse(RelayRequesterRefusal::NoSession, now_ms);
        return Some(RelayRequesterDecision::Refuse(
            RelayRequesterRefusal::NoSession,
        ));
    }
    let Some(nonce) = owner.draw_nonce(rng) else {
        owner.refuse(RelayRequesterRefusal::NoNonce, now_ms);
        return Some(RelayRequesterDecision::Refuse(
            RelayRequesterRefusal::NoNonce,
        ));
    };
    let timestamp = u32::try_from(now_secs).unwrap_or(u32::MAX);
    let block = match owner.build_request(nonce, &candidate, timestamp, 2, local_endpoint) {
        Ok(block) => block,
        Err(reason) => {
            owner.refuse(reason, now_ms);
            return Some(RelayRequesterDecision::Refuse(reason));
        }
    };
    match owner.start_request(nonce, &candidate, &block, local_endpoint, now_ms) {
        Ok(_) => Some(RelayRequesterDecision::Request(candidate)),
        Err(reason) => Some(RelayRequesterDecision::Refuse(reason)),
    }
}

/// Maps one transport refusal onto the daemon's typed reason.
///
/// The transport's own errors are the authority; the daemon names them
/// under its own reasons so a reader sees one vocabulary.
fn map_relay_error(error: i2pr_runtime::RelayError) -> RelayRequesterRefusal {
    match error {
        i2pr_runtime::RelayError::UnknownCorrelation => RelayRequesterRefusal::UnknownCorrelation,
        i2pr_runtime::RelayError::SenderMismatch => RelayRequesterRefusal::SenderMismatch,
        i2pr_runtime::RelayError::InvalidSignature
        | i2pr_runtime::RelayError::UnsupportedSigner => RelayRequesterRefusal::InvalidSignature,
        i2pr_runtime::RelayError::StaleTimestamp => RelayRequesterRefusal::StaleTimestamp,
        _ => RelayRequesterRefusal::TransportRefused,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use i2pr_crypto::{RouterIdentityBundle, X25519PrivateKey};
    use i2pr_proto::{Date, Mapping, RouterAddress};
    use rand_core::{OsRng, TryRngCore};
    use std::time::{SystemTime, UNIX_EPOCH};

    /// The current wall clock in milliseconds.
    fn current_millis() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_millis() as u64
    }

    /// I2P-base64 (alphabet `A-Za-z0-9-~`, `=` padding) encoder for the
    /// address the runtime's identity check reads.
    fn i2p_b64_encode(bytes: &[u8]) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-~";
        let mut output = String::new();
        for chunk in bytes.chunks(3) {
            let mut n: u32 = 0;
            for byte in chunk {
                n = (n << 8) | u32::from(*byte);
            }
            n <<= 8 * (3 - chunk.len());
            let digits = match chunk.len() {
                1 => 2,
                2 => 3,
                _ => 4,
            };
            for index in 0..digits {
                output.push(ALPHABET[((n >> (18 - 6 * index)) & 0x3f) as usize] as char);
            }
            for _ in digits..4 {
                output.push('=');
            }
        }
        output
    }

    /// Builds identity material the runtime **accepts**.
    ///
    /// The runtime refuses an empty or unbound RouterInfo: it verifies the
    /// record's signature, checks that the identity hash matches the one it
    /// was handed, and requires an SSU2 address bound to the static and the
    /// introduction keys. The test therefore signs a real record instead of
    /// asserting against a placeholder the runtime rejects.
    fn identity_material() -> i2pr_runtime::Ssu2IdentityMaterial {
        let bundle = RouterIdentityBundle::generate(&mut OsRng).expect("identity");
        let hash = bundle.identity().hash().expect("hash");
        let static_key = X25519PrivateKey::generate(&mut OsRng).expect("static");
        let static_public_bytes = static_key.public_bytes();
        let mut intro_bytes = [0_u8; 32];
        loop {
            OsRng.try_fill_bytes(&mut intro_bytes).expect("rng");
            if intro_bytes.iter().any(|byte| *byte != 0) {
                break;
            }
        }
        let options = Mapping::from_entries(vec![
            ("host".to_string(), "127.0.0.1".to_string()),
            ("port".to_string(), "43000".to_string()),
            ("v".to_string(), "2".to_string()),
            ("s".to_string(), i2p_b64_encode(&static_public_bytes)),
            ("i".to_string(), i2p_b64_encode(&intro_bytes)),
        ])
        .expect("options");
        let address = RouterAddress::new(
            10,
            Date::from_millis(current_millis()),
            "SSU2".to_string(),
            options,
        )
        .expect("address");
        let info = bundle
            .sign_router_info(
                Date::from_millis(current_millis()),
                vec![address],
                Vec::new(),
                Mapping::empty(),
            )
            .expect("sign");
        i2pr_runtime::Ssu2IdentityMaterial {
            router_hash: hash,
            static_secret_bytes: *static_key.secret_bytes(),
            intro_key: i2pr_runtime::IntroKey::new(intro_bytes),
            router_info: info
                .encode_to_vec(i2pr_runtime::constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
                .expect("encode"),
        }
    }

    #[test]
    fn a_disabled_owner_refuses_every_window() {
        let mut owner = RelayRequesterOwner::new(
            false,
            i2pr_runtime::Ssu2RuntimeService::new(
                i2pr_runtime::Ssu2RuntimeConfig::default(),
                identity_material(),
            )
            .expect("runtime"),
            std::sync::Arc::new(StubSigner),
        );
        let decision = owner.decide(&[], 0);
        assert_eq!(
            decision,
            RelayRequesterDecision::Refuse(RelayRequesterRefusal::Disabled)
        );
        assert_eq!(owner.counters().refusals_disabled, 1);
        assert_eq!(owner.counters().requests_started, 0);
    }

    struct StubSigner;

    impl RelayRequestSigner for StubSigner {
        fn sign_relay_request(&self, _preimage: &[u8]) -> Option<Vec<u8>> {
            Some(vec![0x5A; 64])
        }
    }
}
