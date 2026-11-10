//! Plan 532 — the router answers a relay introduction when it is the target.
//!
//! The SSU2 relay protocol has three roles. The **introducer** answers a
//! relay request and forwards an introduction. The **requester** asks an
//! introducer to reach a peer it cannot reach directly (Plan 531). The
//! **target** is the peer that is reached: it receives the introduction,
//! verifies it, and **answers** it.
//!
//! The target role was the last one with no producer. The transport's own
//! target state machine was complete — it verifies the introducer context
//! against the specification's preimage, enforces the global quota,
//! suppresses a replayed introduction and expires a pending one — and the
//! runtime held the table and exposed the seam that drives it, but nothing
//! in production called the seam, and the introduction itself was routed to
//! the introducer's handoff and refused as not its role.
//!
//! This module owns the **policy** half of the target role. It:
//!
//! - refuses every introduction while the operator's opt-in is off;
//! - reads the **requester's** validated RouterInfo out of the authoritative
//!   store, and never parses unvalidated bytes;
//! - takes the requester's signing key and introduction key from that
//!   validated record, through the transport's strict address parser;
//! - drives the runtime's existing target seam, so the transport's state
//!   machine is used and not re-implemented;
//! - signs the accept with the router's **own** identity key, over the
//!   introducer-bound preimage, and sends it to the **introducer**;
//! - punches the hole to the requester **out of session**, through the
//!   runtime's existing bounded staging queue;
//! - and counts every outcome under its own reason, so a reader observes
//!   what happened instead of inferring it.
//!
//! The owner holds **no task**, **no timer**, **no listener** and **no
//! socket**. The daemon's product tick drives it, exactly as it drives
//! Plan 531's requester owner and Plan 524's introducer owner, and the
//! runtime owns the sockets, the session, the bounded tables and the
//! transport's verification.
//!
//! # What answering an introduction means
//!
//! Nothing about the router's own reachability changes. A router that
//! answers an introduction is reachable **through the relay**, not
//! directly, and the relay's success **contradicts** direct reachability
//! under the transport's own model. This plan therefore records **no**
//! corroborating class and writes **no** reachability signal of its own:
//! the daemon never writes into the reachability tracker.
//!
//! # What this module never does
//!
//! - It never advertises anything. The target is opt-in behind
//!   `ssu2.relay_target` and stays non-advertised.
//! - It never logs a peer hash, an address, a tag value, a key or a
//!   payload. The counters carry counts only.
//! - It never holds an unbounded table. The transport's own target table
//!   owns the pending introductions; this module keeps none.

/// Why the router did not answer a relay introduction.
///
/// Every refusal is a distinct variant and is counted under its own
/// reason. None of them writes anything into a peer's profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RelayTargetRefusal {
    /// The operator left the opt-in off.
    Disabled,
    /// The router names no bound endpoint, so it cannot present one.
    NoLocalEndpoint,
    /// The requester's validated record is not in the authoritative store.
    RequesterUnknown,
    /// The requester's record carries no SSU2 address.
    RequesterNoSsu2Address,
    /// The requester's SSU2 address does not parse.
    RequesterUnparsableAddress,
    /// The requester's SSU2 address names no introduction key.
    RequesterNoIntroKey,
    /// The introduction was a replay of one already answered.
    Replay,
    /// The accept could not be signed with the router's own key.
    SigningRefused,
    /// The transport refused the introduction or the response.
    TransportRefused,
    /// The carriage refused to put the accept on a session.
    CarriageRefused,
    /// The forwarded signature did not verify.
    InvalidSignature,
    /// The timestamp was outside the permitted skew.
    StaleTimestamp,
    /// The target's table is full.
    TooManyRequests,
    /// The router's own randomness produced no usable token.
    NoRandomness,
    /// The hole punch's destination is outside the router's dial profile.
    DestinationRefused,
    /// The runtime's staging queue refused the hole punch.
    StagingRefused,
}

