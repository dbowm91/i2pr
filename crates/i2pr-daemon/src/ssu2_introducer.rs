//! The daemon-owned SSU2 introducer (Bob) role (Plan 490).
//!
//! The router can act as an *introducer*: a peer behind a firewall
//! asks this router for a relay tag, publishes it, and a requester that
//! can reach this router presents that tag to name the target. The
//! specification's relay process is:
//!
//! ```text
//! requester ──Relay Request──▶ this router ──Relay Intro──▶ target
//! requester ◀─Relay Response── this router ◀─Relay Response── target
//! ```
//!
//! The relay tag is the only in-band link between the requester and the
//! target, so the role owner resolves the target through the tag before
//! it verifies the requester's signature over that target's hash.
//!
//! # Ownership
//!
//! - The **runtime** owns the socket, the bounded tables, and the
//!   transport's verification. It hands the daemon decoded, session
//!   authenticated relay blocks over a bounded channel.
//! - The **daemon** owns the policy (admission, per-source rate limit,
//!   the requester routing table) and the receipts.
//!
//! # What this module never does
//!
//! - It never advertises anything. The role is opt-in behind
//!   `ssu2.introducer_service` and stays non-advertised.
//! - It never logs a peer hash, an address, a tag value, or a session
//!   key. The receipt carries counts only.
//! - It never holds an unbounded table. Every table has a fixed
//!   capacity and evicts deterministically.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use i2pr_proto::SigningPublicKey;
use i2pr_runtime::{
    IntroducerResponseOutcome, RelayError, RelayIntroBlock, RelayRequestBlock, RelayResponseBlock,
    Ssu2InboundRelay, Ssu2RelayEvent, Ssu2RuntimeService,
};

/// Maximum relay requests one source may make in one minute.
pub const MAX_INTRODUCER_REQUESTS_PER_MINUTE: u32 = 60;

/// Maximum tracked introductions (one per live relay request).
pub const MAX_INTRODUCER_TRACKED_REQUESTS: usize = 64;

/// Maximum retained per-source rate windows.
pub const MAX_INTRODUCER_RATE_SOURCES: usize = 64;

/// The one-minute window the per-source rate limit is measured over.
pub const INTRODUCER_RATE_WINDOW_MS: u64 = 60_000;

/// The maximum on-wire size of one Relay Request block.
///
/// The transport's anti-amplification budget is derived from the
/// request's size. The role owner uses the protocol's upper bound so
/// the budget can never be under-counted for a short request.
pub const MAX_INTRODUCER_REQUEST_BYTES: usize = 128;

/// The bounded policy the role owner enforces.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ssu2IntroducerConfig {
    /// Whether the role runs at all. The daemon's
    /// `ssu2.introducer_service` key (default off) is the only gate.
    pub enabled: bool,
    /// Per-source request ceiling inside one rate window.
    pub max_requests_per_minute: u32,
    /// Maximum tracked introductions.
    pub max_tracked_requests: usize,
}

impl Default for Ssu2IntroducerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_requests_per_minute: MAX_INTRODUCER_REQUESTS_PER_MINUTE,
            max_tracked_requests: MAX_INTRODUCER_TRACKED_REQUESTS,
        }
    }
}

/// The typed reason a relay event was refused.
///
/// Every refusal is counted in the receipt and leaves no state behind.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Ssu2IntroducerRefusal {
    /// The role is switched off.
    Disabled,
    /// The block belongs to another role (this router is not the
    /// target side).
    NotOurRole,
    /// The source exceeded its per-minute ceiling.
    RateLimited,
    /// The relay tag is unknown to this router.
    UnknownTag,
    /// The relay tag has expired.
    ExpiredTag,
    /// The requester's signing key is unknown, so its signature cannot
    /// be verified.
    UnknownRequester,
    /// The introduction target is unknown.
    TargetUnknown,
    /// The signature did not verify.
    InvalidSignature,
    /// The timestamp was outside the permitted skew.
    StaleTimestamp,
    /// The request was a replay of a live one.
    DuplicateRequest,
    /// The tracked-introduction table is full.
    BudgetExhausted,
    /// The transport refused the message.
    TransportRefused,
    /// The response did not correlate with a live introduction.
    UnknownCorrelation,
    /// The response came from a peer that is not the introduction
    /// target.
    SenderMismatch,
    /// The transport's role is disabled.
    ServiceDisabled,
}

impl Ssu2IntroducerRefusal {
    /// Returns the stable receipt key for this refusal.
    ///
    /// The key is a static label: it never carries a peer, an address,
    /// or a tag value.
    pub const fn receipt_key(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::NotOurRole => "not_our_role",
            Self::RateLimited => "rate_limited",
            Self::UnknownTag => "unknown_tag",
            Self::ExpiredTag => "expired_tag",
            Self::UnknownRequester => "unknown_requester",
            Self::TargetUnknown => "target_unknown",
            Self::InvalidSignature => "invalid_signature",
            Self::StaleTimestamp => "stale_timestamp",
            Self::DuplicateRequest => "duplicate_request",
            Self::BudgetExhausted => "budget_exhausted",
            Self::TransportRefused => "transport_refused",
            Self::UnknownCorrelation => "unknown_correlation",
            Self::SenderMismatch => "sender_mismatch",
            Self::ServiceDisabled => "service_disabled",
        }
    }
}

