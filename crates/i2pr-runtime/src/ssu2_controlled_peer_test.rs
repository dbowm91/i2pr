//! Bounded controlled peer-test evidence driver (Plan 283).
//!
//! This module runs one complete three-party SSU2 peer-test exchange on
//! loopback UDP and returns Alice's typed table outcome. Alice is always
//! the caller's live [`Ssu2RuntimeService`](crate::ssu2_runtime::Ssu2RuntimeService)
//! (the daemon's service on the controlled path); Bob and Charlie are
//! ephemeral helper services built from fresh OS-random identities that
//! never persist, never publish, and are shut down on every exit path.
//!
//! Every corroborating fact is wire-real: all seven peer-test messages
//! cross real UDP datagrams between three distinct bound sockets,
//! sessions authenticate every in-session block, intro-key AEAD
//! authenticates every out-of-session block, and every table transition
//! passes the Plan 160 correlation/role/sender/freshness/signature
//! gates. The driver never substitutes an endpoint it did not genuinely
//! observe: Msgs 1–5 carry the configured claim under test (fixed at
//! `start_controlled_peer_test` like every table), while the verdict
//! Msg 7 carries exactly what Charlie observed as Alice's source.
//!
//! Signature preimage convention (mirrors the transport tests):
//! Alice-signed blocks (Msgs 1, 2, 6) omit `ahash`; Charlie-signed
//! blocks (Msgs 3–5, 7) include it; Bob signs only the opaque Msg 2
//! forward. The service ingress verifies to the same convention.
//!
//! Transport separation is structural: Msgs 1–4 travel in-session
//! through live handshakes the driver establishes first; Msgs 5–7
//! travel out-of-session sealed under the receiver's public intro key.
//! No wire format is added or changed; the driver only orchestrates
//! retained Plan 158–160 machinery.
//!
//! Failure is fail-closed: any refused call, missed deadline,
//! cancelled step, or non-`Confirmed` outcome aborts the exchange
//! without recording reachability evidence (only a real `Confirmed`
//! ingest records `PeerTestResult{Confirmed}`, inside the service).
//! The general production peer-test driver (unsolicited tests,
//! production RouterInfo plumbing, persistent helpers) is explicitly
//! out of scope and stays unbuilt.

use std::net::SocketAddr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use i2pr_crypto::{RouterIdentityBundle, X25519PrivateKey};
use i2pr_proto::{Date, Hash, Mapping, RouterAddress, SigningPublicKey};
use i2pr_transport::{AddressFamily, PeerId};
use i2pr_transport_ssu2::{
    IntroKey, PeerTestBlock, PeerTestOutcome, PeerTestRole, PeerTestState, Ssu2Endpoint,
    Ssu2PublicKey, constants, peer_test_preimage,
};
use rand_core::{OsRng, TryRngCore};

use crate::ssu2_runtime::{
    ControlledPeerTestError, Ssu2DialOutcome, Ssu2DialTarget, Ssu2IdentityMaterial,
    Ssu2RuntimeConfig, Ssu2RuntimeService, Ssu2ServiceHandle, Ssu2SocketConfig,
};
use crate::{CancellationToken, ChildScope};

/// Upper bound for one driver step (handshake or message round).
const MAX_STEP_TIMEOUT: Duration = Duration::from_secs(120);
/// Fixed cost value for helper RouterInfos (never dialed by address).
const HELPER_INFO_COST: u8 = 10;
/// Placeholder SSU2 port inside helper RouterInfos (peers dial the
/// live bound address from the driver, never the advertised port).
const HELPER_INFO_PORT: &str = "43000";

/// Alice's message signer: signs peer-test preimages with her router
/// key. The secret never crosses into the driver: only signatures of
/// peer-test preimages travel back.
pub type ControlledPeerTestSigner<'a> =
    &'a (dyn Fn(&[u8]) -> Result<Vec<u8>, ControlledPeerTestError> + 'a);

/// What the controlled exchange proved about Alice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlledPeerTestOutcome {
    /// The full 4+5+7 quorum confirmed Alice at her bound endpoint
    /// with two corroborating peers.
    Confirmed {
        /// Tested address family.
        family: AddressFamily,
        /// Confirmed endpoint (always Alice's bound socket).
        observed: SocketAddr,
        /// Corroborating peers behind the outcome.
        evidence_peers: u8,
    },
}