impl RelayTargetRefusal {
    /// A stable, count-only reason key for this refusal.
    ///
    /// The key is a label only: it carries no peer identity, no address,
    /// no nonce and no payload. It exists so a refusal can be counted by
    /// reason.
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::NoLocalEndpoint => "no-local-endpoint",
            Self::RequesterUnknown => "requester-unknown",
            Self::RequesterNoSsu2Address => "requester-no-ssu2-address",
            Self::RequesterUnparsableAddress => "requester-unparsable-address",
            Self::RequesterNoIntroKey => "requester-no-intro-key",
            Self::Replay => "replay",
            Self::SigningRefused => "signing-refused",
            Self::TransportRefused => "transport-refused",
            Self::CarriageRefused => "carriage-refused",
            Self::InvalidSignature => "invalid-signature",
            Self::StaleTimestamp => "stale-timestamp",
            Self::TooManyRequests => "too-many-requests",
            Self::NoRandomness => "no-randomness",
            Self::DestinationRefused => "destination-refused",
            Self::StagingRefused => "staging-refused",
        }
    }
}

/// The owner's counters, one per outcome.
///
/// Every refusal and every outcome is counted under its own reason, so a
/// caller can observe what happened instead of inferring it. The counters
/// are counts only: they name no peer, no address, no key, no nonce and no
/// tag.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RelayTargetCounters {
    /// Introductions received on the target's handoff.
    pub intros_received: u64,
    /// Introductions the transport's own target table verified.
    pub intros_verified: u64,
    /// Introductions answered: the accept was queued and the hole punch
    /// was staged.
    pub intros_answered: u64,
    /// Accepts put on the introducer's session.
    pub accepts_queued: u64,
    /// Hole punches staged toward the requester.
    pub hole_punches_staged: u64,
    /// Replayed introductions the transport suppressed.
    pub replays_suppressed: u64,
    /// The operator left the opt-in off.
    pub refusals_disabled: u64,
    /// The router names no bound endpoint.
    pub refusals_no_local_endpoint: u64,
    /// The requester's validated record is not in the store.
    pub refusals_requester_unknown: u64,
    /// The requester's record carries no SSU2 address.
    pub refusals_requester_no_ssu2_address: u64,
    /// The requester's SSU2 address does not parse.
    pub refusals_requester_unparsable_address: u64,
    /// The requester's SSU2 address names no introduction key.
    pub refusals_requester_no_intro_key: u64,
    /// The introduction was a replay.
    pub refusals_replay: u64,
    /// The accept could not be signed.
    pub refusals_signing_refused: u64,
    /// The transport refused the introduction or the response.
    pub refusals_transport_refused: u64,
    /// The carriage refused to put the accept on a session.
    pub refusals_carriage_refused: u64,
    /// The forwarded signature did not verify.
    pub refusals_invalid_signature: u64,
    /// The timestamp was outside the permitted skew.
    pub refusals_stale_timestamp: u64,
    /// The target's table is full.
    pub refusals_too_many_requests: u64,
    /// The router's own randomness produced no usable token.
    pub refusals_no_randomness: u64,
    /// The hole punch's destination is outside the router's dial profile.
    pub refusals_destination_refused: u64,
    /// The runtime's staging queue refused the hole punch.
    pub refusals_staging_refused: u64,
}

/// What the owner did with one introduction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RelayTargetOutcome {
    /// The introduction was verified and answered: the accept is on the
    /// introducer's session and the hole punch is staged toward the
    /// requester.
    Answered,
    /// The introduction was a replay of one already answered, and the
    /// transport suppressed it. Nothing was emitted.
    ReplaySuppressed,
    /// The introduction was refused, with the reason.
    Refused(RelayTargetRefusal),
}

/// The signer of the target's accept.
///
/// The seed never leaves the router's identity bundle: the seam signs one
/// preimage and returns the signature bytes, so no caller can copy the
/// seed into a struct, a log or an assertion.
pub trait RelayTargetSigner: Send + Sync {
    /// Signs the specification's RelayResponse preimage.
    ///
    /// Returns `None` when the key cannot sign. A partial or malformed
    /// signature is never returned.
    fn sign_relay_response(&self, preimage: &[u8]) -> Option<Vec<u8>>;
}

impl RelayTargetSigner for crate::relay_requester::LocalIdentitySigner {
    fn sign_relay_response(&self, preimage: &[u8]) -> Option<Vec<u8>> {
        self.sign(preimage)
    }
}

