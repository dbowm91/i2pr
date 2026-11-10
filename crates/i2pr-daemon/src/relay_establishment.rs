//! Plan 533 — the requester establishes a session with the peer it reached
//! through the relay.
//!
//! ## Why the establishment exists
//!
//! The SSU2 relay exchange ends with a hole punch and an accept, and the
//! accept carries the target's endpoint and a token. The token is the
//! target's responder's **own** establishment token: the target issued it
//! from the table that consumes presented tokens, bound to the
//! requester's exact source address. Presenting it opens the ordinary
//! SSU2 session with the target — the same handshake the router performs
//! for any other peer, and never a relay-specific fake session.
//!
//! ## What this module owns
//!
//! This module owns the **policy** half of the establishment:
//!
//! - it refuses every establishment while the operator's opt-in is off;
//! - it reads the target's static and introduction keys out of the
//!   **validated** store, through the transport's strict address parser,
//!   and never parses unvalidated bytes;
//! - it builds the dial target under the **qualification profile**, so
//!   the relay establishment can never widen the router's reach;
//! - it presents the accept's token, and never a token of its own;
//! - and it counts every outcome under its own reason.
//!
//! The **mechanics** stay in the runtime: the sockets, the handshake, the
//! responder's token table and the transport's verification are the
//! runtime's, and this module reaches them only through the runtime's
//! seams.
//!
//! ## What this module never does
//!
//! - It never advertises anything. The establishment is part of the
//!   requester role, which is opt-in behind `ssu2.relay_requester` and
//!   stays non-advertised.
//! - It never logs a peer hash, an address, a key, a tag or a token. The
//!   counters carry counts only.
//! - It never writes into a peer's profile. Every refusal here is a
//!   **local** condition about this router's own state.

use std::time::Duration;

use i2pr_transport::PeerId;

use crate::relay_requester::RelayEstablishment;

/// The capacity of the handoff between the requester owner and the
/// establishment's task.
///
/// The handoff is bounded: a router whose task cannot keep up refuses the
/// establishments it cannot take under a counted reason, and the material
/// goes back to the owner. Nothing is queued without a bound.
pub const RELAY_ESTABLISHMENT_QUEUE: usize = 8;

/// Why the router did not establish the session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RelayEstablishmentRefusal {
    /// The operator left the requester role's opt-in off.
    Disabled,
    /// The target's validated record is not in the authoritative store.
    TargetUnknown,
    /// The target's record carries no SSU2 address.
    TargetNoSsu2Address,
    /// The target's SSU2 address does not parse.
    TargetUnparsableAddress,
    /// The target's SSU2 address names no introduction key.
    TargetNoIntroKey,
    /// The dial target is outside the qualification profile.
    DialTargetRefused,
    /// The handshake did not complete.
    HandshakeFailed,
    /// The establishment was cancelled.
    Cancelled,
}

impl RelayEstablishmentRefusal {
    /// A stable, count-only reason key for this refusal.
    ///
    /// The key is a label only: it carries no peer identity, no address,
    /// no key and no token. It exists so a refusal can be counted by
    /// reason.
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::TargetUnknown => "target-unknown",
            Self::TargetNoSsu2Address => "target-no-ssu2-address",
            Self::TargetUnparsableAddress => "target-unparsable-address",
            Self::TargetNoIntroKey => "target-no-intro-key",
            Self::DialTargetRefused => "dial-target-refused",
            Self::HandshakeFailed => "handshake-failed",
            Self::Cancelled => "cancelled",
        }
    }
}

/// The shared cell the establishment's task publishes into.
///
/// The cell holds counts only: it names no peer, no address, no key and
/// no token.
pub type RelayEstablishmentCounterCell = std::sync::Mutex<RelayEstablishmentCounters>;

/// The owner's counters, one per outcome.
///
/// Every refusal and every outcome is counted under its own reason, so a
/// caller can observe what happened instead of inferring it. The counters
/// are counts only: they name no peer, no address, no key and no token.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RelayEstablishmentCounters {
    /// Establishments taken from the handoff.
    pub establishments_received: u64,
    /// Establishments that reached an authenticated session.
    pub establishments_established: u64,
    /// Refusals because the operator left the opt-in off.
    pub refusals_disabled: u64,
    /// Refusals because the target's record is not in the store.
    pub refusals_target_unknown: u64,
    /// Refusals because the target's record carries no SSU2 address.
    pub refusals_target_no_ssu2_address: u64,
    /// Refusals because the target's SSU2 address does not parse.
    pub refusals_target_unparsable_address: u64,
    /// Refusals because the target's SSU2 address names no introduction
    /// key.
    pub refusals_target_no_intro_key: u64,
    /// Refusals because the dial target is outside the profile.
    pub refusals_dial_target_refused: u64,
    /// Refusals because the handshake did not complete.
    pub refusals_handshake_failed: u64,
    /// Refusals because the establishment was cancelled.
    pub refusals_cancelled: u64,
}

/// The bounded owner of this router's relay establishments.
///
/// The owner holds no socket and dials nothing itself: it prepares the
/// dial target, hands the material to the runtime's establishment path
/// and records every outcome.
pub struct RelayEstablishmentOwner {
    /// The operator's opt-in. A disabled owner refuses every
    /// establishment.
    enabled: bool,
    counters: RelayEstablishmentCounters,
}

impl std::fmt::Debug for RelayEstablishmentOwner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RelayEstablishmentOwner")
            .field("enabled", &self.enabled)
            .finish()
    }
}