/// Inputs for one controlled peer-test exchange.
///
/// Alice is the caller's live service; every Alice-named field must
/// describe that same service (its bound address, its transport keys,
/// its signing key). The driver checks the bindings it can
/// (`start_controlled_peer_test` enforces the endpoint and hash
/// bindings inside the service); the caller is responsible for
/// passing its own material.
pub struct ControlledPeerTestParams<'a> {
    /// Owner scope for the ephemeral helper loop tasks.
    pub scope: &'a ChildScope,
    /// Alice: the live service under test.
    pub alice: &'a Ssu2RuntimeService,
    /// Alice's canonical router hash.
    pub alice_hash: [u8; 32],
    /// Alice's bound socket address under test.
    pub alice_addr: SocketAddr,
    /// Alice's transport static public key (for helper dial targets).
    pub alice_static_public: Ssu2PublicKey,
    /// Alice's intro key (seals Msgs 5/7 to Alice).
    pub alice_intro: IntroKey,
    /// Alice's signing public key (registered for Msg 6 verification).
    pub alice_signing_public: SigningPublicKey,
    /// Signs Alice's Msg 1 and Msg 6 preimages with her router key.
    /// The secret never crosses into the driver: only signatures of
    /// peer-test preimages travel back.
    pub alice_sign: ControlledPeerTestSigner<'a>,
    /// Cooperative cancellation checked between steps and in dials.
    pub cancellation: &'a CancellationToken,
    /// Per-step deadline for handshakes and message rounds.
    pub step_timeout: Duration,
    /// Poll interval while awaiting wire/table transitions.
    pub poll_interval: Duration,
}

/// Ephemeral helper identity (OS-random, never persisted, dropped
/// with the driver; the bundle zeroizes its secrets on drop).
struct HelperKeys {
    bundle: RouterIdentityBundle,
    hash_bytes: [u8; 32],
    peer: PeerId,
    static_public: Ssu2PublicKey,
    static_bytes: [u8; 32],
    intro: IntroKey,
    router_info: Vec<u8>,
}

/// One ephemeral helper service with its bound address.
struct Helper {
    keys: HelperKeys,
    service: Ssu2RuntimeService,
    #[allow(dead_code)]
    handle: Ssu2ServiceHandle,
    addr: SocketAddr,
}

fn wall_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(1_700_000_000)
}

fn wall_timestamp() -> u32 {
    wall_secs().min(u64::from(u32::MAX)) as u32
}

fn i2p_b64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-~";
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