/// The typed outcome of one inbound relay event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Ssu2IntroducerOutcome {
    /// The role is switched off; the event was refused without state.
    Refused {
        /// The typed refusal reason.
        reason: Ssu2IntroducerRefusal,
    },
    /// A Relay Request was admitted. A Relay Intro must go to the
    /// named target.
    IntroRequired {
        /// The introduction target the relay tag identified.
        target: [u8; 32],
        /// The Relay Intro to send to the target.
        intro: RelayIntroBlock,
    },
    /// A Relay Response was verified. It must be forwarded to the
    /// requester.
    ResponseRequired {
        /// The requester the response belongs to.
        requester: [u8; 32],
        /// The Relay Response to forward verbatim.
        response: RelayResponseBlock,
    },
}

impl Ssu2IntroducerOutcome {
    /// Returns the refusal reason, when this outcome is a refusal.
    pub const fn refusal(&self) -> Option<Ssu2IntroducerRefusal> {
        match self {
            Self::Refused { reason } => Some(*reason),
            _ => None,
        }
    }
}

/// The privacy-safe receipt for the introducer role.
///
/// Counts only: no peer hash, no address, no tag value, no key, no
/// payload.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Ssu2IntroducerReceipt {
    /// Relay requests admitted.
    pub requests_admitted: u64,
    /// Relay intros produced.
    pub intros_produced: u64,
    /// Relay responses verified and forwarded.
    pub responses_forwarded: u64,
    /// Relay tags issued.
    pub tags_issued: u64,
    /// Refusals by typed reason.
    pub refusals: BTreeMap<&'static str, u64>,
}

impl Ssu2IntroducerReceipt {
    fn refuse(&mut self, reason: Ssu2IntroducerRefusal) {
        let counter = self.refusals.entry(reason.receipt_key()).or_insert(0);
        *counter = counter.saturating_add(1);
    }
}

/// One live introduction: the requester the response must reach.
#[derive(Clone, Copy, Debug)]
struct TrackedIntroduction {
    requester: [u8; 32],
    target: [u8; 32],
    deadline_ms: u64,
}

/// A bounded per-source request window.
#[derive(Clone, Copy, Debug, Default)]
struct SourceWindow {
    window_start_ms: u64,
    count: u32,
}

/// Resolves the signing key of a peer from a validated RouterInfo.
///
/// The daemon's role owner never trusts a key that arrived on the wire:
/// the key comes from a RouterInfo the NetDb already validated.
pub trait IntroducerSignerResolver {
    /// Returns the peer's signing key, or `None` when the peer is
    /// unknown or its RouterInfo is not validated.
    fn signing_key(&self, peer_hash: &[u8; 32]) -> Option<SigningPublicKey>;
}

/// The daemon's production [`IntroducerSignerResolver`].
///
/// Plan 498: the resolver answers **only** from the NetDB's validated
/// store. A peer this NetDB does not hold yields `None`, and the role
/// owner then refuses the relay block with
/// [`Ssu2IntroducerRefusal::UnknownRequester`] rather than guessing.
///
/// The bootstrap's lock is a `std::sync::Mutex`. The critical section is
/// one map lookup and it never awaits: the guard is created and dropped
/// inside the synchronous [`IntroducerSignerResolver::signing_key`] call.
pub struct NetDbSignerResolver {
    bootstrap: Arc<Mutex<crate::bootstrap::Bootstrap>>,
}

impl NetDbSignerResolver {
    /// Wraps the shared bootstrap owner.
    pub fn new(bootstrap: Arc<Mutex<crate::bootstrap::Bootstrap>>) -> Self {
        Self { bootstrap }
    }
}

impl IntroducerSignerResolver for NetDbSignerResolver {
    fn signing_key(&self, peer_hash: &[u8; 32]) -> Option<SigningPublicKey> {
        let hash = i2pr_netdb::RouterHash::from_bytes(*peer_hash);
        // A poisoned lock is fail-closed: the answer is `None`, so the role
        // owner refuses the block instead of admitting an unverifiable peer.
        let bootstrap = self.bootstrap.lock().ok()?;
        bootstrap.validated_signing_key(&hash)
    }
}

/// The daemon's SSU2 introducer (Bob) role owner.
///
/// The owner applies the admission policy and the per-source rate limit,
/// drives the runtime's `RelayIntroducer`, and emits a typed outcome
/// plus a privacy-safe receipt.
pub struct Ssu2IntroducerService {
    config: Ssu2IntroducerConfig,
    runtime: Ssu2RuntimeService,
    local_hash: [u8; 32],
    tracked: HashMap<u32, TrackedIntroduction>,
    sources: HashMap<[u8; 32], SourceWindow>,
    receipt: Ssu2IntroducerReceipt,
}