/// The bounded owner of this router's relay introductions.
///
/// The owner holds no introduction table: the transport's own target table
/// owns the pending introductions and is the authority on a replay. The
/// owner reads the requester's validated record, drives the runtime's
/// target seam, signs the accept and hands the hole punch's pieces to the
/// runtime's outbound seam.
pub struct RelayTargetOwner {
    /// The operator's opt-in. A disabled owner refuses every
    /// introduction, so a router that has not asked to answer a relay
    /// introduction never answers one.
    enabled: bool,
    /// The runtime that owns the sessions, the tables and the transport's
    /// verification.
    runtime: i2pr_runtime::Ssu2RuntimeService,
    /// The router's own signing key.
    signer: std::sync::Arc<dyn RelayTargetSigner>,
    counters: RelayTargetCounters,
}

impl std::fmt::Debug for RelayTargetOwner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RelayTargetOwner")
            .field("enabled", &self.enabled)
            .finish()
    }
}

impl RelayTargetOwner {
    /// Creates an owner.
    ///
    /// `enabled` is the operator's opt-in. A disabled owner refuses every
    /// introduction, so a router that has not asked to answer a relay
    /// introduction never answers one.
    pub fn new(
        enabled: bool,
        runtime: i2pr_runtime::Ssu2RuntimeService,
        signer: std::sync::Arc<dyn RelayTargetSigner>,
    ) -> Self {
        Self {
            enabled,
            runtime,
            signer,
            counters: RelayTargetCounters::default(),
        }
    }

    /// Returns the owner's counters.
    pub const fn counters(&self) -> &RelayTargetCounters {
        &self.counters
    }

    /// Records one refusal under its own reason.
    pub fn refuse(&mut self, reason: RelayTargetRefusal) {
        match reason {
            RelayTargetRefusal::Disabled => self.counters.refusals_disabled += 1,
            RelayTargetRefusal::NoLocalEndpoint => {
                self.counters.refusals_no_local_endpoint += 1;
            }
            RelayTargetRefusal::RequesterUnknown => {
                self.counters.refusals_requester_unknown += 1;
            }
            RelayTargetRefusal::RequesterNoSsu2Address => {
                self.counters.refusals_requester_no_ssu2_address += 1;
            }
            RelayTargetRefusal::RequesterUnparsableAddress => {
                self.counters.refusals_requester_unparsable_address += 1;
            }
            RelayTargetRefusal::RequesterNoIntroKey => {
                self.counters.refusals_requester_no_intro_key += 1;
            }
            RelayTargetRefusal::Replay => self.counters.refusals_replay += 1,
            RelayTargetRefusal::SigningRefused => {
                self.counters.refusals_signing_refused += 1;
            }
            RelayTargetRefusal::TransportRefused => {
                self.counters.refusals_transport_refused += 1;
            }
            RelayTargetRefusal::CarriageRefused => {
                self.counters.refusals_carriage_refused += 1;
            }
            RelayTargetRefusal::InvalidSignature => {
                self.counters.refusals_invalid_signature += 1;
            }
            RelayTargetRefusal::StaleTimestamp => {
                self.counters.refusals_stale_timestamp += 1;
            }
            RelayTargetRefusal::TooManyRequests => {
                self.counters.refusals_too_many_requests += 1;
            }
            RelayTargetRefusal::NoRandomness => {
                self.counters.refusals_no_randomness += 1;
            }
            RelayTargetRefusal::DestinationRefused => {
                self.counters.refusals_destination_refused += 1;
            }
            RelayTargetRefusal::StagingRefused => {
                self.counters.refusals_staging_refused += 1;
            }
        }
    }
}

/// The requester's validated material, read out of the authoritative
/// store.
///
/// The requester's **signing** key verifies the forwarded signature and
/// its **introduction** key seals the hole punch, so both come from the
/// requester's validated RouterInfo. The read never mutates the store and
/// never reaches outside it.
pub struct RequesterMaterial {
    /// The requester's signing public key.
    pub signing_key: i2pr_proto::SigningPublicKey,
    /// The requester's introduction key.
    pub intro_key: i2pr_runtime::IntroKey,
}

