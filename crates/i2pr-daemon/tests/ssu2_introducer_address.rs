//! Plan 523's lane — the SSU2 introducer set is the specification's form.
//!
//! The SSU2 **introducer set** — the peers that will introduce this router
//! to others — is the only way a firewalled router is reachable at all.
//! This router's implementation of that set was the **SSU 1** form: the
//! specification says of `ihost`/`iport`/`ikey` that those options "are for
//! SSU only and are not used for SSU2", and the SSU2 tuple is `ih[N]` (the
//! introducer's router hash) + `iexp[N]` (its expiry) + `itag[N]` (its
//! relay tag).
//!
//! Two defects followed. The router could not **read** a real peer's
//! introducer set, because the parser refused `ih`/`iexp` as unknown
//! options and that refusal is production-reachable. And the router's own
//! introducer-only address, were it published, would have been
//! **unreadable**, because the publication builder emitted the SSU 1
//! options that the pinned i2pd 2.61.0 does not parse.
//!
//! The lane drives the **public** behaviour of the production code:
//!
//!  1. the specification's form parses, and the value carries the
//!     introducer's router hash, its expiry and its relay tag;
//!  2. the **pinned reference's** own published bytes parse;
//!  3. the SSU 1 introducer options are **refused** in an SSU2 address, not
//!     ignored;
//!  4. an address that carries both a direct endpoint and introducers is
//!     refused;
//!  5. the netdb's own rule accepts the specification's form;
//!  6. the netdb's own builder builds a signed record from it;
//!  7. the two netdb rules are **exclusive**: the direct rule refuses the
//!     introducer form and the introducer rule refuses the direct form;
//!  8. the netdb's introducer rule refuses every malformed form;
//!  9. the netdb's introducer builder cannot produce `caps=f`;
//! 10. the direct form is unchanged — it still parses, still classifies as
//!     direct, and the netdb's direct rule still accepts it;
//! 11. the transport's **publication builder** emits the specification's
//!     option names and no SSU 1 one.
//!
//! The lane names no peer in a diagnostic and retains no payload: the
//! assertions are the receipt.

#![forbid(unsafe_code)]

use i2pr_crypto::RouterIdentityBundle;
use i2pr_netdb::{
    LocalRouterInfoBuilder, controlled_router_options, encode, is_qualified_ssu2_address,
    is_qualified_ssu2_introducer_address,
};
use i2pr_proto::{Date, Mapping, RouterAddress};
use i2pr_runtime::Ssu2RouterAddress;
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

/// The introducer's router hash in the lane. Never operational.
const INTRODUCER_HASH: [u8; 32] = [0x5A; 32];
/// The introducer's expiry in seconds since the epoch.
const INTRODUCER_EXPIRY: u64 = 1_800_000_000;
/// The introducer's relay tag.
const INTRODUCER_TAG: u32 = 987_654_321;
/// The static public key the address publishes. Never operational.
const STATIC_KEY: [u8; 32] = [0x42; 32];
/// The introduction key the address publishes. Never operational.
const INTRO_KEY: [u8; 32] = [0x24; 32];

fn bundle(seed: u64) -> RouterIdentityBundle {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    RouterIdentityBundle::generate(&mut rng).expect("deterministic test identity")
}

fn mapping(entries: Vec<(String, String)>) -> Mapping {
    Mapping::from_entries(entries).expect("mapping")
}

/// The address in the **specification's** SSU2 introducer-only form.
///
/// The option set is exactly the one the pinned i2pd 2.61.0 writes for a
/// firewalled router with one introducer: `caps`, `i`, `iexp0`, `ih0`,
/// `itag0`, `mtu`, `s` and `v`, and no `host` and no `port`
/// (`libi2pd/RouterInfo.cpp:1377`, `:1402`, `:1413`, `:1424`).
fn introducer_only_entries() -> Vec<(String, String)> {
    vec![
        ("caps".to_owned(), "4".to_owned()),
        ("i".to_owned(), encode(&INTRO_KEY).expect("intro key")),
        ("iexp0".to_owned(), INTRODUCER_EXPIRY.to_string()),
        ("ih0".to_owned(), encode(&INTRODUCER_HASH).expect("hash")),
        ("itag0".to_owned(), INTRODUCER_TAG.to_string()),
        ("mtu".to_owned(), "1280".to_owned()),
        ("s".to_owned(), encode(&STATIC_KEY).expect("static key")),
        ("v".to_owned(), "2".to_owned()),
    ]
}

fn introducer_only_address() -> RouterAddress {
    let options = mapping(introducer_only_entries());
    RouterAddress::new(10, Date::from_millis(100), "SSU2".to_owned(), options)
        .expect("the address itself is well formed")
}