impl Ssu2IntroducerService {
    /// Creates a role owner.
    ///
    /// The role does not start when `config.enabled` is false: every
    /// inbound event is refused with [`Ssu2IntroducerRefusal::Disabled`]
    /// and no state is created.
    pub fn new(
        config: Ssu2IntroducerConfig,
        runtime: Ssu2RuntimeService,
        local_hash: [u8; 32],
    ) -> Self {
        Self {
            config,
            runtime,
            local_hash,
            tracked: HashMap::new(),
            sources: HashMap::new(),
            receipt: Ssu2IntroducerReceipt::default(),
        }
    }

    /// Returns the privacy-safe receipt.
    pub const fn receipt(&self) -> &Ssu2IntroducerReceipt {
        &self.receipt
    }

    /// Returns whether the role is enabled.
    pub const fn enabled(&self) -> bool {
        self.config.enabled
    }

    /// Handles one inbound relay event from the runtime handoff.
    ///
    /// The event is already session authenticated and decoded: the
    /// runtime owns that. The owner applies admission, the per-source
    /// rate limit, and the transport's verification, and returns the
    /// typed outcome. Every refusal leaves no state behind.
    pub fn on_relay_event(
        &mut self,
        event: &Ssu2InboundRelay,
        signers: &dyn IntroducerSignerResolver,
        now_ms: u64,
        now_secs: u64,
    ) -> Ssu2IntroducerOutcome {
        if !self.config.enabled {
            return self.refuse(Ssu2IntroducerRefusal::Disabled);
        }
        match &event.event {
            Ssu2RelayEvent::Request(block) => self.on_relay_request(
                block,
                *event.peer.hash().as_bytes(),
                signers,
                now_ms,
                now_secs,
            ),
            Ssu2RelayEvent::Response(block) => self.on_relay_response(
                block,
                *event.peer.hash().as_bytes(),
                signers,
                now_ms,
                now_secs,
            ),
            // The Relay Intro is the target side's message. This router
            // is the introducer, so it is not our role.
            Ssu2RelayEvent::Intro(_) => self.refuse(Ssu2IntroducerRefusal::NotOurRole),
        }
    }

    fn on_relay_request(
        &mut self,
        block: &RelayRequestBlock,
        requester: [u8; 32],
        signers: &dyn IntroducerSignerResolver,
        now_ms: u64,
        now_secs: u64,
    ) -> Ssu2IntroducerOutcome {
        // The per-source rate limit comes first: a flood must not reach
        // the cryptographic path at all.
        if !self.admit_source(requester, now_ms) {
            return self.refuse(Ssu2IntroducerRefusal::RateLimited);
        }
        // The relay tag identifies the introduction target.
        let Some(target) = self.runtime.relay_tag_owner(block.relay_tag(), now_secs) else {
            return self.refuse(Ssu2IntroducerRefusal::UnknownTag);
        };
        // The requester's own key comes from a validated RouterInfo.
        let Some(signer) = signers.signing_key(&requester) else {
            return self.refuse(Ssu2IntroducerRefusal::UnknownRequester);
        };
        if self.tracked.len() >= self.config.max_tracked_requests {
            self.expire_tracked(now_ms);
            if self.tracked.len() >= self.config.max_tracked_requests {
                return self.refuse(Ssu2IntroducerRefusal::BudgetExhausted);
            }
        }
        // The transport verifies the requester's signature over the
        // specification's preimage, which binds the target's hash, and
        // enforces the tag binding, the freshness window and the
        // response-size budget.
        let admitted = match self.runtime.on_relay_request(
            block,
            &requester,
            &self.local_hash,
            &target,
            &signer,
            MAX_INTRODUCER_REQUEST_BYTES,
            now_secs,
            now_ms,
        ) {
            Ok(admitted) => admitted,
            Err(error) => return self.refuse(map_relay_error(error)),
        };
        if !admitted {
            // The transport refused to amplify a replayed request.
            return self.refuse(Ssu2IntroducerRefusal::DuplicateRequest);
        }
        let intro = RelayIntroBlock::new(
            requester,
            block.nonce(),
            block.relay_tag(),
            block.timestamp(),
            block.version(),
            block.endpoint(),
            block.signature().to_vec(),
        );
        let intro = match intro {
            Ok(intro) => intro,
            Err(_) => return self.refuse(Ssu2IntroducerRefusal::TransportRefused),
        };
        self.tracked.insert(
            block.nonce(),
            TrackedIntroduction {
                requester,
                target,
                deadline_ms: now_ms.saturating_add(INTRODUCER_RATE_WINDOW_MS),
            },
        );
        self.receipt.requests_admitted = self.receipt.requests_admitted.saturating_add(1);
        self.receipt.intros_produced = self.receipt.intros_produced.saturating_add(1);
        Ssu2IntroducerOutcome::IntroRequired { target, intro }
    }