impl RelayEstablishmentOwner {
    /// Creates an owner.
    ///
    /// `enabled` is the operator's opt-in. A disabled owner refuses every
    /// establishment, so a router that has not asked to reach a peer
    /// through a relay never dials one.
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            counters: RelayEstablishmentCounters::default(),
        }
    }

    /// Whether the operator enabled this router's establishments.
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    /// Returns the owner's counters.
    pub const fn counters(&self) -> RelayEstablishmentCounters {
        self.counters
    }

    /// Records one refusal under its own reason.
    pub fn refuse(&mut self, reason: RelayEstablishmentRefusal) {
        match reason {
            RelayEstablishmentRefusal::Disabled => self.counters.refusals_disabled += 1,
            RelayEstablishmentRefusal::TargetUnknown => {
                self.counters.refusals_target_unknown += 1;
            }
            RelayEstablishmentRefusal::TargetNoSsu2Address => {
                self.counters.refusals_target_no_ssu2_address += 1;
            }
            RelayEstablishmentRefusal::TargetUnparsableAddress => {
                self.counters.refusals_target_unparsable_address += 1;
            }
            RelayEstablishmentRefusal::TargetNoIntroKey => {
                self.counters.refusals_target_no_intro_key += 1;
            }
            RelayEstablishmentRefusal::DialTargetRefused => {
                self.counters.refusals_dial_target_refused += 1;
            }
            RelayEstablishmentRefusal::HandshakeFailed => {
                self.counters.refusals_handshake_failed += 1;
            }
            RelayEstablishmentRefusal::Cancelled => self.counters.refusals_cancelled += 1,
        }
    }
}

/// What the owner did with one establishment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RelayEstablishmentOutcome {
    /// The peer was reached: the session is authenticated.
    Established,
    /// The establishment was refused, with the reason.
    Refused(RelayEstablishmentRefusal),
}

/// Prepares the dial target for one establishment.
///
/// The target's static and introduction keys come from its **validated**
/// RouterInfo, read through the transport's strict address parser, and
/// the **address** comes from the accept — the target's own bound
/// endpoint, authenticated by the target's signature over the preimage
/// that carries it.
///
/// The address is then checked against the qualification profile exactly
/// as every other dial in the tree is, so the relay establishment can
/// never widen the router's reach: the controlled loopback profile stays
/// loopback-only and the private-LAN variant stays RFC1918 IPv4.
///
/// The read never mutates the store and never reaches outside it.
pub fn prepare_relay_establishment(
    store: &i2pr_netdb::RouterInfoStore,
    establishment: &RelayEstablishment,
    profile: crate::config::Ssu2QualificationProfile,
) -> Result<i2pr_runtime::Ssu2DialTarget, RelayEstablishmentRefusal> {
    let key = i2pr_netdb::RouterHash::from_bytes(establishment.target);
    let Some(record) = store.get(&key) else {
        return Err(RelayEstablishmentRefusal::TargetUnknown);
    };
    let info = record.router_info();
    let Some(address) = info
        .addresses()
        .iter()
        .find(|address| address.transport_style() == "SSU2")
    else {
        return Err(RelayEstablishmentRefusal::TargetNoSsu2Address);
    };
    let Ok(parsed) = i2pr_runtime::Ssu2RouterAddress::parse(address) else {
        return Err(RelayEstablishmentRefusal::TargetUnparsableAddress);
    };
    let Some(intro) = parsed.intro_key() else {
        return Err(RelayEstablishmentRefusal::TargetNoIntroKey);
    };
    let Ok(static_public) =
        i2pr_runtime::Ssu2PublicKey::new(*parsed.static_public_key().as_bytes())
    else {
        return Err(RelayEstablishmentRefusal::TargetUnparsableAddress);
    };
    let peer_hash = PeerId::from_bytes(establishment.target).hash();
    crate::router_i2np::daemon_dial_target_for_profile(
        peer_hash,
        establishment.endpoint.socket_addr(),
        static_public,
        i2pr_runtime::IntroKey::new(*intro.as_bytes()),
        profile,
    )
    .map_err(|_| RelayEstablishmentRefusal::DialTargetRefused)
}

/// Drives one establishment to an authenticated session.
///
/// The establishment is the runtime's **ordinary** establishment path: the
/// accept's token is presented in the initiator's Session Request and the
/// target's own responder consumes it from the table it was issued into.
/// No relay-specific session is created and the responder's verification
/// is never bypassed.
///
/// The dial is awaited with **no** store lock held, so a slow handshake
/// can never hold the router's NetDB.
pub async fn drive_relay_establishment(
    owner: &mut RelayEstablishmentOwner,
    target: i2pr_runtime::Ssu2DialTarget,
    token: u64,
    service: &i2pr_runtime::Ssu2RuntimeService,
    dial_timeout: Duration,
    cancellation: &i2pr_runtime::CancellationToken,
) -> RelayEstablishmentOutcome {
    if !owner.enabled {
        owner.refuse(RelayEstablishmentRefusal::Disabled);
        return RelayEstablishmentOutcome::Refused(RelayEstablishmentRefusal::Disabled);
    }
    owner.counters.establishments_received += 1;
    match service
        .dial_ssu2_with_token(target, token, dial_timeout, cancellation)
        .await
    {
        Ok(_) => {
            owner.counters.establishments_established += 1;
            RelayEstablishmentOutcome::Established
        }
        Err(i2pr_runtime::Ssu2DialOutcome::Cancelled) => {
            owner.refuse(RelayEstablishmentRefusal::Cancelled);
            RelayEstablishmentOutcome::Refused(RelayEstablishmentRefusal::Cancelled)
        }
        Err(_) => {
            owner.refuse(RelayEstablishmentRefusal::HandshakeFailed);
            RelayEstablishmentOutcome::Refused(RelayEstablishmentRefusal::HandshakeFailed)
        }
    }
}