/// Reads the requester's validated material from the authoritative store.
///
/// Returns the typed refusal rather than a guess: a requester this router
/// does not hold is a **local** condition, and it is never written into
/// the requester's profile.
pub fn read_requester_material(
    store: &i2pr_netdb::RouterInfoStore,
    requester: [u8; 32],
) -> Result<RequesterMaterial, RelayTargetRefusal> {
    let key = i2pr_netdb::RouterHash::from_bytes(requester);
    let Some(record) = store.get(&key) else {
        return Err(RelayTargetRefusal::RequesterUnknown);
    };
    let Some(address) = record
        .router_info()
        .addresses()
        .iter()
        .find(|address| address.transport_style() == "SSU2")
    else {
        return Err(RelayTargetRefusal::RequesterNoSsu2Address);
    };
    let Ok(parsed) = i2pr_runtime::Ssu2RouterAddress::parse(address) else {
        return Err(RelayTargetRefusal::RequesterUnparsableAddress);
    };
    let Some(intro_key) = parsed.intro_key() else {
        return Err(RelayTargetRefusal::RequesterNoIntroKey);
    };
    // The address's introduction key and the transport's sealing key are
    // the same 32 bytes in two shapes: the address validates them, the
    // transport seals with them. The conversion is the only place the two
    // meet.
    Ok(RequesterMaterial {
        signing_key: record.router_info().router_identity().signing_key().clone(),
        intro_key: i2pr_runtime::IntroKey::new(*intro_key.as_bytes()),
    })
}

/// Answers one relay introduction.
///
/// The owner verifies the introduction through the runtime's own target
/// seam — the transport's state machine, used and not re-implemented — and
/// then answers it: the accept is signed with the router's own identity
/// key over the introducer-bound preimage and queued on the introducer's
/// session, and the hole punch is staged toward the requester out of
/// session.
///
/// Every refusal is typed and counted, and none of them writes anything
/// into a peer's profile.
pub fn drive_relay_target(
    owner: &mut RelayTargetOwner,
    store: &i2pr_netdb::RouterInfoStore,
    event: &i2pr_runtime::Ssu2InboundTarget,
    rng: &mut rand_chacha::ChaCha8Rng,
    now_secs: u64,
    now_ms: u64,
) -> RelayTargetOutcome {
    if !owner.enabled {
        owner.refuse(RelayTargetRefusal::Disabled);
        return RelayTargetOutcome::Refused(RelayTargetRefusal::Disabled);
    }
    owner.counters.intros_received += 1;

    let block = &event.block;
    let requester = *block.alice_hash();
    let material = match read_requester_material(store, requester) {
        Ok(material) => material,
        Err(reason) => {
            owner.refuse(reason);
            return RelayTargetOutcome::Refused(reason);
        }
    };
    let Some(own_endpoint) = owner.runtime.local_bound_endpoint() else {
        owner.refuse(RelayTargetRefusal::NoLocalEndpoint);
        return RelayTargetOutcome::Refused(RelayTargetRefusal::NoLocalEndpoint);
    };

    // The introducer is the peer the introduction arrived on, and this
    // router is the target it named.
    let introducer = *event.peer.hash().as_bytes();
    let target = owner.runtime.local_router_hash();

    match owner.runtime.on_target_relay_intro(
        block,
        &introducer,
        &target,
        &material.signing_key,
        now_secs,
        now_ms,
    ) {
        Ok(false) => {
            owner.counters.replays_suppressed += 1;
            owner.refuse(RelayTargetRefusal::Replay);
            RelayTargetOutcome::ReplaySuppressed
        }
        Ok(true) => {
            owner.counters.intros_verified += 1;
            answer_introduction(owner, event, &material, own_endpoint, rng, now_secs)
        }
        Err(error) => {
            let reason = map_relay_error(error);
            owner.refuse(reason);
            RelayTargetOutcome::Refused(reason)
        }
    }
}