    fn on_relay_response(
        &mut self,
        block: &RelayResponseBlock,
        sender: [u8; 32],
        signers: &dyn IntroducerSignerResolver,
        now_ms: u64,
        now_secs: u64,
    ) -> Ssu2IntroducerOutcome {
        self.expire_tracked(now_ms);
        let Some(tracked) = self.tracked.get(&block.nonce()).copied() else {
            return self.refuse(Ssu2IntroducerRefusal::UnknownCorrelation);
        };
        if tracked.target != sender {
            return self.refuse(Ssu2IntroducerRefusal::SenderMismatch);
        }
        let Some(signer) = signers.signing_key(&sender) else {
            return self.refuse(Ssu2IntroducerRefusal::UnknownRequester);
        };
        let outcome = match self.runtime.on_introducer_relay_response(
            block,
            &sender,
            &self.local_hash,
            &signer,
            now_secs,
            now_ms,
        ) {
            Ok(outcome) => outcome,
            Err(error) => return self.refuse(map_relay_error(error)),
        };
        match outcome {
            IntroducerResponseOutcome::Forwarded { .. } => {}
            IntroducerResponseOutcome::UnknownRequest => {
                return self.refuse(Ssu2IntroducerRefusal::UnknownCorrelation);
            }
        }
        self.tracked.remove(&block.nonce());
        self.receipt.responses_forwarded = self.receipt.responses_forwarded.saturating_add(1);
        Ssu2IntroducerOutcome::ResponseRequired {
            requester: tracked.requester,
            response: block.clone(),
        }
    }

    /// Issues one relay tag that identifies `target`.
    ///
    /// The target asks the introducer for a tag and publishes it; a
    /// requester presents the tag to name the target. The tag value
    /// comes from the caller's randomness (production: the OS CSPRNG).
    pub fn issue_tag(
        &mut self,
        target: [u8; 32],
        tag: u32,
        now_secs: u64,
    ) -> Result<u32, Ssu2IntroducerRefusal> {
        if !self.config.enabled {
            return Err(Ssu2IntroducerRefusal::Disabled);
        }
        self.runtime
            .issue_relay_tag(tag, &target, now_secs)
            .map_err(|_| Ssu2IntroducerRefusal::TransportRefused)?;
        self.receipt.tags_issued = self.receipt.tags_issued.saturating_add(1);
        Ok(tag)
    }

    /// Applies the per-source rate limit.
    ///
    /// The window is one minute. The source table is bounded: when it
    /// is full the oldest window is dropped deterministically.
    fn admit_source(&mut self, source: [u8; 32], now_ms: u64) -> bool {
        // The source key is the peer hash; no address is retained, so a
        // receipt can never leak one.
        if self.sources.len() >= MAX_INTRODUCER_RATE_SOURCES && !self.sources.contains_key(&source)
        {
            // Evict the oldest window rather than clearing the table: a
            // flood that cycles many peers must not reset everyone
            // else's allowance. The eviction is deterministic — the
            // oldest window start wins, and the lower peer hash breaks
            // a tie.
            let oldest = self
                .sources
                .iter()
                .min_by(|(left_hash, left), (right_hash, right)| {
                    left.window_start_ms
                        .cmp(&right.window_start_ms)
                        .then_with(|| left_hash.cmp(right_hash))
                })
                .map(|(hash, _)| *hash);
            if let Some(hash) = oldest {
                self.sources.remove(&hash);
            }
        }
        let window = self.sources.entry(source).or_default();
        if now_ms.saturating_sub(window.window_start_ms) >= INTRODUCER_RATE_WINDOW_MS {
            window.window_start_ms = now_ms;
            window.count = 0;
        }
        if window.count >= self.config.max_requests_per_minute {
            return false;
        }
        window.count = window.count.saturating_add(1);
        true
    }

    fn expire_tracked(&mut self, now_ms: u64) {
        self.tracked
            .retain(|_, tracked| tracked.deadline_ms > now_ms);
    }

    fn refuse(&mut self, reason: Ssu2IntroducerRefusal) -> Ssu2IntroducerOutcome {
        self.receipt.refuse(reason);
        Ssu2IntroducerOutcome::Refused { reason }
    }
}

