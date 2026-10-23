//! Plan 490 — the private controlled mesh for the SSU2 introducer role.
//!
//! The mesh has three peers with real keys and real signatures:
//!
//! ```text
//! Bob   = this router (the introducer)
//! Alice = the requester (direct)
//! Charlie = the target (firewalled, reached through Bob)
//! ```
//!
//! The lane drives the **daemon's production role owner** through the
//! same entry point the runtime handoff uses
//! (`Ssu2IntroducerService::on_relay_event`). Nothing here reaches a
//! socket, a listener, or the public network.
//!
//! The evidence file records counts and digests only: no peer hash, no
//! address, no tag value, no key, no payload.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};

use i2pr_crypto::{RouterIdentityBundle, SigningPrivateKey, verify_signature};
use i2pr_daemon::router_i2np::generate_controlled_identity;
use i2pr_daemon::ssu2_introducer::{
    IntroducerSignerResolver, Ssu2IntroducerConfig, Ssu2IntroducerOutcome, Ssu2IntroducerService,
};
use i2pr_proto::{Hash, SigningPublicKey};
use i2pr_runtime::{
    Ssu2Endpoint, Ssu2InboundRelay, Ssu2RelayEvent, Ssu2RuntimeConfig, Ssu2RuntimeService,
    relay_request_preimage, relay_response_preimage,
};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

const TARGET_HASH: [u8; 32] = [0x7A; 32];
const REQUESTER_HASH: [u8; 32] = [0xC1; 32];
const NOW_MS: u64 = 1_000;
const NOW_SECS: u64 = 1_000;
const TAG: u32 = 0xABCD;

struct MeshSigners {
    keys: HashMap<[u8; 32], SigningPublicKey>,
}

impl IntroducerSignerResolver for MeshSigners {
    fn signing_key(&self, peer_hash: &[u8; 32]) -> Option<SigningPublicKey> {
        self.keys.get(peer_hash).cloned()
    }
}

fn endpoint() -> Ssu2Endpoint {
    Ssu2Endpoint::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 9_999).expect("endpoint")
}

fn inbound(peer: [u8; 32], event: Ssu2RelayEvent) -> Ssu2InboundRelay {
    Ssu2InboundRelay {
        link_id: i2pr_transport::LinkId::new(1).expect("link id"),
        peer: i2pr_transport::PeerId::from_hash(Hash::from_bytes(peer)),
        event,
    }
}

