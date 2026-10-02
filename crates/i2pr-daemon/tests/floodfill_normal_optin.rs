//! Plan 279 §4/§10 — normal floodfill opt-in against a live loopback SSU2 service.
//!
//! Ordinary runs execute this file (no `#[ignore]`): it binds only
//! loopback sockets and proves the fail-closed half of the normal
//! path through real composition — explicit opt-in with a
//! loopback-only daemon can never present a publicly reachable
//! address, so activation fails with categorical reasons, the role
//! stays Disabled, and no `caps=f` record is ever built. The
//! eligible half (public material to signed `caps=f`) is proven
//! without sockets in `floodfill::tests`, since no loopback host can
//! satisfy the public-routability gate by construction.

#![forbid(unsafe_code)]

use std::time::Duration;

use i2pr_crypto::{RouterIdentityBundle, X25519PrivateKey};
use i2pr_daemon::config::Config;
use i2pr_daemon::floodfill::{
    FloodfillCoordinator, FloodfillCoordinatorPolicy, FloodfillServiceState, FloodfillServiceStep,
    NormalActivationError, NormalActivationParams, NormalIneligibility, NormalReadiness,
    activate_normal, is_sane_wall_ms,
};
use i2pr_daemon::router_i2np::Ssu2DaemonService;
use i2pr_netdb::{
    FloodfillResourcePolicy, FloodfillRoleState, FloodfillStorePolicy, ReplicationPolicy,
    ServerNetDbConfig,
};
use i2pr_proto::{Date, Mapping, RouterAddress};
use i2pr_runtime::{
    CancellationToken, ChildFailurePolicy, ChildScope, IntroKey, Ssu2IdentityMaterial,
};
use rand_core::{OsRng, TryRngCore};

fn wall_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

fn i2p_b64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-~";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let value = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((value >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((value >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((value >> 6) & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(value & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

#[tokio::test]
async fn normal_opt_in_on_loopback_stays_disabled_without_f() {
    let mut rng = OsRng;
    let bundle = RouterIdentityBundle::generate(&mut rng).expect("identity");
    let static_secret = X25519PrivateKey::generate(&mut rng).expect("static secret");
    let mut intro_bytes = [0_u8; 32];
    loop {
        rng.try_fill_bytes(&mut intro_bytes).expect("rng");
        if intro_bytes.iter().any(|byte| *byte != 0) {
            break;
        }
    }
    let hash = bundle.identity().hash().expect("identity hash");

    // Strict loopback profile with explicit normal opt-in: parses, but
    // the runtime can never qualify a publicly reachable address.
    let text = "schema_version = 1\n[router]\ndata_dir = \"./state\"\n[ssu2]\nenabled = true\n[floodfill]\nenabled = true\n";
    let config = Config::parse(text).expect("opt-in parses");
    assert!(config.floodfill.enabled);

    let options = Mapping::from_entries(vec![
        ("host".to_string(), "127.0.0.1".to_string()),
        ("port".to_string(), "44001".to_string()),
        ("v".to_string(), "2".to_string()),
        (
            "s".to_string(),
            i2p_b64_encode(&static_secret.public_bytes()),
        ),
        ("i".to_string(), i2p_b64_encode(&intro_bytes)),
    ])
    .expect("address options");
    let address = RouterAddress::new(
        10,
        Date::from_millis(9_999_999_999_999),
        "SSU2".to_string(),
        options,
    )
    .expect("router address");
    let initial = bundle
        .sign_router_info(
            Date::from_millis(wall_ms()),
            vec![address],
            Vec::new(),
            i2pr_netdb::controlled_router_options().expect("controlled options"),
        )
        .expect("sign initial RouterInfo")
        .encode_to_vec(i2pr_runtime::constants::MAX_ESTABLISHMENT_ROUTER_INFO_BYTES)
        .expect("encode initial RouterInfo");

    let service = Ssu2DaemonService::new(
        &config.ssu2,
        Ssu2IdentityMaterial {
            router_hash: hash,
            static_secret_bytes: *static_secret.secret_bytes(),
            intro_key: IntroKey::new(intro_bytes),
            router_info: initial,
        },
    )
    .expect("daemon service");
    let token = CancellationToken::new();
    let scope = ChildScope::for_test(&token, ChildFailurePolicy::FailParent);
    let handle = service
        .start(&scope, &config.ssu2)
        .await
        .expect("bind loopback socket");

    // A fresh loopback service has no corroborated reachability, so no
    // live material exists on this tick.
    let wall = wall_ms();
    assert!(handle.service().publication_material(wall).is_err());

    let mut state =
        FloodfillServiceState::new(hash, config.floodfill.enabled).expect("service state");
    let readiness = NormalReadiness {
        material: None,
        wall_now_ms: wall,
        netdb_ready: true,
        storage_ready: true,
        maintenance_ready: true,
        resource_headroom: true,
        clock_sane: is_sane_wall_ms(wall),
        supervision_healthy: true,
    };
    assert_eq!(state.evaluate(&readiness), FloodfillServiceStep::Idle);
    assert_eq!(state.role_state(), FloodfillRoleState::Disabled);
    let status = state.status();
    assert!(status.requested);
    assert_eq!(status.role, FloodfillRoleState::Disabled);
    assert!(
        status
            .reasons
            .contains(&NormalIneligibility::AddressUnqualified)
    );
    assert!(
        status
            .reasons
            .contains(&NormalIneligibility::ReachabilityUnconfirmed)
    );

    // The one-shot composition agrees through the same live handle:
    // ineligible, Disabled, no permit, no build.
    let mut coordinator = FloodfillCoordinator::new(
        hash,
        FloodfillCoordinatorPolicy::default(),
        FloodfillStorePolicy::default(),
        ServerNetDbConfig::default(),
        FloodfillResourcePolicy::default(),
        ReplicationPolicy::default(),
    )
    .expect("coordinator");
    let outcome = activate_normal(NormalActivationParams {
        handle: &handle,
        coordinator: &mut coordinator,
        bundle: &bundle,
        intent: true,
        readiness: NormalReadiness {
            material: None,
            wall_now_ms: wall,
            netdb_ready: true,
            storage_ready: true,
            maintenance_ready: true,
            resource_headroom: true,
            clock_sane: is_sane_wall_ms(wall),
            supervision_healthy: true,
        },
        cancellation: &CancellationToken::new(),
    })
    .await;
    assert!(
        matches!(outcome, Err(NormalActivationError::EligibilityFailed(_))),
        "loopback activation must fail eligibility"
    );
    assert_eq!(coordinator.role_state(), FloodfillRoleState::Disabled);

    token.cancel(i2pr_runtime::CancellationReason::OperatorRequest);
    tokio::time::timeout(Duration::from_secs(30), scope.shutdown())
        .await
        .expect("shutdown drains");
}