/// Signs and emits the answer to a verified introduction.
fn answer_introduction(
    owner: &mut RelayTargetOwner,
    event: &i2pr_runtime::Ssu2InboundTarget,
    material: &RequesterMaterial,
    own_endpoint: i2pr_runtime::Ssu2Endpoint,
    rng: &mut rand_chacha::ChaCha8Rng,
    now_secs: u64,
) -> RelayTargetOutcome {
    let block = &event.block;
    let nonce = block.nonce();
    let introducer = *event.peer.hash().as_bytes();
    let Ok(timestamp) = u32::try_from(now_secs) else {
        owner.refuse(RelayTargetRefusal::NoRandomness);
        return RelayTargetOutcome::Refused(RelayTargetRefusal::NoRandomness);
    };
    // The accept is bound to the **introducer** that forwarded the
    // introduction: the preimage names the introducer's hash, and the
    // introducer forwards the accept verbatim.
    let preimage = i2pr_runtime::relay_response_preimage(
        &introducer,
        nonce,
        timestamp,
        block.version(),
        Some(own_endpoint),
    );
    let Some(signature) = owner.signer.sign_relay_response(&preimage) else {
        owner.refuse(RelayTargetRefusal::SigningRefused);
        return RelayTargetOutcome::Refused(RelayTargetRefusal::SigningRefused);
    };
    let Some(token) = draw_nonzero_token(rng) else {
        owner.refuse(RelayTargetRefusal::NoRandomness);
        return RelayTargetOutcome::Refused(RelayTargetRefusal::NoRandomness);
    };
    let Ok(accept) = i2pr_runtime::RelayResponseBlock::accept(
        nonce,
        timestamp,
        block.version(),
        own_endpoint,
        signature,
        token,
    ) else {
        owner.refuse(RelayTargetRefusal::TransportRefused);
        return RelayTargetOutcome::Refused(RelayTargetRefusal::TransportRefused);
    };

    if owner
        .runtime
        .queue_relay_response(event.peer, accept.clone())
        .is_err()
    {
        owner.refuse(RelayTargetRefusal::CarriageRefused);
        return RelayTargetOutcome::Refused(RelayTargetRefusal::CarriageRefused);
    }
    owner.counters.accepts_queued += 1;

    match owner.runtime.stage_relay_hole_punch(
        &material.intro_key,
        nonce,
        own_endpoint,
        block.endpoint(),
        accept,
        now_secs,
    ) {
        Ok(()) => {
            owner.counters.hole_punches_staged += 1;
            owner.counters.intros_answered += 1;
            // The introduction is answered: the transport's pending entry
            // is released, so a replay cannot produce a second emission.
            owner.runtime.consume_target_intro(nonce);
            RelayTargetOutcome::Answered
        }
        Err(error) => {
            let reason = map_target_error(error);
            owner.refuse(reason);
            RelayTargetOutcome::Refused(reason)
        }
    }
}

/// Draws a fresh nonzero correlation token.
///
/// The token is the requester's handle for the establishment that follows
/// the hole punch. It is drawn from the router's own randomness and is
/// never zero: the transport refuses a zero token.
fn draw_nonzero_token(rng: &mut rand_chacha::ChaCha8Rng) -> Option<u64> {
    use rand_core::TryRngCore;
    for _ in 0..8 {
        let mut bytes = [0_u8; 8];
        rng.try_fill_bytes(&mut bytes).ok()?;
        let token = u64::from_be_bytes(bytes);
        if token != 0 {
            return Some(token);
        }
    }
    None
}

/// Maps one transport refusal onto the owner's typed reason.
///
/// The transport's own errors are the authority; the owner names them
/// under its own reasons so a reader sees one vocabulary.
fn map_relay_error(error: i2pr_runtime::RelayError) -> RelayTargetRefusal {
    match error {
        i2pr_runtime::RelayError::InvalidSignature
        | i2pr_runtime::RelayError::UnsupportedSigner => RelayTargetRefusal::InvalidSignature,
        i2pr_runtime::RelayError::StaleTimestamp => RelayTargetRefusal::StaleTimestamp,
        i2pr_runtime::RelayError::TooManyRequests => RelayTargetRefusal::TooManyRequests,
        _ => RelayTargetRefusal::TransportRefused,
    }
}

/// Maps one runtime refusal onto the owner's typed reason.
fn map_target_error(error: i2pr_runtime::Ssu2RelayTargetError) -> RelayTargetRefusal {
    match error {
        i2pr_runtime::Ssu2RelayTargetError::DestinationRefused => {
            RelayTargetRefusal::DestinationRefused
        }
        i2pr_runtime::Ssu2RelayTargetError::NoLocalEndpoint => RelayTargetRefusal::NoLocalEndpoint,
        i2pr_runtime::Ssu2RelayTargetError::TransportRefused => {
            RelayTargetRefusal::TransportRefused
        }
        i2pr_runtime::Ssu2RelayTargetError::NoRandomness => RelayTargetRefusal::NoRandomness,
        i2pr_runtime::Ssu2RelayTargetError::StagingRefused => RelayTargetRefusal::StagingRefused,
    }
}