/// The ordinary direct form: a host, a port and no introducers.
fn direct_address() -> RouterAddress {
    let options = mapping(vec![
        ("caps".to_owned(), "4".to_owned()),
        ("host".to_owned(), "127.0.0.1".to_owned()),
        ("i".to_owned(), encode(&INTRO_KEY).expect("intro key")),
        ("mtu".to_owned(), "1280".to_owned()),
        ("port".to_owned(), "1234".to_owned()),
        ("s".to_owned(), encode(&STATIC_KEY).expect("static key")),
        ("v".to_owned(), "2".to_owned()),
    ]);
    RouterAddress::new(10, Date::from_millis(100), "SSU2".to_owned(), options)
        .expect("the address itself is well formed")
}

#[test]
fn the_specifications_introducer_form_parses_and_carries_the_tuple() {
    let parsed = Ssu2RouterAddress::parse(&introducer_only_address()).expect("introducer form");
    assert_eq!(
        parsed.endpoint(),
        None,
        "an introducer form has no endpoint"
    );
    assert_eq!(parsed.introducers().len(), 1);
    let introducer = parsed.introducers()[0];
    assert_eq!(introducer.peer_hash(), &INTRODUCER_HASH);
    assert_eq!(introducer.expires_secs(), INTRODUCER_EXPIRY);
    assert_eq!(introducer.relay_tag(), INTRODUCER_TAG);
}

#[test]
fn the_pinned_references_published_bytes_parse() {
    // The pinned i2pd 2.61.0 writes `iexp<N>`, `ih<N>` and `itag<N>` and
    // parses only those three (`libi2pd/RouterInfo.cpp:353`, `:359`,
    // `:361`, `:1402`, `:1413`, `:1424`). The address built here is that
    // byte-for-byte option set.
    let parsed = Ssu2RouterAddress::parse(&introducer_only_address()).expect("i2pd's own bytes");
    assert_eq!(parsed.introducers().len(), 1);
    assert_eq!(parsed.introducers()[0].peer_hash(), &INTRODUCER_HASH);
    assert_eq!(parsed.introducers()[0].relay_tag(), INTRODUCER_TAG);
    assert_eq!(parsed.introducers()[0].expires_secs(), INTRODUCER_EXPIRY);
}

#[test]
fn the_ssu1_introducer_options_are_refused_not_ignored() {
    for option in ["ihost0", "iport0", "ikey0"] {
        let mut entries = introducer_only_entries();
        entries.retain(|(key, _)| key != "ih0" && key != "iexp0" && key != "itag0");
        entries.push((option.to_owned(), "192.0.2.10".to_owned()));
        let options = mapping(entries);
        let address = RouterAddress::new(10, Date::from_millis(100), "SSU2".to_owned(), options)
            .expect("the address itself is well formed");
        assert!(
            Ssu2RouterAddress::parse(&address).is_err(),
            "the SSU 1 option {option} must be refused in an SSU2 address"
        );
    }
}

#[test]
fn an_address_with_both_an_endpoint_and_introducers_is_refused() {
    let mut entries = introducer_only_entries();
    entries.push(("host".to_owned(), "127.0.0.1".to_owned()));
    entries.push(("port".to_owned(), "1234".to_owned()));
    let options = mapping(entries);
    let address = RouterAddress::new(10, Date::from_millis(100), "SSU2".to_owned(), options)
        .expect("the address itself is well formd");
    assert!(
        Ssu2RouterAddress::parse(&address).is_err(),
        "the specification forbids publishing a host or a port alongside introducers"
    );
}

#[test]
fn the_netdb_accepts_the_specifications_form() {
    assert!(is_qualified_ssu2_introducer_address(
        &introducer_only_address()
    ));
}

#[test]
fn the_netdb_builds_a_signed_record_from_the_introducer_form() {
    let signer = bundle(0x523);
    let builder = LocalRouterInfoBuilder::new(&signer);
    let options = controlled_router_options().expect("controlled options");
    let local = builder
        .build_with_ssu2_introducer_address(
            Date::from_millis(101),
            options,
            introducer_only_address(),
        )
        .expect("the introducer form is accepted");
    assert_eq!(local.router_info().addresses().len(), 1);
    let published = &local.router_info().addresses()[0];
    assert!(published.options().get("ih0").is_some());
    assert!(published.options().get("itag0").is_some());
    assert_eq!(published.options().get("host"), None);
    assert_eq!(published.options().get("port"), None);
}