#[test]
fn a_requester_asks_this_router_to_introduce_a_firewalled_peer_and_the_introduction_completes() {
    let mut rng = ChaCha8Rng::seed_from_u64(0x4A0);
    let bundle = RouterIdentityBundle::generate(&mut rng).expect("identity");
    let identity =
        generate_controlled_identity(&bundle, "127.0.0.1", 34_567).expect("ssu2 identity");
    let bob = *identity.router_hash.as_bytes();

    let runtime = Ssu2RuntimeService::new_with_bandwidth(
        Ssu2RuntimeConfig {
            introducer_service: true,
            ..Default::default()
        },
        identity,
        None,
    )
    .expect("runtime");
    let mut owner = Ssu2IntroducerService::new(
        Ssu2IntroducerConfig {
            enabled: true,
            ..Default::default()
        },
        runtime,
        bob,
    );

    let alice_key = SigningPrivateKey::from_bytes([0x11; 32]);
    let charlie_key = SigningPrivateKey::from_bytes([0x22; 32]);
    let mut signers = MeshSigners {
        keys: HashMap::new(),
    };
    signers.keys.insert(
        REQUESTER_HASH,
        alice_key.public_key().expect("alice public"),
    );
    signers.keys.insert(
        TARGET_HASH,
        charlie_key.public_key().expect("charlie public"),
    );

    // 1. The firewalled target asks this router for an introduction tag.
    owner.issue_tag(TARGET_HASH, TAG, NOW_SECS).expect("tag");

    // 2. The requester sends the Relay Request naming the tag.
    let request_preimage =
        relay_request_preimage(&bob, &TARGET_HASH, 21, TAG, NOW_SECS as u32, 2, endpoint());
    let request_signature = alice_key.sign(&request_preimage).expect("sign");
    let request = i2pr_runtime::RelayRequestBlock::new(
        21,
        TAG,
        NOW_SECS as u32,
        2,
        endpoint(),
        request_signature.as_bytes().to_vec(),
    )
    .expect("request");

    // 3. This router admits the request and produces the Relay Intro.
    let outcome = owner.on_relay_event(
        &inbound(REQUESTER_HASH, Ssu2RelayEvent::Request(request)),
        &signers,
        NOW_MS,
        NOW_SECS,
    );
    let Ssu2IntroducerOutcome::IntroRequired { target, intro } = outcome else {
        panic!("the introducer must answer with a Relay Intro, got {outcome:?}");
    };
    assert_eq!(target, TARGET_HASH, "the tag names the target");
    assert_eq!(intro.alice_hash(), &REQUESTER_HASH);
    assert_eq!(intro.nonce(), 21);
    assert_eq!(intro.relay_tag(), TAG);

    // 4. The target verifies the intro and answers with a Relay Response.
    assert_eq!(intro.alice_hash(), &REQUESTER_HASH);
    let response_preimage =
        relay_response_preimage(&bob, intro.nonce(), NOW_SECS as u32, 2, Some(endpoint()));
    let response_signature = charlie_key.sign(&response_preimage).expect("sign");
    let response = i2pr_runtime::RelayResponseBlock::accept(
        intro.nonce(),
        NOW_SECS as u32,
        2,
        endpoint(),
        response_signature.as_bytes().to_vec(),
        0xF00D,
    )
    .expect("accept");

    // 5. This router verifies the response and forwards it to the requester.
    let outcome = owner.on_relay_event(
        &inbound(TARGET_HASH, Ssu2RelayEvent::Response(response.clone())),
        &signers,
        NOW_MS,
        NOW_SECS,
    );
    let Ssu2IntroducerOutcome::ResponseRequired {
        requester,
        response,
    } = outcome
    else {
        panic!("the introducer must forward the response, got {outcome:?}");
    };
    assert_eq!(requester, REQUESTER_HASH);

    // 6. The requester verifies the forwarded response under the
    //    target's key: the introduction completed.
    let response_signature = i2pr_proto::SignatureValue::new(
        i2pr_proto::SigningKeyType::EdDsaSha512Ed25519,
        response.signature().to_vec(),
    )
    .expect("signature value");
    verify_signature(
        &charlie_key.public_key().expect("charlie public"),
        &response_preimage,
        &response_signature,
    )
    .expect("the requester must verify the target's signature");

    // 7. The receipt proves the role ran, and carries counts only.
    let receipt = owner.receipt();
    assert_eq!(receipt.requests_admitted, 1);
    assert_eq!(receipt.intros_produced, 1);
    assert_eq!(receipt.responses_forwarded, 1);
    assert_eq!(receipt.tags_issued, 1);
    assert!(receipt.refusals.is_empty(), "{:?}", receipt.refusals);

    // 8. The evidence file records counts and digests only.
    let rendered = format!("{receipt:?}");
    for forbidden in ["0xC1", "0x7A", "127.0.0.1", "9999", "43981"] {
        assert!(
            !rendered.contains(forbidden),
            "the receipt must not carry `{forbidden}`: {rendered}"
        );
    }
    let evidence = serde_json::json!({
        "schema": "i2pr-ssu2-introducer-mesh-v1",
        "plan": 490,
        "mesh": {
            "peers": 3,
            "tags_issued": receipt.tags_issued,
            "requests_admitted": receipt.requests_admitted,
            "intros_produced": receipt.intros_produced,
            "responses_forwarded": receipt.responses_forwarded,
            "refusals": receipt.refusals.len(),
        },
        "receipt_digests": {
            "admitted": digest(format!("{:?}", receipt.requests_admitted).as_bytes()),
            "intro": digest(format!("{:?}", receipt.intros_produced).as_bytes()),
            "forwarded": digest(format!("{:?}", receipt.responses_forwarded).as_bytes()),
        },
    });
    let rendered_evidence = evidence.to_string();
    for forbidden in ["0xC1", "0x7A", "127.0.0.1", "9999", "43981"] {
        assert!(
            !rendered_evidence.contains(forbidden),
            "the evidence must not carry `{forbidden}`: {rendered_evidence}"
        );
    }
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/integration/network-participation/evidence/plan490/result.json");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("evidence directory");
    }
    let mut rendered_evidence = serde_json::to_string_pretty(&evidence).expect("evidence json");
    rendered_evidence.push('\n');
    std::fs::write(&path, rendered_evidence).expect("evidence file");
}

/// Returns the lowercase hex SHA-256 of the supplied bytes.
fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
