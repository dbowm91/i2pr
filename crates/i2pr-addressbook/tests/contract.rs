//! Plan 294 owner bounds remain separate from the canonical Proposal 170
//! wire inventory, which the daemon translates into these internal keys.

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
    let expected = [
        "etags",
        "last_modified",
        "local_book",
        "log_file",
        "log_level",
        "lookup_timeout",
        "max_entries",
        "private_book",
        "proxy_host",
        "proxy_port",
        "published_book",
        "refresh_interval",
        "router_book",
        "should_publish",
        "subscriptions",
        "theme",
    ];
    assert_eq!(rendered_keys, expected);
    for key in expected {
        assert!(parse_config_key(key).is_ok(), "{key} parses in the owner");
    }
    for key in i2pr_i2pcontrol::SET_CONFIG_KEYS {
        assert!(
            parse_set_config_key(key).is_ok(),
            "{key} parses in the Proposal wire contract"
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
    assert_eq!(
        MAX_SUBSCRIPTION_URLS,
        i2pr_i2pcontrol::MAX_SUBSCRIPTION_URLS
    );
}

#[test]
fn path_and_inert_classifications_agree() {
    // The wire parser classifies Proposal path fields; the daemon maps them
    // to the owner's confined artifact names.
    for (wire, internal) in [
        ("subscriptions", "subscriptions"),
        ("published_addressbook", "published_book"),
        ("router_addressbook", "router_book"),
        ("local_addressbook", "local_book"),
        ("private_addressbook", "private_book"),
        ("etags", "subscriptions"),
        ("last_modified", "subscriptions"),
        ("log", "log_file"),
    ] {
        assert!(is_path_like_config_key(wire), "{wire} is a path field");
        assert!(
            parse_config_key(internal)
                .expect("known owner key")
                .is_path_like(),
            "{internal} is confined by the owner"
        );
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