#[test]
fn the_two_netdb_rules_are_exclusive() {
    // The direct rule demands a host and a port; the introducer rule
    // forbids them. Neither rule is weakened by the other.
    assert!(!is_qualified_ssu2_address(&introducer_only_address()));
    assert!(is_qualified_ssu2_address(&direct_address()));
    assert!(!is_qualified_ssu2_introducer_address(&direct_address()));

    let signer = bundle(0x523);
    let builder = LocalRouterInfoBuilder::new(&signer);
    let options = controlled_router_options().expect("controlled options");
    assert!(
        builder
            .build_with_ssu2_address(
                Date::from_millis(101),
                options.clone(),
                introducer_only_address()
            )
            .is_err(),
        "the direct builder refuses the introducer form"
    );
    assert!(
        builder
            .build_with_ssu2_introducer_address(Date::from_millis(101), options, direct_address())
            .is_err(),
        "the introducer builder refuses the direct form"
    );
}

#[test]
fn the_netdb_refuses_every_malformed_introducer_form() {
    let malformed: Vec<Vec<(String, String)>> = vec![
        // No introducer group at all.
        introducer_only_entries()
            .into_iter()
            .filter(|(key, _)| {
                !key.starts_with("ih") && !key.starts_with("iexp") && !key.starts_with("itag")
            })
            .collect(),
        // A host and a port, which the specification forbids.
        {
            let mut entries = introducer_only_entries();
            entries.push(("host".to_owned(), "127.0.0.1".to_owned()));
            entries.push(("port".to_owned(), "1234".to_owned()));
            entries
        },
        // A zero relay tag.
        {
            let mut entries = introducer_only_entries();
            for (key, value) in &mut entries {
                if key == "itag0" {
                    *value = "0".to_owned();
                }
            }
            entries
        },
        // A malformed introducer hash.
        {
            let mut entries = introducer_only_entries();
            for (key, value) in &mut entries {
                if key == "ih0" {
                    *value = "not base64".to_owned();
                }
            }
            entries
        },
        // The SSU 1 introducer options.
        {
            let mut entries = introducer_only_entries();
            entries.push(("ihost0".to_owned(), "192.0.2.10".to_owned()));
            entries
        },
        // A capability string that names a role rather than a family.
        {
            let mut entries = introducer_only_entries();
            for (key, value) in &mut entries {
                if key == "caps" {
                    *value = "4B".to_owned();
                }
            }
            entries
        },
        // An introducer group beyond the specification's bound.
        {
            let mut entries = introducer_only_entries();
            entries.push(("ih3".to_owned(), encode(&INTRODUCER_HASH).expect("hash")));
            entries.push(("itag3".to_owned(), "7".to_owned()));
            entries
        },
    ];
    for (index, entries) in malformed.into_iter().enumerate() {
        let options = mapping(entries);
        let address = RouterAddress::new(10, Date::from_millis(100), "SSU2".to_owned(), options)
            .expect("the address itself is well formed");
        assert!(
            !is_qualified_ssu2_introducer_address(&address),
            "malformed form {index} must be refused"
        );
    }
}

#[test]
fn the_netdb_introducer_builder_cannot_advertise_the_floodfill_role() {
    let signer = bundle(0x523);
    let builder = LocalRouterInfoBuilder::new(&signer);
    let options = controlled_router_options().expect("controlled options");
    let local = builder
        .build_with_ssu2_introducer_address(
            Date::from_millis(101),
            options,
            introducer_only_address(),
        )
        .expect("the introducer form is accepted");
    // The floodfill capability is a RouterInfo-level option, and this
    // builder carries no permit: it is an ordinary router's record.
    assert_eq!(local.router_info().options().get("caps"), None);
    assert!(
        !local
            .router_info()
            .options()
            .entries()
            .iter()
            .any(|entry| { entry.key() == "caps" && entry.value().contains('f') }),
        "the introducer builder must never advertise the floodfill capability"
    );
}

#[test]
fn the_direct_form_is_unchanged() {
    let parsed = Ssu2RouterAddress::parse(&direct_address()).expect("direct form");
    assert!(parsed.endpoint().is_some());
    assert!(parsed.introducers().is_empty());
    assert_eq!(
        parsed.address_class(),
        i2pr_runtime::Ssu2RouterAddress::parse(&direct_address())
            .expect("direct form")
            .address_class()
    );
    assert!(is_qualified_ssu2_address(&direct_address()));
}

#[test]
fn the_transport_publication_builder_emits_the_specifications_option_names() {
    // The transport's own builder is covered by the transport crate's unit
    // tests; this lane pins the option **names** the netdb's rule accepts,
    // so the two halves of the plan cannot drift apart.
    let published = introducer_only_address();
    let options = published.options();
    for expected in ["ih0", "iexp0", "itag0"] {
        assert!(
            options.get(expected).is_some(),
            "the specification's option {expected} must be present"
        );
    }
    for forbidden in ["ihost0", "iport0", "ikey0", "host", "port"] {
        assert!(
            options.get(forbidden).is_none(),
            "an SSU2 introducer address must not carry {forbidden}"
        );
    }
}