fn map_relay_error(error: RelayError) -> Ssu2IntroducerRefusal {
    match error {
        RelayError::ServiceDisabled => Ssu2IntroducerRefusal::ServiceDisabled,
        RelayError::InvalidTag => Ssu2IntroducerRefusal::UnknownTag,
        RelayError::InvalidSignature | RelayError::UnsupportedSigner => {
            Ssu2IntroducerRefusal::InvalidSignature
        }
        RelayError::StaleTimestamp => Ssu2IntroducerRefusal::StaleTimestamp,
        RelayError::DuplicateCorrelation => Ssu2IntroducerRefusal::DuplicateRequest,
        RelayError::UnknownCorrelation => Ssu2IntroducerRefusal::UnknownCorrelation,
        RelayError::WrongRole => Ssu2IntroducerRefusal::NotOurRole,
        RelayError::SenderMismatch => Ssu2IntroducerRefusal::SenderMismatch,
        RelayError::TooManyRequests
        | RelayError::PeerQuotaExceeded
        | RelayError::TooManyTags
        | RelayError::TagQuotaExceeded => Ssu2IntroducerRefusal::BudgetExhausted,
        _ => Ssu2IntroducerRefusal::TransportRefused,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::net::{IpAddr, Ipv4Addr};

    use i2pr_crypto::{RouterIdentityBundle, SigningPrivateKey};
    use i2pr_proto::Hash;
    use i2pr_runtime::{
        Ssu2Endpoint, Ssu2RuntimeConfig, relay_request_preimage, relay_response_preimage,
    };
    use rand_chacha::ChaCha8Rng;
    use rand_core::SeedableRng;

    const TARGET_HASH: [u8; 32] = [0x7A; 32];
    const REQUESTER_HASH: [u8; 32] = [0xC1; 32];
    const OTHER_HASH: [u8; 32] = [0xDE; 32];
    const NOW_MS: u64 = 1_000;
    const NOW_SECS: u64 = 1_000;

    /// Builds a runtime with a valid controlled identity and returns it
    /// with the router's own hash.
    fn runtime(enabled: bool) -> (Ssu2RuntimeService, [u8; 32]) {
        let mut rng = ChaCha8Rng::seed_from_u64(0x490);
        let bundle = RouterIdentityBundle::generate(&mut rng).expect("identity");
        let identity =
            crate::router_i2np::generate_controlled_identity(&bundle, "127.0.0.1", 34_567)
                .expect("ssu2 identity");
        let local = *identity.router_hash.as_bytes();
        let config = Ssu2RuntimeConfig {
            introducer_service: enabled,
            ..Default::default()
        };
        let service =
            Ssu2RuntimeService::new_with_bandwidth(config, identity, None).expect("runtime");
        (service, local)
    }

    fn key(byte: u8) -> SigningPrivateKey {
        SigningPrivateKey::from_bytes([byte; 32])
    }

    #[derive(Default)]
    struct TestSigners {
        keys: HashMap<[u8; 32], SigningPublicKey>,
    }

    impl IntroducerSignerResolver for TestSigners {
        fn signing_key(&self, peer_hash: &[u8; 32]) -> Option<SigningPublicKey> {
            self.keys.get(peer_hash).cloned()
        }
    }

    fn signers() -> TestSigners {
        let mut signers = TestSigners::default();
        signers.keys.insert(
            REQUESTER_HASH,
            key(0x11).public_key().expect("requester public"),
        );
        signers
            .keys
            .insert(TARGET_HASH, key(0x22).public_key().expect("target public"));
        signers
    }

    fn endpoint() -> Ssu2Endpoint {
        Ssu2Endpoint::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 9_999).expect("endpoint")
    }

    fn request(
        bob: &[u8; 32],
        nonce: u32,
        tag: u32,
        timestamp: u32,
        signer: &SigningPrivateKey,
    ) -> RelayRequestBlock {
        let preimage =
            relay_request_preimage(bob, &TARGET_HASH, nonce, tag, timestamp, 2, endpoint());
        let signature = signer.sign(&preimage).expect("sign");
        let signature = signature.as_bytes().to_vec();
        RelayRequestBlock::new(nonce, tag, timestamp, 2, endpoint(), signature).expect("request")
    }

    fn accept_response(
        bob: &[u8; 32],
        nonce: u32,
        timestamp: u32,
        signer: &SigningPrivateKey,
    ) -> RelayResponseBlock {
        let preimage = relay_response_preimage(bob, nonce, timestamp, 2, Some(endpoint()));
        let signature = signer.sign(&preimage).expect("sign");
        let signature = signature.as_bytes().to_vec();
        RelayResponseBlock::accept(nonce, timestamp, 2, endpoint(), signature, 0xABCD)
            .expect("accept")
    }

    fn inbound(peer: [u8; 32], event: Ssu2RelayEvent) -> Ssu2InboundRelay {
        Ssu2InboundRelay {
            link_id: i2pr_transport::LinkId::new(1).expect("link id"),
            peer: i2pr_transport::PeerId::from_hash(Hash::from_bytes(peer)),
            event,
        }
    }

    fn owner(enabled: bool) -> (Ssu2IntroducerService, [u8; 32]) {
        let (runtime, local) = runtime(enabled);
        let owner = Ssu2IntroducerService::new(
            Ssu2IntroducerConfig {
                enabled,
                ..Default::default()
            },
            runtime,
            local,
        );
        (owner, local)
    }

    fn issue_tag(owner: &mut Ssu2IntroducerService, target: [u8; 32], tag: u32) -> u32 {
        owner.issue_tag(target, tag, NOW_SECS).expect("tag")
    }

    #[test]
    fn the_role_does_not_start_when_the_key_is_off() {
        let (mut owner, bob) = owner(false);
        let event = inbound(
            REQUESTER_HASH,
            Ssu2RelayEvent::Request(request(&bob, 1, 7, NOW_SECS as u32, &key(0x11))),
        );
        let outcome = owner.on_relay_event(&event, &signers(), NOW_MS, NOW_SECS);
        assert_eq!(
            outcome,
            Ssu2IntroducerOutcome::Refused {
                reason: Ssu2IntroducerRefusal::Disabled
            }
        );
        assert_eq!(owner.receipt().requests_admitted, 0);
        assert_eq!(owner.receipt().intros_produced, 0);
        assert_eq!(owner.receipt().refusals.get("disabled"), Some(&1));
        // A disabled role issues no tag either.
        assert_eq!(
            owner.issue_tag(TARGET_HASH, 7, NOW_SECS),
            Err(Ssu2IntroducerRefusal::Disabled)
        );
        assert_eq!(owner.receipt().tags_issued, 0);
    }

    #[test]
    fn an_admitted_request_produces_a_relay_intro_for_the_tagged_target() {
        let (mut owner, bob) = owner(true);
        issue_tag(&mut owner, TARGET_HASH, 7);
        let event = inbound(
            REQUESTER_HASH,
            Ssu2RelayEvent::Request(request(&bob, 21, 7, NOW_SECS as u32, &key(0x11))),
        );
        let outcome = owner.on_relay_event(&event, &signers(), NOW_MS, NOW_SECS);
        let Ssu2IntroducerOutcome::IntroRequired { target, intro } = outcome else {
            panic!("expected an intro, got {outcome:?}");
        };
        assert_eq!(target, TARGET_HASH);
        assert_eq!(intro.alice_hash(), &REQUESTER_HASH);
        assert_eq!(intro.nonce(), 21);
        assert_eq!(intro.relay_tag(), 7);
        assert_eq!(owner.receipt().requests_admitted, 1);
        assert_eq!(owner.receipt().intros_produced, 1);
    }

    #[test]
    fn an_unknown_relay_tag_is_refused_without_state() {
        let (mut owner, bob) = owner(true);
        let event = inbound(
            REQUESTER_HASH,
            Ssu2RelayEvent::Request(request(&bob, 22, 9, NOW_SECS as u32, &key(0x11))),
        );
        let outcome = owner.on_relay_event(&event, &signers(), NOW_MS, NOW_SECS);
        assert_eq!(
            outcome,
            Ssu2IntroducerOutcome::Refused {
                reason: Ssu2IntroducerRefusal::UnknownTag
            }
        );
        assert_eq!(owner.receipt().intros_produced, 0);
    }

    #[test]
    fn a_request_from_an_unknown_requester_is_refused() {
        let (mut owner, bob) = owner(true);
        issue_tag(&mut owner, TARGET_HASH, 7);
        let event = inbound(
            OTHER_HASH,
            Ssu2RelayEvent::Request(request(&bob, 23, 7, NOW_SECS as u32, &key(0x33))),
        );
        let outcome = owner.on_relay_event(&event, &signers(), NOW_MS, NOW_SECS);
        assert_eq!(
            outcome,
            Ssu2IntroducerOutcome::Refused {
                reason: Ssu2IntroducerRefusal::UnknownRequester
            }
        );
        assert_eq!(owner.receipt().intros_produced, 0);
    }

    #[test]
    fn a_request_with_a_bad_signature_is_refused() {
        let (mut owner, bob) = owner(true);
        issue_tag(&mut owner, TARGET_HASH, 7);
        // Signed by a key that is not the requester's.
        let event = inbound(
            REQUESTER_HASH,
            Ssu2RelayEvent::Request(request(&bob, 24, 7, NOW_SECS as u32, &key(0x44))),
        );
        let outcome = owner.on_relay_event(&event, &signers(), NOW_MS, NOW_SECS);
        assert_eq!(
            outcome,
            Ssu2IntroducerOutcome::Refused {
                reason: Ssu2IntroducerRefusal::InvalidSignature
            }
        );
        assert_eq!(owner.receipt().intros_produced, 0);
    }

    #[test]
    fn a_stale_request_is_refused() {
        let (mut owner, bob) = owner(true);
        issue_tag(&mut owner, TARGET_HASH, 7);
        let stale = (NOW_SECS as u32).saturating_sub(3_600);
        let event = inbound(
            REQUESTER_HASH,
            Ssu2RelayEvent::Request(request(&bob, 25, 7, stale, &key(0x11))),
        );
        let outcome = owner.on_relay_event(&event, &signers(), NOW_MS, NOW_SECS);
        assert_eq!(
            outcome,
            Ssu2IntroducerOutcome::Refused {
                reason: Ssu2IntroducerRefusal::StaleTimestamp
            }
        );
    }

    #[test]
    fn the_transit_role_is_refused_by_the_service_owner() {
        let (mut owner, _bob) = owner(true);
        let intro = RelayIntroBlock::new(
            REQUESTER_HASH,
            1,
            7,
            NOW_SECS as u32,
            2,
            endpoint(),
            vec![0x55; 64],
        )
        .expect("intro");
        let event = inbound(TARGET_HASH, Ssu2RelayEvent::Intro(intro));
        let outcome = owner.on_relay_event(&event, &signers(), NOW_MS, NOW_SECS);
        assert_eq!(
            outcome,
            Ssu2IntroducerOutcome::Refused {
                reason: Ssu2IntroducerRefusal::NotOurRole
            }
        );
    }

    #[test]
    fn the_per_source_rate_limit_refuses_a_flood() {
        let (runtime, bob) = runtime(true);
        let mut owner = Ssu2IntroducerService::new(
            Ssu2IntroducerConfig {
                enabled: true,
                max_requests_per_minute: 2,
                max_tracked_requests: MAX_INTRODUCER_TRACKED_REQUESTS,
            },
            runtime,
            bob,
        );
        issue_tag(&mut owner, TARGET_HASH, 7);
        for nonce in 1..=2u32 {
            let event = inbound(
                REQUESTER_HASH,
                Ssu2RelayEvent::Request(request(&bob, nonce, 7, NOW_SECS as u32, &key(0x11))),
            );
            let outcome = owner.on_relay_event(&event, &signers(), NOW_MS, NOW_SECS);
            assert!(
                matches!(outcome, Ssu2IntroducerOutcome::IntroRequired { .. }),
                "the first two requests must be admitted, got {outcome:?}"
            );
        }
        let event = inbound(
            REQUESTER_HASH,
            Ssu2RelayEvent::Request(request(&bob, 3, 7, NOW_SECS as u32, &key(0x11))),
        );
        let outcome = owner.on_relay_event(&event, &signers(), NOW_MS, NOW_SECS);
        assert_eq!(
            outcome,
            Ssu2IntroducerOutcome::Refused {
                reason: Ssu2IntroducerRefusal::RateLimited
            }
        );
        assert_eq!(owner.receipt().refusals.get("rate_limited"), Some(&1));
    }

    #[test]
    fn the_rate_limiter_releases_its_state_on_expiry() {
        let (mut owner, bob) = owner(true);
        issue_tag(&mut owner, TARGET_HASH, 7);
        let mut config = Ssu2IntroducerConfig {
            enabled: true,
            max_requests_per_minute: 1,
            max_tracked_requests: MAX_INTRODUCER_TRACKED_REQUESTS,
        };
        config.max_requests_per_minute = 1;
        let mut limited = Ssu2IntroducerService::new(config, owner.runtime.clone(), bob);
        let event = inbound(
            REQUESTER_HASH,
            Ssu2RelayEvent::Request(request(&bob, 51, 7, NOW_SECS as u32, &key(0x11))),
        );
        assert!(matches!(
            limited.on_relay_event(&event, &signers(), NOW_MS, NOW_SECS),
            Ssu2IntroducerOutcome::IntroRequired { .. }
        ));
        let event = inbound(
            REQUESTER_HASH,
            Ssu2RelayEvent::Request(request(&bob, 52, 7, NOW_SECS as u32, &key(0x11))),
        );
        assert_eq!(
            limited.on_relay_event(&event, &signers(), NOW_MS, NOW_SECS),
            Ssu2IntroducerOutcome::Refused {
                reason: Ssu2IntroducerRefusal::RateLimited
            }
        );
        // The window rolls over: the same source is admitted again.
        let later = NOW_MS + INTRODUCER_RATE_WINDOW_MS;
        let event = inbound(
            REQUESTER_HASH,
            Ssu2RelayEvent::Request(request(&bob, 53, 7, NOW_SECS as u32, &key(0x11))),
        );
        assert!(
            matches!(
                limited.on_relay_event(&event, &signers(), later, NOW_SECS),
                Ssu2IntroducerOutcome::IntroRequired { .. }
            ),
            "the rate window must release on expiry"
        );
    }

    #[test]
    fn the_rate_table_evicts_the_oldest_window_without_resetting_the_others() {
        let (mut owner, bob) = owner(true);
        issue_tag(&mut owner, TARGET_HASH, 7);
        let mut limited = Ssu2IntroducerService::new(
            Ssu2IntroducerConfig {
                enabled: true,
                max_requests_per_minute: 1,
                max_tracked_requests: MAX_INTRODUCER_TRACKED_REQUESTS,
            },
            owner.runtime.clone(),
            bob,
        );
        // Fill the source table with distinct peers, one request each.
        for index in 0..MAX_INTRODUCER_RATE_SOURCES as u32 {
            let peer = [index as u8 + 1; 32];
            let mut signers = signers();
            signers
                .keys
                .insert(peer, key(index as u8 + 1).public_key().expect("signer"));
            let event = inbound(
                peer,
                Ssu2RelayEvent::Request(request(
                    &bob,
                    60 + index,
                    7,
                    NOW_SECS as u32,
                    &key(index as u8 + 1),
                )),
            );
            limited.on_relay_event(&event, &signers, NOW_MS, NOW_SECS);
        }
        assert_eq!(limited.sources.len(), MAX_INTRODUCER_RATE_SOURCES);
        // A new peer evicts one window instead of clearing the table.
        let fresh = [0xEE; 32];
        let mut signers = signers();
        signers
            .keys
            .insert(fresh, key(0xEE).public_key().expect("signer"));
        let event = inbound(
            fresh,
            Ssu2RelayEvent::Request(request(&bob, 200, 7, NOW_SECS as u32, &key(0xEE))),
        );
        limited.on_relay_event(&event, &signers, NOW_MS, NOW_SECS);
        assert_eq!(limited.sources.len(), MAX_INTRODUCER_RATE_SOURCES);
    }

    #[test]
    fn a_verified_response_is_forwarded_to_the_requester() {
        let (mut owner, bob) = owner(true);
        issue_tag(&mut owner, TARGET_HASH, 7);
        let event = inbound(
            REQUESTER_HASH,
            Ssu2RelayEvent::Request(request(&bob, 31, 7, NOW_SECS as u32, &key(0x11))),
        );
        assert!(matches!(
            owner.on_relay_event(&event, &signers(), NOW_MS, NOW_SECS),
            Ssu2IntroducerOutcome::IntroRequired { .. }
        ));
        let response = accept_response(&bob, 31, NOW_SECS as u32, &key(0x22));
        let event = inbound(TARGET_HASH, Ssu2RelayEvent::Response(response));
        let outcome = owner.on_relay_event(&event, &signers(), NOW_MS, NOW_SECS);
        let Ssu2IntroducerOutcome::ResponseRequired { requester, .. } = outcome else {
            panic!("expected a forward, got {outcome:?}");
        };
        assert_eq!(requester, REQUESTER_HASH);
        assert_eq!(owner.receipt().responses_forwarded, 1);
    }

    #[test]
    fn a_replayed_response_is_refused() {
        let (mut owner, bob) = owner(true);
        issue_tag(&mut owner, TARGET_HASH, 7);
        let event = inbound(
            REQUESTER_HASH,
            Ssu2RelayEvent::Request(request(&bob, 32, 7, NOW_SECS as u32, &key(0x11))),
        );
        owner.on_relay_event(&event, &signers(), NOW_MS, NOW_SECS);
        let response = accept_response(&bob, 32, NOW_SECS as u32, &key(0x22));
        let event = inbound(TARGET_HASH, Ssu2RelayEvent::Response(response.clone()));
        assert!(matches!(
            owner.on_relay_event(&event, &signers(), NOW_MS, NOW_SECS),
            Ssu2IntroducerOutcome::ResponseRequired { .. }
        ));
        let event = inbound(TARGET_HASH, Ssu2RelayEvent::Response(response));
        let outcome = owner.on_relay_event(&event, &signers(), NOW_MS, NOW_SECS);
        assert_eq!(
            outcome,
            Ssu2IntroducerOutcome::Refused {
                reason: Ssu2IntroducerRefusal::UnknownCorrelation
            }
        );
        assert_eq!(owner.receipt().responses_forwarded, 1);
    }

    #[test]
    fn a_response_from_another_peer_is_refused() {
        let (mut owner, bob) = owner(true);
        issue_tag(&mut owner, TARGET_HASH, 7);
        let event = inbound(
            REQUESTER_HASH,
            Ssu2RelayEvent::Request(request(&bob, 33, 7, NOW_SECS as u32, &key(0x11))),
        );
        owner.on_relay_event(&event, &signers(), NOW_MS, NOW_SECS);
        let response = accept_response(&bob, 33, NOW_SECS as u32, &key(0x22));
        let event = inbound(OTHER_HASH, Ssu2RelayEvent::Response(response));
        let outcome = owner.on_relay_event(&event, &signers(), NOW_MS, NOW_SECS);
        assert_eq!(
            outcome,
            Ssu2IntroducerOutcome::Refused {
                reason: Ssu2IntroducerRefusal::SenderMismatch
            }
        );
    }

    #[test]
    fn the_receipt_carries_no_peer_hash_no_tag_value_and_no_address() {
        // A distinctive tag value: the receipt carries a *count* of
        // issued tags, never the value, and never a peer or an address.
        const TAG: u32 = 0xABCD;
        let (mut owner, bob) = owner(true);
        issue_tag(&mut owner, TARGET_HASH, TAG);
        let event = inbound(
            REQUESTER_HASH,
            Ssu2RelayEvent::Request(request(&bob, 41, TAG, NOW_SECS as u32, &key(0x11))),
        );
        owner.on_relay_event(&event, &signers(), NOW_MS, NOW_SECS);
        let rendered = format!("{:?}", owner.receipt());
        assert!(
            !rendered.contains("43981"),
            "the receipt must not carry the tag value: {rendered}"
        );
        for forbidden in ["0xC1", "0x7A", "127.0.0.1", "9999"] {
            assert!(
                !rendered.contains(forbidden),
                "the receipt must not carry `{forbidden}`: {rendered}"
            );
        }
        // The counts are present: the receipt proves the role ran.
        assert_eq!(owner.receipt().requests_admitted, 1);
        assert_eq!(owner.receipt().intros_produced, 1);
        assert_eq!(owner.receipt().tags_issued, 1);
    }
}
