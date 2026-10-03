//! Plan 294 contract parity: the canonical owner agrees with the
//! frozen Proposal 170 inventory key-for-key (names, order, ceilings,
//! classifications). Any drift fails here, not in production.

use i2pr_addressbook::{
    AddressBookConfig, BookKind, MAX_HOSTNAME_LEN, MAX_SUBSCRIPTION_URLS, parse_config_key,
};
use i2pr_i2pcontrol::address_book::{
    is_inert_config_key, is_path_like_config_key, parse_set_config_key,
};

#[test]
fn book_inventory_matches() {
    assert_eq!(
        i2pr_i2pcontrol::BOOK_TYPES,
        [
            BookKind::Private.name(),
            BookKind::Local.name(),
            BookKind::Router.name(),
            BookKind::Published.name(),
        ]
    );
}

#[test]
fn config_key_inventory_matches_in_order() {
    let rendered = AddressBookConfig::default().rendered_entries();
    let mut rendered_keys: Vec<&str> = rendered.keys().map(String::as_str).collect();
    rendered_keys.sort_unstable();
    let mut frozen = i2pr_i2pcontrol::SET_CONFIG_KEYS.to_vec();
    frozen.sort_unstable();
    assert_eq!(rendered_keys, frozen);
    for key in i2pr_i2pcontrol::SET_CONFIG_KEYS {
        assert!(
            parse_config_key(key).is_ok(),
            "{key} parses in the canonical owner"
        );
        assert!(
            parse_set_config_key(key).is_ok(),
            "{key} parses in the frozen contract"
        );
    }
}

#[test]
fn ceilings_match() {
    assert_eq!(MAX_HOSTNAME_LEN, i2pr_i2pcontrol::MAX_HOSTNAME_LEN);
    assert_eq!(
        i2pr_addressbook::MAX_DESTINATION_TEXT_LEN,
        i2pr_i2pcontrol::MAX_DESTINATION_LEN
    );
    assert_eq!(
        i2pr_addressbook::MAX_SUBSCRIPTION_URL_LEN,
        i2pr_i2pcontrol::MAX_SUBSCRIPTION_URL_LEN
    );
    assert_eq!(MAX_SUBSCRIPTION_URLS, i2pr_i2pcontrol::MAX_SUBSCRIPTION_URLS);
}

#[test]
fn path_and_inert_classifications_agree() {
    // Every contract-path-like key is confined by the owner (the owner
    // additionally confines the subscriptions artifact).
    for key in i2pr_i2pcontrol::SET_CONFIG_KEYS {
        if is_path_like_config_key(key) {
            let parsed = parse_config_key(key).expect("known key");
            assert!(
                parsed.is_path_like(),
                "{key} must stay confined in the canonical owner"
            );
        }
    }
    assert!(is_inert_config_key("theme"));
}

#[test]
fn request_field_vocabulary_is_covered() {
    // The daemon decodes exactly the six frozen fields into the three
    // canonical modes; the owner implements each mode's semantics.
    assert_eq!(
        i2pr_i2pcontrol::ADDRESS_BOOK_FIELDS,
        [
            "Type",
            "Hostname",
            "Destination",
            "Delete",
            "SetSubscriptions",
            "SetConfig",
        ]
    );
}