fn make_helper_keys() -> Result<HelperKeys, ControlledPeerTestError> {
    let bundle = RouterIdentityBundle::generate(&mut OsRng)
        .map_err(|_| ControlledPeerTestError::ExchangeCrypto)?;
    let hash = bundle
        .identity()
        .hash()
        .map_err(|_| ControlledPeerTestError::ExchangeCrypto)?;
    let hash_bytes = *hash.as_bytes();
    let static_key = X25519PrivateKey::generate(&mut OsRng)
        .map_err(|_| ControlledPeerTestError::ExchangeCrypto)?;
    let static_bytes = *static_key.secret_bytes();
    let static_public = Ssu2PublicKey::new(static_key.public_bytes())
        .map_err(|_| ControlledPeerTestError::ExchangeCrypto)?;
    let mut intro_bytes = [0_u8; 32];
    for _ in 0..8 {
        OsRng
            .try_fill_bytes(&mut intro_bytes)
            .map_err(|_| ControlledPeerTestError::ExchangeCrypto)?;
        if intro_bytes.iter().any(|byte| *byte != 0) {
            break;
        }
    }
    if intro_bytes.iter().all(|byte| *byte == 0) {
        return Err(ControlledPeerTestError::ExchangeCrypto);
    }
    let intro = IntroKey::new(intro_bytes);
    let options = Mapping::from_entries(vec![
        ("host".to_string(), "127.0.0.1".to_string()),
        ("port".to_string(), HELPER_INFO_PORT.to_string()),
        ("v".to_string(), "2".to_string()),
        ("s".to_string(), i2p_b64_encode(&static_key.public_bytes())),
        ("i".to_string(), i2p_b64_encode(&intro_bytes)),
    ])
    .map_err(|_| ControlledPeerTestError::ExchangeCrypto)?;
    let address = RouterAddress::new(
        HELPER_INFO_COST,
        Date::from_millis(9_999_999_999_999),
        "SSU2".to_string(),
        options,
    )
    .map_err(|_| ControlledPeerTestError::ExchangeCrypto)?;
    let info = bundle
        .sign_router_info(
            Date::from_millis(wall_secs().saturating_mul(1000)),
            vec![address],
            Vec::new(),
            Mapping::empty(),
        )
        .map_err(|_| ControlledPeerTestError::ExchangeCrypto)?;
    let router_info = info
        .encode_to_vec(constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
        .map_err(|_| ControlledPeerTestError::ExchangeCrypto)?;
    Ok(HelperKeys {
        bundle,
        hash_bytes,
        peer: PeerId::from_hash(hash),
        static_public,
        static_bytes,
        intro,
        router_info,
    })
}

fn sign_with(
    bundle: &RouterIdentityBundle,
    message: u8,
    bob_hash: &[u8; 32],
    alice_hash: Option<&[u8; 32]>,
    nonce: u32,
    timestamp: u32,
    endpoint: Ssu2Endpoint,
) -> Result<Vec<u8>, ControlledPeerTestError> {
    let preimage = peer_test_preimage(
        message,
        bob_hash,
        alice_hash,
        constants::SSU2_VERSION,
        nonce,
        timestamp,
        endpoint,
    );
    bundle
        .signing_key()
        .sign(&preimage)
        .map(|signature| signature.as_bytes().to_vec())
        .map_err(|_| ControlledPeerTestError::ExchangeCrypto)
}

fn peer_test_block(
    message: u8,
    router_hash: Option<[u8; 32]>,
    nonce: u32,
    timestamp: u32,
    endpoint: Ssu2Endpoint,
    signature: Vec<u8>,
) -> Result<PeerTestBlock, ControlledPeerTestError> {
    PeerTestBlock::new(
        message,
        0,
        router_hash,
        constants::SSU2_VERSION,
        nonce,
        timestamp,
        endpoint,
        signature,
    )
    .map_err(|_| ControlledPeerTestError::ExchangeCrypto)
}

fn loopback_socket_config() -> Ssu2SocketConfig {
    Ssu2SocketConfig {
        ipv4: Some(
            "127.0.0.1:0"
                .parse()
                .expect("loopback socket literal parses"),
        ),
        ipv6: None,
    }
}

/// Runs one complete controlled peer-test exchange to Alice-Confirmed.
///
/// See the module docs for the evidence contract. Every step is
/// causally ordered on wire/table transitions with `step_timeout`
/// deadlines; any failure, cancellation, or non-`Confirmed` outcome
/// returns an error after best-effort teardown, recording nothing.
pub async fn run_controlled_peer_test(
    params: ControlledPeerTestParams<'_>,
) -> Result<ControlledPeerTestOutcome, ControlledPeerTestError> {
    if params.step_timeout.is_zero()
        || params.step_timeout > MAX_STEP_TIMEOUT
        || params.poll_interval.is_zero()
        || params.poll_interval > params.step_timeout
    {
        return Err(ControlledPeerTestError::InvalidDriverConfig);
    }
    if params.cancellation.is_cancelled() {
        return Err(ControlledPeerTestError::ExchangeCancelled);
    }
    if params.alice_addr.port() == 0 || !params.alice_addr.ip().is_loopback() {
        return Err(ControlledPeerTestError::InvalidDriverConfig);
    }
    let alice_endpoint = Ssu2Endpoint::from_socket_addr(params.alice_addr)
        .map_err(|_| ControlledPeerTestError::InvalidDriverConfig)?;

    let mut nonce_bytes = [0_u8; 4];
    for _ in 0..8 {
        OsRng
            .try_fill_bytes(&mut nonce_bytes)
            .map_err(|_| ControlledPeerTestError::ExchangeCrypto)?;
        if u32::from_be_bytes(nonce_bytes) != 0 {
            break;
        }
    }
    let nonce = u32::from_be_bytes(nonce_bytes);
    if nonce == 0 {
        return Err(ControlledPeerTestError::ExchangeCrypto);
    }

    let bob_keys = make_helper_keys()?;
    let charlie_keys = make_helper_keys()?;
    let config = Ssu2RuntimeConfig {
        controlled_peer_test: true,
        ..Ssu2RuntimeConfig::default()
    };
    let mut bob = match start_helper(params.scope, config, bob_keys).await {
        Ok(helper) => Some(helper),
        Err(error) => return Err(error),
    };
    let mut charlie = match start_helper(params.scope, config, charlie_keys).await {
        Ok(helper) => Some(helper),
        Err(error) => {
            shutdown_helper(bob.take());
            return Err(error);
        }
    };

    let result = drive_exchange(
        &params,
        nonce,
        alice_endpoint,
        bob.as_ref().expect("helper started"),
        charlie.as_ref().expect("helper started"),
    )
    .await;
    if let Some(helper) = bob.take() {
        let _ = helper.service.finish_controlled_peer_test(nonce);
        helper.service.shutdown();
    }
    if let Some(helper) = charlie.take() {
        let _ = helper.service.finish_controlled_peer_test(nonce);
        helper.service.shutdown();
    }
    let _ = params.alice.finish_controlled_peer_test(nonce);
    result
}

fn shutdown_helper(helper: Option<Helper>) {
    if let Some(helper) = helper {
        helper.service.shutdown();
    }
}

async fn start_helper(
    scope: &ChildScope,
    config: Ssu2RuntimeConfig,
    keys: HelperKeys,
) -> Result<Helper, ControlledPeerTestError> {
    let service = Ssu2RuntimeService::new(
        config,
        Ssu2IdentityMaterial {
            router_hash: Hash::from_bytes(keys.hash_bytes),
            static_secret_bytes: keys.static_bytes,
            intro_key: keys.intro,
            router_info: keys.router_info.clone(),
        },
    )
    .map_err(|_| ControlledPeerTestError::ExchangeCrypto)?;
    let handle = service
        .start(scope, loopback_socket_config())
        .await
        .map_err(|_| ControlledPeerTestError::ExchangeHandshake)?;
    let addr = handle
        .local_v4()
        .ok_or(ControlledPeerTestError::ExchangeHandshake)?;
    Ok(Helper {
        keys,
        service,
        handle,
        addr,
    })
}

async fn check_cancelled(cancellation: &CancellationToken) -> Result<(), ControlledPeerTestError> {
    if cancellation.is_cancelled() {
        return Err(ControlledPeerTestError::ExchangeCancelled);
    }
    Ok(())
}

async fn poll_until<T>(
    cancellation: &CancellationToken,
    deadline: tokio::time::Instant,
    interval: Duration,
    mut condition: impl FnMut() -> Option<T>,
) -> Result<T, ControlledPeerTestError> {
    loop {
        if cancellation.is_cancelled() {
            return Err(ControlledPeerTestError::ExchangeCancelled);
        }
        if let Some(value) = condition() {
            return Ok(value);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(ControlledPeerTestError::ExchangeTimeout);
        }
        tokio::time::sleep(interval).await;
    }
}

fn dial_target(
    peer: PeerId,
    expected: Hash,
    addr: SocketAddr,
    static_public: Ssu2PublicKey,
    intro: IntroKey,
) -> Result<Ssu2DialTarget, ControlledPeerTestError> {
    Ssu2DialTarget::new(peer, expected, addr, static_public, intro)
        .map_err(|_| ControlledPeerTestError::InvalidDriverConfig)
}

fn map_dial_error(outcome: Ssu2DialOutcome) -> ControlledPeerTestError {
    match outcome {
        Ssu2DialOutcome::Cancelled => ControlledPeerTestError::ExchangeCancelled,
        Ssu2DialOutcome::Timeout => ControlledPeerTestError::ExchangeTimeout,
        Ssu2DialOutcome::Failed | Ssu2DialOutcome::ResourceDenied => {
            ControlledPeerTestError::ExchangeHandshake
        }
    }
}

#[allow(clippy::too_many_lines)]
async fn drive_exchange(
    params: &ControlledPeerTestParams<'_>,
    nonce: u32,
    alice_endpoint: Ssu2Endpoint,
    bob: &Helper,
    charlie: &Helper,
) -> Result<ControlledPeerTestOutcome, ControlledPeerTestError> {
    let alice_hash = params.alice_hash;
    let bob_hash = bob.keys.hash_bytes;
    let charlie_hash = charlie.keys.hash_bytes;
    let alice_peer = PeerId::from_hash(Hash::from_bytes(alice_hash));
    let step = params.step_timeout;
    let poll = params.poll_interval;

    // Helper signing keys for table verification (public only).
    let charlie_signing_public = charlie
        .keys
        .bundle
        .signing_key()
        .public_key()
        .map_err(|_| ControlledPeerTestError::ExchangeCrypto)?;
    params
        .alice
        .register_controlled_peer_signer(charlie_hash, &charlie_signing_public)?;
    bob.service
        .register_controlled_peer_signer(charlie_hash, &charlie_signing_public)?;
    charlie
        .service
        .register_controlled_peer_signer(alice_hash, &params.alice_signing_public)?;

    // Bind all three roles before any handshake carries test traffic.
    params.alice.start_controlled_peer_test(
        nonce,
        PeerTestRole::Alice,
        alice_hash,
        bob_hash,
        charlie_hash,
        alice_endpoint,
    )?;
    bob.service.start_controlled_peer_test(
        nonce,
        PeerTestRole::Bob,
        alice_hash,
        bob_hash,
        charlie_hash,
        alice_endpoint,
    )?;
    charlie.service.start_controlled_peer_test(
        nonce,
        PeerTestRole::Charlie,
        alice_hash,
        bob_hash,
        charlie_hash,
        alice_endpoint,
    )?;

    // Live sessions for the in-session Msgs 1–4 (token path first).
    check_cancelled(params.cancellation).await?;
    let bob_to_alice = dial_target(
        alice_peer,
        Hash::from_bytes(alice_hash),
        params.alice_addr,
        params.alice_static_public,
        params.alice_intro,
    )?;
    bob.service
        .dial_ssu2(bob_to_alice, step, params.cancellation)
        .await
        .map_err(map_dial_error)?;
    let charlie_to_bob = dial_target(
        bob.keys.peer,
        Hash::from_bytes(bob_hash),
        bob.addr,
        bob.keys.static_public,
        bob.keys.intro,
    )?;
    charlie
        .service
        .dial_ssu2(charlie_to_bob, step, params.cancellation)
        .await
        .map_err(map_dial_error)?;

    // Msg 1: Alice commits on wire; Bob's receipt triggers Msg 2.
    check_cancelled(params.cancellation).await?;
    let timestamp = wall_timestamp();
    let msg1_preimage = peer_test_preimage(
        1,
        &bob_hash,
        None,
        constants::SSU2_VERSION,
        nonce,
        timestamp,
        alice_endpoint,
    );
    let msg1_signature = (params.alice_sign)(&msg1_preimage)?;
    let msg1 = peer_test_block(1, None, nonce, timestamp, alice_endpoint, msg1_signature)?;
    params
        .alice
        .queue_controlled_peer_test(bob.keys.peer, msg1)?;
    let deadline = tokio::time::Instant::now() + step;
    poll_until(params.cancellation, deadline, poll, || {
        bob.service
            .take_controlled_forward(nonce)
            .ok()
            .flatten()
            .filter(|(block, _)| block.message() == 1)
    })
    .await?;

    // Msg 2: Bob forwards toward Charlie; Charlie's receipt triggers Msg 3.
    check_cancelled(params.cancellation).await?;
    let timestamp = wall_timestamp();
    let msg2_signature = sign_with(
        &bob.keys.bundle,
        2,
        &bob_hash,
        None,
        nonce,
        timestamp,
        alice_endpoint,
    )?;
    let msg2 = peer_test_block(
        2,
        Some(alice_hash),
        nonce,
        timestamp,
        alice_endpoint,
        msg2_signature,
    )?;
    bob.service
        .queue_controlled_peer_test(charlie.keys.peer, msg2)?;
    let deadline = tokio::time::Instant::now() + step;
    poll_until(params.cancellation, deadline, poll, || {
        charlie
            .service
            .take_controlled_forward(nonce)
            .ok()
            .flatten()
            .filter(|(block, _)| block.message() == 2)
    })
    .await?;

    // Class-1 evidence: Bob reports his genuine observation of Alice.
    // The service enforces reported == live peer address; the driver
    // additionally requires it to equal the claim under test.
    check_cancelled(params.cancellation).await?;
    let reported = bob.service.queue_controlled_address(alice_peer)?;
    if reported != alice_endpoint {
        return Err(ControlledPeerTestError::BindingMismatch);
    }

    // Msg 3: Charlie answers; Bob's role completes on ingest.
    check_cancelled(params.cancellation).await?;
    let timestamp = wall_timestamp();
    let msg3_signature = sign_with(
        &charlie.keys.bundle,
        3,
        &bob_hash,
        Some(&alice_hash),
        nonce,
        timestamp,
        alice_endpoint,
    )?;
    let msg3 = peer_test_block(3, None, nonce, timestamp, alice_endpoint, msg3_signature)?;
    charlie
        .service
        .queue_controlled_peer_test(bob.keys.peer, msg3)?;
    let deadline = tokio::time::Instant::now() + step;
    poll_until(params.cancellation, deadline, poll, || {
        bob.service
            .controlled_test_outcome(nonce)
            .ok()
            .flatten()
            .filter(|outcome| matches!(outcome, PeerTestOutcome::Inconclusive { .. }))
    })
    .await?;

    // Msg 4: Charlie's signed answer forwarded by Bob; Alice advances.
    check_cancelled(params.cancellation).await?;
    let timestamp = wall_timestamp();
    let msg4_signature = sign_with(
        &charlie.keys.bundle,
        4,
        &bob_hash,
        Some(&alice_hash),
        nonce,
        timestamp,
        alice_endpoint,
    )?;
    let msg4 = peer_test_block(
        4,
        Some(charlie_hash),
        nonce,
        timestamp,
        alice_endpoint,
        msg4_signature,
    )?;
    bob.service.queue_controlled_peer_test(alice_peer, msg4)?;
    let deadline = tokio::time::Instant::now() + step;
    poll_until(params.cancellation, deadline, poll, || {
        params
            .alice
            .controlled_test_state(nonce)
            .ok()
            .flatten()
            .filter(|state| *state == PeerTestState::AliceAwaitingMsg5)
    })
    .await?;

    // Msg 5: Charlie's out-of-session corroboration; Alice advances.
    check_cancelled(params.cancellation).await?;
    let timestamp = wall_timestamp();
    let msg5_signature = sign_with(
        &charlie.keys.bundle,
        5,
        &bob_hash,
        Some(&alice_hash),
        nonce,
        timestamp,
        alice_endpoint,
    )?;
    let msg5 = peer_test_block(5, None, nonce, timestamp, alice_endpoint, msg5_signature)?;
    charlie
        .service
        .send_controlled_peer_test(params.alice_addr, &params.alice_intro, msg5)?;
    let deadline = tokio::time::Instant::now() + step;
    poll_until(params.cancellation, deadline, poll, || {
        params
            .alice
            .controlled_test_state(nonce)
            .ok()
            .flatten()
            .filter(|state| *state == PeerTestState::AliceAwaitingMsg7)
    })
    .await?;

    // Msg 6: Alice answers Charlie out-of-session; Charlie completes
    // and genuinely observes Alice's source for the verdict.
    check_cancelled(params.cancellation).await?;
    let timestamp = wall_timestamp();
    let msg6_preimage = peer_test_preimage(
        6,
        &bob_hash,
        None,
        constants::SSU2_VERSION,
        nonce,
        timestamp,
        alice_endpoint,
    );
    let msg6_signature = (params.alice_sign)(&msg6_preimage)?;
    let msg6 = peer_test_block(6, None, nonce, timestamp, alice_endpoint, msg6_signature)?;
    params
        .alice
        .send_controlled_peer_test(charlie.addr, &charlie.keys.intro, msg6)?;
    let deadline = tokio::time::Instant::now() + step;
    let (_, observed) = poll_until(params.cancellation, deadline, poll, || {
        charlie
            .service
            .controlled_test_outcome(nonce)
            .ok()
            .flatten()
            .filter(|outcome| matches!(outcome, PeerTestOutcome::Inconclusive { .. }))?;
        charlie
            .service
            .take_controlled_forward(nonce)
            .ok()
            .flatten()
            .filter(|(block, _)| block.message() == 6)
    })
    .await?;

    // Msg 7: the verdict carries exactly what Charlie observed —
    // never a substituted endpoint. Agreement confirms; anything
    // else is an honest mismatch the driver propagates as failure.
    check_cancelled(params.cancellation).await?;
    let timestamp = wall_timestamp();
    let msg7_signature = sign_with(
        &charlie.keys.bundle,
        7,
        &bob_hash,
        Some(&alice_hash),
        nonce,
        timestamp,
        observed,
    )?;
    let msg7 = peer_test_block(7, None, nonce, timestamp, observed, msg7_signature)?;
    charlie
        .service
        .send_controlled_peer_test(params.alice_addr, &params.alice_intro, msg7)?;
    let deadline = tokio::time::Instant::now() + step;
    let outcome = poll_until(params.cancellation, deadline, poll, || {
        params.alice.controlled_test_outcome(nonce).ok().flatten()
    })
    .await?;
    match outcome {
        PeerTestOutcome::DirectReachabilityConfirmed {
            family,
            observed: confirmed,
            evidence_peers,
        } if confirmed.socket_addr() == params.alice_addr && evidence_peers == 2 => {
            Ok(ControlledPeerTestOutcome::Confirmed {
                family,
                observed: confirmed.socket_addr(),
                evidence_peers,
            })
        }
        _ => Err(ControlledPeerTestError::ExchangeNotConfirmed),
    }
}
