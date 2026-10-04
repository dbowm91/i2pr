//! Plan 286 contract tests: exact inventories, deterministic parsing,
//! typed literal failures, max/max+1 bounds, secret classification.

use i2pr_i2pcontrol::{
    ADDRESS_BOOK_FIELDS, AddressBookField, AuthErrorCode, BASE_ROUTER_INFO_FIELDS, BOOK_TYPES,
    BookType, CLIENT_SERVICES, ClientService, ContractError, ContractInventory, JsonRpcErrorCode,
    JsonRpcRequest, MAX_BATCH_ELEMENTS, MAX_DESTINATION_LEN, MAX_HOSTNAME_LEN, MAX_HTTP_BODY_BYTES,
    MAX_ID_STRING_LEN, MAX_INFLIGHT_REQUESTS, MAX_LIST_ITEMS, MAX_LIVE_TOKENS, MAX_MAP_ENTRIES,
    MAX_MAP_KEY_LEN, MAX_METHOD_NAME_LEN, MAX_OPTION_NAME_LEN, MAX_OPTION_VALUE_LEN,
    MAX_OPTIONS_PER_TUNNEL, MAX_PARAMS_KEYS, MAX_PASSWORD_LEN, MAX_PRESENTED_TOKEN_LEN,
    MAX_SELECTOR_LEN, MAX_STRING_LEN, MAX_SUBSCRIPTION_URL_LEN, MAX_SUBSCRIPTION_URLS,
    MAX_TUNNEL_DEFS, MAX_TUNNEL_NAME_LEN, METHODS, Method, PROPOSAL_ADDRESS_BOOK_CONFIG_KEYS,
    PROPOSAL_ENCRYPT_LEASE_SET_VALUES, PROPOSAL_ROUTER_INFO_FIELDS, PROPOSAL_TUNNEL_INTEGER_RANGES,
    PROPOSAL_TUNNEL_MANAGER_FIELDS, PROPOSAL_TUNNEL_POLICY_INTEGER_RANGES, ROUTER_INFO_SELECTORS,
    RequestId, ReturnType, RouterInfoSelector, SECRET_OPTIONS, SET_CONFIG_KEYS, TOKEN_BYTES,
    TOKEN_LIFETIME_SECS, TUNNEL_ACTIONS, TUNNEL_OPTIONS, TUNNEL_TYPES, TunnelAction, TunnelStatus,
    TunnelType, auth, conformance, jsonrpc, limits, proposal_wire, tunnel, tunnel_options,
};

#[test]
fn frozen_counts_match_plan_286() {
    assert_eq!(METHODS.len(), 5);
    assert_eq!(ROUTER_INFO_SELECTORS.len(), 30);
    assert_eq!(PROPOSAL_ROUTER_INFO_FIELDS.len(), 43);
    assert_eq!(BASE_ROUTER_INFO_FIELDS.len(), 14);
    assert_eq!(CLIENT_SERVICES.len(), 6);
    assert_eq!(BOOK_TYPES.len(), 4);
    assert_eq!(ADDRESS_BOOK_FIELDS.len(), 6);
    assert_eq!(SET_CONFIG_KEYS.len(), 13);
    assert_eq!(TUNNEL_ACTIONS.len(), 7);
    assert_eq!(TUNNEL_TYPES.len(), 12);
    assert_eq!(TUNNEL_OPTIONS.len(), 46);
    assert_eq!(SECRET_OPTIONS.len(), 4);
    assert_eq!(AuthErrorCode::ALL.len(), 6);
    assert_eq!(TunnelStatus::ALL.len(), 6);
    let inventory = ContractInventory::current();
    assert!(conformance::assert_frozen_counts(&inventory));
    // Canonical JSON round-trips deterministically.
    let first = inventory.to_canonical_json();
    let second = ContractInventory::current().to_canonical_json();
    assert_eq!(first, second);
}

#[test]
fn proposal_170_wire_inventory_is_exact_and_unique() {
    assert_eq!(PROPOSAL_ROUTER_INFO_FIELDS.len(), 43);
    let mut keys = std::collections::BTreeSet::new();
    for field in PROPOSAL_ROUTER_INFO_FIELDS {
        assert!(
            keys.insert(field.key),
            "duplicate canonical key: {}",
            field.key
        );
        assert!(proposal_wire::router_info_field(field.key).is_some());
    }
    for field in BASE_ROUTER_INFO_FIELDS {
        assert!(
            keys.insert(field.key),
            "base selector overlaps proposal key: {}",
            field.key
        );
        assert!(proposal_wire::router_info_field(field.key).is_some());
    }
    assert_eq!(
        proposal_wire::router_info_field("i2p.router.version")
            .unwrap()
            .value_type,
        proposal_wire::ProposalValueType::String
    );
    assert_eq!(
        proposal_wire::router_info_field("i2p.router.netdb.isreseeding")
            .unwrap()
            .value_type,
        proposal_wire::ProposalValueType::Boolean
    );
    assert_eq!(PROPOSAL_ADDRESS_BOOK_CONFIG_KEYS.len(), 13);
    assert_eq!(
        PROPOSAL_ADDRESS_BOOK_CONFIG_KEYS,
        [
            "subscriptions",
            "update_delay",
            "published_addressbook",
            "router_addressbook",
            "local_addressbook",
            "private_addressbook",
            "proxy_port",
            "proxy_host",
            "should_publish",
            "etags",
            "last_modified",
            "log",
            "theme",
        ]
    );
    assert!(proposal_wire::router_info_field("router.version").is_none());
    assert!(proposal_wire::router_info_field("I2P.router.news").is_none());
    let tunnel_fields: std::collections::BTreeSet<_> =
        PROPOSAL_TUNNEL_MANAGER_FIELDS.iter().copied().collect();
    assert_eq!(tunnel_fields.len(), PROPOSAL_TUNNEL_MANAGER_FIELDS.len());
    assert!(tunnel_fields.contains(&"Action"));
    assert!(tunnel_fields.contains(&"All"));
    assert!(tunnel_fields.contains(&"OptionalLookup"));
    assert!(!tunnel_fields.contains(&"options"));
    assert_eq!(PROPOSAL_ENCRYPT_LEASE_SET_VALUES.len(), 10);
    assert_eq!(PROPOSAL_TUNNEL_INTEGER_RANGES.len(), 19);
    assert_eq!(PROPOSAL_TUNNEL_POLICY_INTEGER_RANGES.len(), 3);
}

#[test]
fn proposal_tunnel_wire_types_ranges_and_compound_values_are_checked() {
    for range in PROPOSAL_TUNNEL_INTEGER_RANGES
        .iter()
        .chain(PROPOSAL_TUNNEL_POLICY_INTEGER_RANGES.iter())
    {
        assert!(
            proposal_wire::validate_proposal_tunnel_value(
                range.key,
                &serde_json::json!(range.minimum)
            )
            .is_ok(),
            "{} minimum is inclusive",
            range.key
        );
        assert!(
            proposal_wire::validate_proposal_tunnel_value(
                range.key,
                &serde_json::json!(range.maximum)
            )
            .is_ok(),
            "{} maximum is inclusive",
            range.key
        );
        assert!(
            proposal_wire::validate_proposal_tunnel_value(
                range.key,
                &serde_json::json!(range.minimum - 1)
            )
            .is_err(),
            "{} rejects min-1",
            range.key
        );
        assert!(
            proposal_wire::validate_proposal_tunnel_value(
                range.key,
                &serde_json::json!(range.maximum + 1)
            )
            .is_err(),
            "{} rejects max+1",
            range.key
        );
    }
    for value in PROPOSAL_ENCRYPT_LEASE_SET_VALUES {
        assert!(
            proposal_wire::validate_proposal_tunnel_value(
                "EncryptLeaseSet",
                &serde_json::json!(value)
            )
            .is_ok()
        );
    }
    assert!(
        proposal_wire::validate_proposal_tunnel_value(
            "EncryptLeaseSet",
            &serde_json::json!("encrypted (unknown)")
        )
        .is_err()
    );
    assert!(
        proposal_wire::validate_proposal_tunnel_value("StartOnLoad", &serde_json::json!(true))
            .is_ok()
    );
    assert!(
        proposal_wire::validate_proposal_tunnel_value("StartOnLoad", &serde_json::json!(1))
            .is_err()
    );
    assert!(
        proposal_wire::validate_proposal_tunnel_value("ConnectDelay", &serde_json::json!(false))
            .is_ok()
    );
    assert!(
        proposal_wire::validate_proposal_tunnel_value("NewDest", &serde_json::json!(2)).is_ok()
    );
    assert!(
        proposal_wire::validate_proposal_tunnel_value("NewDest", &serde_json::json!(3)).is_err()
    );
    assert!(
        proposal_wire::validate_proposal_tunnel_value("JumpList", &serde_json::json!("true"))
            .is_ok()
    );
    assert!(
        proposal_wire::validate_proposal_tunnel_value("JumpList", &serde_json::json!(true))
            .is_err()
    );
    assert!(
        proposal_wire::validate_proposal_tunnel_value(
            "LeaseSetClientAuths",
            &serde_json::json!([{"Name": "client", "Key": "secret"}])
        )
        .is_ok()
    );
    assert!(
        proposal_wire::validate_proposal_tunnel_value(
            "LeaseSetClientAuths",
            &serde_json::json!([{"Name": 1, "Key": "secret"}])
        )
        .is_err()
    );
}

#[test]
fn methods_parse_exactly_and_classify() {
    assert_eq!(Method::parse("Authenticate"), Ok(Method::Authenticate));
    assert_eq!(Method::parse("RouterInfo"), Ok(Method::RouterInfo));
    assert_eq!(Method::parse("AddressBook"), Ok(Method::AddressBook));
    assert_eq!(Method::parse("TunnelManager"), Ok(Method::TunnelManager));
    assert_eq!(
        Method::parse("ClientServicesInfo"),
        Ok(Method::ClientServicesInfo)
    );
    assert!(!Method::Authenticate.requires_token());
    for method in [
        Method::RouterInfo,
        Method::AddressBook,
        Method::TunnelManager,
        Method::ClientServicesInfo,
    ] {
        assert!(method.requires_token());
    }
    assert!(Method::check_auth_spelling());
    // Canonical order is deterministic.
    let mut indexed: Vec<(usize, &str)> =
        METHODS.iter().enumerate().map(|(i, m)| (i, *m)).collect();
    indexed.sort_by_key(|(i, _)| *i);
    assert_eq!(indexed[0].1, "Authenticate");
    assert_eq!(indexed[4].1, "ClientServicesInfo");
}

#[test]
fn unknown_and_case_methods_fail_typed() {
    assert_eq!(
        Method::parse("GetVersion"),
        Err(ContractError::UnknownLiteral)
    );
    assert_eq!(Method::parse(""), Err(ContractError::UnknownLiteral));
    assert_eq!(
        Method::parse("authenticate"),
        Err(ContractError::CaseMismatch)
    );
    assert_eq!(
        Method::parse("ROUTERINFO"),
        Err(ContractError::CaseMismatch)
    );
    assert_eq!(
        Method::parse("tunnelmanager"),
        Err(ContractError::CaseMismatch)
    );
}

#[test]
fn tunnel_actions_and_types_parse_exactly() {
    for (index, name) in TUNNEL_ACTIONS.iter().enumerate() {
        let parsed = TunnelAction::parse(name).expect("known action parses");
        assert_eq!(parsed.name(), *name);
        let _ = index;
    }
    for name in TUNNEL_TYPES {
        let parsed = TunnelType::parse(name).expect("known type parses");
        assert_eq!(parsed.name(), name);
    }
    // Six Plan 289 backends are real; the six composed/Streamr families are not yet.
    let backend_count = TUNNEL_TYPES
        .iter()
        .filter(|name| {
            TunnelType::parse(name)
                .expect("known")
                .has_plan289_backend()
        })
        .count();
    assert_eq!(backend_count, 6);
    // Plan 290 adds the four composed families; only the two
    // Streamr families stay without a backend until Plan 291.
    let backend290_count = TUNNEL_TYPES
        .iter()
        .filter(|name| {
            TunnelType::parse(name)
                .expect("known")
                .has_plan290_backend()
        })
        .count();
    assert_eq!(backend290_count, 10);
    // Plan 291 adds the two Streamr families; all twelve Proposal
    // types have backends and Plan 292 becomes ready.
    let backend291_count = TUNNEL_TYPES
        .iter()
        .filter(|name| {
            TunnelType::parse(name)
                .expect("known")
                .has_plan291_backend()
        })
        .count();
    assert_eq!(backend291_count, 12);
    assert_eq!(
        TunnelAction::parse("CREATE"),
        Err(ContractError::CaseMismatch)
    );
    assert_eq!(
        TunnelAction::parse("launch"),
        Err(ContractError::UnknownLiteral)
    );
    assert_eq!(
        TunnelType::parse("Client"),
        Err(ContractError::CaseMismatch)
    );
    assert_eq!(
        TunnelType::parse("stream"),
        Err(ContractError::UnknownLiteral)
    );
    // Get is the only read action.
    assert!(!TunnelAction::Get.is_mutating());
    for action in [
        TunnelAction::Create,
        TunnelAction::Edit,
        TunnelAction::Delete,
        TunnelAction::Start,
        TunnelAction::Stop,
        TunnelAction::Restart,
    ] {
        assert!(action.is_mutating());
    }
}

#[test]
fn router_info_selectors_parse_and_type() {
    for name in ROUTER_INFO_SELECTORS {
        let parsed = RouterInfoSelector::parse(name).expect("known selector parses");
        assert_eq!(parsed.name(), name);
        let _ = parsed.return_type();
    }
    // Address-book selectors are exactly six and flagged.
    let address_book: Vec<_> = ROUTER_INFO_SELECTORS
        .iter()
        .filter(|name| {
            RouterInfoSelector::parse(name)
                .expect("known")
                .is_address_book()
        })
        .collect();
    assert_eq!(address_book.len(), 6);
    // Return-type metadata is total: spot-check each family.
    assert_eq!(
        RouterInfoSelector::RouterVersion.return_type(),
        ReturnType::String
    );
    assert_eq!(
        RouterInfoSelector::RouterUptime.return_type(),
        ReturnType::Integer
    );
    assert_eq!(
        RouterInfoSelector::NetDbKnownPeers.return_type(),
        ReturnType::List
    );
    assert_eq!(
        RouterInfoSelector::AddressBookPrivate.return_type(),
        ReturnType::Map
    );
    assert_eq!(
        RouterInfoSelector::parse("Router.Version"),
        Err(ContractError::CaseMismatch)
    );
    assert_eq!(
        RouterInfoSelector::parse("router.unknown"),
        Err(ContractError::UnknownLiteral)
    );
}

#[test]
fn client_services_parse_and_bob_is_constant() {
    for name in CLIENT_SERVICES {
        let parsed = ClientService::parse(name).expect("known service parses");
        assert_eq!(parsed.name(), name);
    }
    assert!(ClientService::Bob.is_constant());
    assert!(ClientService::I2pTunnel.is_service_map());
    assert!(!ClientService::Sam.is_constant());
    assert_eq!(
        ClientService::parse("sam"),
        Err(ContractError::CaseMismatch)
    );
    assert_eq!(
        ClientService::parse("BOB2"),
        Err(ContractError::UnknownLiteral)
    );
}

#[test]
fn address_book_books_fields_and_config_keys() {
    for name in BOOK_TYPES {
        let parsed = BookType::parse(name).expect("known book parses");
        assert_eq!(parsed.name(), name);
    }
    for name in ADDRESS_BOOK_FIELDS {
        let parsed = AddressBookField::parse(name).expect("known field parses");
        assert_eq!(parsed.name(), name);
    }
    assert_eq!(SET_CONFIG_KEYS.len(), 13);
    for (index, key) in SET_CONFIG_KEYS.iter().enumerate() {
        assert_eq!(
            i2pr_i2pcontrol::address_book::parse_set_config_key(key),
            Ok(index)
        );
    }
    assert_eq!(BookType::parse("Private"), Err(ContractError::CaseMismatch));
    assert_eq!(
        BookType::parse("shared"),
        Err(ContractError::UnknownLiteral)
    );
    assert_eq!(
        AddressBookField::parse("hostname"),
        Err(ContractError::CaseMismatch)
    );
    assert_eq!(
        i2pr_i2pcontrol::address_book::parse_set_config_key("PRIVATE_ADDRESSBOOK"),
        Err(ContractError::CaseMismatch)
    );
    assert_eq!(
        i2pr_i2pcontrol::address_book::parse_set_config_key("nope"),
        Err(ContractError::UnknownLiteral)
    );
    // Path-like and inert classifications are disjoint and total on their rows.
    assert!(i2pr_i2pcontrol::address_book::is_path_like_config_key(
        "private_addressbook"
    ));
    assert!(i2pr_i2pcontrol::address_book::is_inert_config_key("theme"));
    assert!(!i2pr_i2pcontrol::address_book::is_path_like_config_key(
        "theme"
    ));
}

#[test]
fn secret_classification_is_total() {
    // Every option has an explicit sensitivity; the secret set is exactly four.
    let mut secret_count = 0;
    for option in TUNNEL_OPTIONS {
        match option.sensitivity {
            i2pr_i2pcontrol::OptionSensitivity::Secret => secret_count += 1,
            i2pr_i2pcontrol::OptionSensitivity::Public => {}
        }
        // Every option applies to at least one family.
        assert!(
            option.applies_mask != 0,
            "option applies nowhere: {}",
            option.name
        );
        // Lookup round-trips.
        assert_eq!(
            tunnel_options::find_option(option.name)
                .expect("known")
                .name,
            option.name
        );
    }
    assert_eq!(secret_count, SECRET_OPTIONS.len());
    assert_eq!(secret_count, 4);
    for secret in SECRET_OPTIONS {
        let option = tunnel_options::find_option(secret).expect("secret is a known option");
        assert_eq!(
            option.sensitivity,
            i2pr_i2pcontrol::OptionSensitivity::Secret
        );
    }
    assert_eq!(
        tunnel_options::find_option("TARGET_HOST"),
        Err(ContractError::CaseMismatch)
    );
    assert_eq!(
        tunnel_options::find_option("nope"),
        Err(ContractError::UnknownLiteral)
    );
}

#[test]
fn auth_vocabulary_and_errors() {
    assert_eq!(i2pr_i2pcontrol::AUTHENTICATE_METHOD, "Authenticate");
    assert_eq!(TOKEN_BYTES, 32);
    assert_eq!(TOKEN_LIFETIME_SECS, 86_400);
    assert_eq!(MAX_LIVE_TOKENS, 1_024);
    assert_eq!(MAX_PRESENTED_TOKEN_LEN, 256);
    let codes: Vec<i64> = AuthErrorCode::ALL.iter().map(|code| code.code()).collect();
    assert_eq!(
        codes,
        vec![-32_001, -32_002, -32_003, -32_004, -32_005, -32_006]
    );
    for code in AuthErrorCode::ALL {
        assert_eq!(AuthErrorCode::from_code(code.code()), Some(code));
    }
    assert_eq!(AuthErrorCode::from_code(-32_000), None);
    assert!(auth::validate_presented_token("abc").is_ok());
    assert_eq!(
        auth::validate_presented_token(""),
        Err(ContractError::Malformed)
    );
    let oversized = "t".repeat(MAX_PRESENTED_TOKEN_LEN + 1);
    assert_eq!(
        auth::validate_presented_token(&oversized),
        Err(ContractError::OverBound)
    );
    let at_max = "t".repeat(MAX_PRESENTED_TOKEN_LEN);
    assert!(auth::validate_presented_token(&at_max).is_ok());
}

#[test]
fn jsonrpc_ids_and_envelopes() {
    // Id shapes: string / integer / explicit null.
    let string_id = RequestId::from_json(&serde_json::json!("abc")).expect("string id");
    assert_eq!(string_id, RequestId::String("abc".to_string()));
    let int_id = RequestId::from_json(&serde_json::json!(7)).expect("int id");
    assert_eq!(int_id, RequestId::Integer(7));
    let null_id = RequestId::from_json(&serde_json::json!(null)).expect("null id");
    assert_eq!(null_id, RequestId::Null);
    assert!(RequestId::from_json(&serde_json::json!(1.5)).is_err());
    assert!(RequestId::from_json(&serde_json::json!(true)).is_err());
    // Oversized string id fails.
    let big = "i".repeat(MAX_ID_STRING_LEN + 1);
    assert_eq!(
        RequestId::from_json(&serde_json::json!(big)),
        Err(ContractError::OverBound)
    );
    // Exact request decode: absent id = notification; explicit null = request.
    let notification = serde_json::json!({"jsonrpc": "2.0", "method": "RouterInfo", "params": {}});
    let decoded = JsonRpcRequest::decode(&notification).expect("notification decodes");
    assert!(decoded.is_notification());
    let null_request =
        serde_json::json!({"jsonrpc": "2.0", "method": "RouterInfo", "params": {}, "id": null});
    let decoded = JsonRpcRequest::decode(&null_request).expect("null id decodes");
    assert!(!decoded.is_notification());
    assert_eq!(decoded.id, Some(RequestId::Null));
    // Positional params rejected; unknown top-level members rejected.
    let positional = serde_json::json!({"jsonrpc": "2.0", "method": "RouterInfo", "params": [1]});
    assert_eq!(
        JsonRpcRequest::decode(&positional),
        Err(ContractError::Malformed)
    );
    let extra =
        serde_json::json!({"jsonrpc": "2.0", "method": "RouterInfo", "params": {}, "extra": 1});
    assert_eq!(
        JsonRpcRequest::decode(&extra),
        Err(ContractError::Malformed)
    );
    let wrong_version = serde_json::json!({"jsonrpc": "1.0", "method": "RouterInfo", "params": {}});
    assert_eq!(
        JsonRpcRequest::decode(&wrong_version),
        Err(ContractError::Malformed)
    );
    // Error codes round-trip.
    for code in [
        JsonRpcErrorCode::ParseError,
        JsonRpcErrorCode::InvalidRequest,
        JsonRpcErrorCode::MethodNotFound,
        JsonRpcErrorCode::InvalidParams,
        JsonRpcErrorCode::InternalError,
    ] {
        assert_eq!(JsonRpcErrorCode::from_code(code.code()), Some(code));
    }
    // Success/error envelopes preserve id shape.
    let envelope = jsonrpc::success_envelope(
        Some(&RequestId::Integer(3)),
        serde_json::json!({"ok": true}),
    );
    assert_eq!(envelope["id"], serde_json::json!(3));
    let envelope = jsonrpc::error_envelope(Some(&RequestId::Null), -32_603, "Internal error");
    assert_eq!(envelope["id"], serde_json::Value::Null);
    // Batch split: arrays are batches (empty allowed here; daemon maps empty
    // to one invalid-request response), objects are singles.
    let empty_batch = serde_json::json!([]);
    let (is_batch, elements) = jsonrpc::split_body(&empty_batch).expect("empty batch splits");
    assert!(is_batch && elements.is_empty());
    let single_body = serde_json::json!({});
    let (is_batch, _) = jsonrpc::split_body(&single_body).expect("single splits");
    assert!(!is_batch);
    let malformed_body = serde_json::json!(1);
    assert!(jsonrpc::split_body(&malformed_body).is_err());
}

#[test]
fn max_and_max_plus_one_bounds() {
    // Method names.
    let at_max = "m".repeat(MAX_METHOD_NAME_LEN);
    let over_max = "m".repeat(MAX_METHOD_NAME_LEN + 1);
    let body = |method: &str| serde_json::json!({"jsonrpc": "2.0", "method": method, "params": {}});
    assert!(JsonRpcRequest::decode(&body(&at_max)).is_ok());
    assert_eq!(
        JsonRpcRequest::decode(&body(&over_max)),
        Err(ContractError::OverBound)
    );
    // Tunnel names.
    let name_max = "n".repeat(MAX_TUNNEL_NAME_LEN);
    assert!(tunnel::validate_tunnel_name(&name_max).is_ok());
    let name_over = "n".repeat(MAX_TUNNEL_NAME_LEN + 1);
    assert_eq!(
        tunnel::validate_tunnel_name(&name_over),
        Err(ContractError::OverBound)
    );
    assert_eq!(
        tunnel::validate_tunnel_name(""),
        Err(ContractError::Malformed)
    );
    assert_eq!(
        tunnel::validate_tunnel_name("a/b"),
        Err(ContractError::Malformed)
    );
    // Generic strings and collections.
    assert!(limits::check_str(&"s".repeat(MAX_STRING_LEN), MAX_STRING_LEN).is_ok());
    assert_eq!(
        limits::check_str(&"s".repeat(MAX_STRING_LEN + 1), MAX_STRING_LEN),
        Err(ContractError::OverBound)
    );
    assert!(limits::check_len(MAX_LIST_ITEMS, MAX_LIST_ITEMS).is_ok());
    assert_eq!(
        limits::check_len(MAX_LIST_ITEMS + 1, MAX_LIST_ITEMS),
        Err(ContractError::OverBound)
    );
    assert!(limits::check_len(MAX_MAP_ENTRIES, MAX_MAP_ENTRIES).is_ok());
    assert_eq!(
        limits::check_len(MAX_MAP_ENTRIES + 1, MAX_MAP_ENTRIES),
        Err(ContractError::OverBound)
    );
    assert!(limits::check_len(MAX_PARAMS_KEYS, MAX_PARAMS_KEYS).is_ok());
    assert_eq!(
        limits::check_len(MAX_PARAMS_KEYS + 1, MAX_PARAMS_KEYS),
        Err(ContractError::OverBound)
    );
    // Password/token/option/hostname/destination ceilings.
    assert!(limits::check_str(&"p".repeat(MAX_PASSWORD_LEN), MAX_PASSWORD_LEN).is_ok());
    assert_eq!(
        limits::check_str(&"p".repeat(MAX_PASSWORD_LEN + 1), MAX_PASSWORD_LEN),
        Err(ContractError::OverBound)
    );
    assert!(limits::check_str(&"v".repeat(MAX_OPTION_VALUE_LEN), MAX_OPTION_VALUE_LEN).is_ok());
    assert_eq!(
        limits::check_str(&"v".repeat(MAX_OPTION_VALUE_LEN + 1), MAX_OPTION_VALUE_LEN),
        Err(ContractError::OverBound)
    );
    assert!(limits::check_str(&"h".repeat(MAX_HOSTNAME_LEN), MAX_HOSTNAME_LEN).is_ok());
    assert_eq!(
        limits::check_str(&"h".repeat(MAX_HOSTNAME_LEN + 1), MAX_HOSTNAME_LEN),
        Err(ContractError::OverBound)
    );
    assert!(limits::check_str(&"d".repeat(MAX_DESTINATION_LEN), MAX_DESTINATION_LEN).is_ok());
    assert_eq!(
        limits::check_str(&"d".repeat(MAX_DESTINATION_LEN + 1), MAX_DESTINATION_LEN),
        Err(ContractError::OverBound)
    );
    assert!(
        limits::check_str(
            &"u".repeat(MAX_SUBSCRIPTION_URL_LEN),
            MAX_SUBSCRIPTION_URL_LEN
        )
        .is_ok()
    );
    assert_eq!(
        limits::check_str(
            &"u".repeat(MAX_SUBSCRIPTION_URL_LEN + 1),
            MAX_SUBSCRIPTION_URL_LEN
        ),
        Err(ContractError::OverBound)
    );
    assert!(limits::check_len(MAX_SUBSCRIPTION_URLS, MAX_SUBSCRIPTION_URLS).is_ok());
    assert_eq!(
        limits::check_len(MAX_SUBSCRIPTION_URLS + 1, MAX_SUBSCRIPTION_URLS),
        Err(ContractError::OverBound)
    );
    assert!(limits::check_len(MAX_TUNNEL_DEFS, MAX_TUNNEL_DEFS).is_ok());
    assert_eq!(
        limits::check_len(MAX_TUNNEL_DEFS + 1, MAX_TUNNEL_DEFS),
        Err(ContractError::OverBound)
    );
    assert!(limits::check_len(MAX_OPTIONS_PER_TUNNEL, MAX_OPTIONS_PER_TUNNEL).is_ok());
    assert_eq!(
        limits::check_len(MAX_OPTIONS_PER_TUNNEL + 1, MAX_OPTIONS_PER_TUNNEL),
        Err(ContractError::OverBound)
    );
    assert!(limits::check_str(&"o".repeat(MAX_OPTION_NAME_LEN), MAX_OPTION_NAME_LEN).is_ok());
    assert_eq!(
        limits::check_str(&"o".repeat(MAX_OPTION_NAME_LEN + 1), MAX_OPTION_NAME_LEN),
        Err(ContractError::OverBound)
    );
    assert!(limits::check_str(&"s".repeat(MAX_SELECTOR_LEN), MAX_SELECTOR_LEN).is_ok());
    assert_eq!(
        limits::check_str(&"s".repeat(MAX_SELECTOR_LEN + 1), MAX_SELECTOR_LEN),
        Err(ContractError::OverBound)
    );
    assert!(limits::check_str(&"k".repeat(MAX_MAP_KEY_LEN), MAX_MAP_KEY_LEN).is_ok());
    assert_eq!(
        limits::check_str(&"k".repeat(MAX_MAP_KEY_LEN + 1), MAX_MAP_KEY_LEN),
        Err(ContractError::OverBound)
    );
    // Initial daemon ceilings carried by the contract.
    assert_eq!(MAX_HTTP_BODY_BYTES, 1_048_576);
    assert_eq!(MAX_BATCH_ELEMENTS, 32);
    assert_eq!(MAX_INFLIGHT_REQUESTS, 64);
}

#[test]
fn plan288_source_matrix_mirrors_frozen_inventories() {
    use i2pr_i2pcontrol::{
        CLIENT_SERVICES_SOURCE_MATRIX, ROUTER_INFO_SOURCE_MATRIX, SourceAvailability,
        matrix_mirrors_inventories, selector_index, service_index, service_row, source_row,
    };
    assert!(matrix_mirrors_inventories());
    assert_eq!(ROUTER_INFO_SOURCE_MATRIX.len(), 30);
    assert_eq!(CLIENT_SERVICES_SOURCE_MATRIX.len(), 6);
    // Every selector resolves through its canonical index.
    for selector in [
        RouterInfoSelector::RouterVersion,
        RouterInfoSelector::RouterApiVersion,
        RouterInfoSelector::RouterUptime,
        RouterInfoSelector::RouterStatus,
        RouterInfoSelector::RouterNetworkId,
        RouterInfoSelector::RouterHash,
        RouterInfoSelector::NetDbKnownPeers,
        RouterInfoSelector::NetDbActivePeers,
        RouterInfoSelector::NetDbFloodfillMode,
        RouterInfoSelector::Ntcp2ActivePeers,
        RouterInfoSelector::Ssu2ActiveSessions,
        RouterInfoSelector::Reachability,
        RouterInfoSelector::TransportErrors,
        RouterInfoSelector::ExploratoryCount,
        RouterInfoSelector::ClientCount,
        RouterInfoSelector::ParticipatingCount,
        RouterInfoSelector::BuildQueue,
        RouterInfoSelector::SuccessRate,
        RouterInfoSelector::Bandwidth,
        RouterInfoSelector::AddressBookPrivate,
        RouterInfoSelector::AddressBookLocal,
        RouterInfoSelector::AddressBookRouter,
        RouterInfoSelector::AddressBookPublished,
        RouterInfoSelector::AddressBookSubscriptions,
        RouterInfoSelector::AddressBookConfig,
        RouterInfoSelector::LogsRecent,
        RouterInfoSelector::NewsFeed,
        RouterInfoSelector::ClockSkew,
        RouterInfoSelector::BannedPeers,
        RouterInfoSelector::Rates,
    ] {
        let row = source_row(selector);
        assert_eq!(row.key, selector.name());
        assert_eq!(row.return_type, selector.return_type());
        assert_eq!(
            ROUTER_INFO_SOURCE_MATRIX[selector_index(selector)].key,
            selector.name()
        );
    }
    for service in [
        ClientService::I2pTunnel,
        ClientService::HttpProxy,
        ClientService::Socks,
        ClientService::Sam,
        ClientService::Bob,
        ClientService::I2cp,
    ] {
        let row = service_row(service);
        assert_eq!(row.key, service.name());
        assert_eq!(
            CLIENT_SERVICES_SOURCE_MATRIX[service_index(service)].key,
            service.name()
        );
    }
    // Availability census: 27 live (5 base + 6 Plan 294 address-book
    // + 16 Plan 295 sources) + 1 publish-gated (router.hash, in-plan
    // 288) + 1 unavailable (router news, unsupported by Plan 295
    // determination) + 1 permitted-neutral (clock skew, justified by
    // Plan 295 from the Proposal's null allowance).
    let mut available = 0;
    let mut gated = 0;
    let mut gated_288 = 0;
    let mut gated_295 = 0;
    let mut unavailable = 0;
    let mut neutral = 0;
    let mut unavailable_294 = 0;
    let mut unavailable_295 = 0;
    for row in ROUTER_INFO_SOURCE_MATRIX {
        match row.availability {
            SourceAvailability::Available => available += 1,
            SourceAvailability::PublishedGated { owner_plan, .. } => {
                gated += 1;
                match owner_plan {
                    // Only router.hash gates inside Plan 288 (identity
                    // publication wiring in this plan's scope).
                    "288" => {
                        assert_eq!(row.key, "router.hash");
                        gated_288 += 1;
                    }
                    "295" => gated_295 += 1,
                    other => panic!("row {} gates on an unexpected plan {other}", row.key),
                }
            }
            SourceAvailability::PermittedNeutral { .. } => neutral += 1,
            SourceAvailability::Unavailable { owner_plan, .. } => {
                unavailable += 1;
                match owner_plan {
                    "294" => unavailable_294 += 1,
                    "295" => unavailable_295 += 1,
                    other => panic!("row {} names unexpected owner plan {other}", row.key),
                }
            }
        }
        assert!(!row.owner.is_empty());
        assert!(!row.snapshot.is_empty());
        assert!(!row.test_id.is_empty());
        assert!(row.max_bytes > 0);
    }
    // router.hash gates on Plan 288 itself (identity publication wiring).
    assert!(
        matches!(
            source_row(RouterInfoSelector::RouterHash).availability,
            SourceAvailability::PublishedGated {
                owner_plan: "288",
                ..
            }
        ),
        "router.hash must be Plan 288 publish-gated"
    );
    assert_eq!(available, 27);
    assert_eq!(gated_288, 1);
    assert_eq!(gated_295, 0);
    assert_eq!(gated, gated_288 + gated_295);
    assert_eq!(unavailable, 1);
    assert_eq!(unavailable_294, 0);
    assert_eq!(unavailable_295, 1);
    assert_eq!(neutral, 1);
    assert_eq!(
        i2pr_i2pcontrol::SOURCE_MATRIX_NEUTRAL_COUNT,
        neutral,
        "neutral census must stay machine-checked"
    );
    // All six service rows are answerable (disabled is truthful state).
    for row in CLIENT_SERVICES_SOURCE_MATRIX {
        assert_eq!(row.availability, SourceAvailability::Available);
    }
}

#[test]
fn plan322_source_matrix_covers_all_canonical_additions_and_marks_gaps() {
    use std::collections::BTreeSet;

    use i2pr_i2pcontrol::{
        PROPOSAL_ROUTER_INFO_FIELDS, SourceAvailability, proposal_router_info_source_matrix,
    };

    let rows = proposal_router_info_source_matrix();
    assert_eq!(rows.len(), 43);
    assert_eq!(
        rows.iter().map(|row| row.key).collect::<Vec<_>>(),
        PROPOSAL_ROUTER_INFO_FIELDS
            .iter()
            .map(|field| field.key)
            .collect::<Vec<_>>()
    );
    let keys = rows.iter().map(|row| row.key).collect::<BTreeSet<_>>();
    assert_eq!(keys.len(), 43, "canonical Proposal keys are unique");
    for row in &rows {
        assert!(!row.owner.is_empty(), "{} owner", row.key);
        assert!(!row.snapshot.is_empty(), "{} snapshot", row.key);
        assert!(!row.sensitivity.is_empty(), "{} sensitivity", row.key);
        assert!(!row.freshness.is_empty(), "{} freshness", row.key);
        assert!(row.max_bytes > 0, "{} byte ceiling", row.key);
        match row.availability {
            SourceAvailability::Unavailable { reason, .. } => {
                assert!(
                    row.evidence_test.is_none(),
                    "{} has no source test",
                    row.key
                );
                assert!(!reason.is_empty(), "{} unavailable reason", row.key);
            }
            SourceAvailability::PublishedGated { .. }
            | SourceAvailability::PermittedNeutral { .. }
            | SourceAvailability::Available => {
                assert!(row.evidence_test.is_some(), "{} source evidence", row.key);
            }
        }
    }
    assert!(matches!(
        rows.iter()
            .find(|row| row.key == "i2p.router.news")
            .unwrap()
            .availability,
        SourceAvailability::Unavailable {
            owner_plan: "322",
            ..
        }
    ));
    for key in [
        "i2p.router.net.total.received.bytes",
        "i2p.router.net.total.sent.bytes",
    ] {
        assert!(matches!(
            rows.iter().find(|row| row.key == key).unwrap().availability,
            SourceAvailability::PublishedGated {
                owner_plan: "322",
                ..
            }
        ));
    }
    assert_eq!(
        rows.iter()
            .filter(|row| matches!(row.availability, SourceAvailability::Unavailable { .. }))
            .count(),
        16,
        "unimplemented canonical fields remain explicit gaps"
    );
}

#[test]
fn plan289_tunnel_request_envelope_rules() {
    use i2pr_i2pcontrol::{TunnelAction, TunnelRequestError, TunnelType, decode_tunnel_request};

    fn params(value: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
        value.as_object().expect("object").clone()
    }

    // Canonical get requires a name; whole-inventory get is a nonstandard
    // extension and is not accepted by this endpoint.
    assert_eq!(
        decode_tunnel_request(&params(serde_json::json!({
            "Token": "t", "Action": "get",
        }))),
        Err(TunnelRequestError::MissingField("name"))
    );

    // get with name selects one tunnel; type/options/new_name forbidden.
    let request = decode_tunnel_request(&params(serde_json::json!({
        "Token": "t", "Action": "get", "Name": "alpha",
    })))
    .expect("named get decodes");
    assert_eq!(request.name.as_deref(), Some("alpha"));
    assert!(
        decode_tunnel_request(&params(serde_json::json!({
            "Action": "get", "Name": "alpha", "Type": "client",
        })))
        .is_err()
    );
    assert!(
        decode_tunnel_request(&params(serde_json::json!({
            "Action": "get", "Port": 8180,
        })))
        .is_err()
    );

    // create requires name + type; options validated against the universe.
    let request = decode_tunnel_request(&params(serde_json::json!({
        "Action": "create", "Name": "alpha", "Type": "httpclient",
        "Port": 8180, "StartOnLoad": true,
    })))
    .expect("create decodes");
    assert_eq!(request.action, TunnelAction::Create);
    assert_eq!(request.tunnel_type, Some(TunnelType::HttpClient));
    assert_eq!(
        request.options.get("listen_port").map(String::as_str),
        Some("8180")
    );
    assert_eq!(
        request.options.get("start_on_load").map(String::as_str),
        Some("true")
    );
    assert!(
        decode_tunnel_request(&params(serde_json::json!({
            "Action": "create", "Name": "alpha",
        })))
        .is_err(),
        "create without type fails"
    );
    assert!(
        decode_tunnel_request(&params(serde_json::json!({
            "Action": "create", "Type": "client",
        })))
        .is_err(),
        "create without name fails"
    );

    // Unknown option keys fail at the envelope; values are typed.
    assert_eq!(
        decode_tunnel_request(&params(serde_json::json!({
            "Action": "create", "Name": "a", "Type": "client",
            "NoSuchOption": "x",
        }))),
        Err(TunnelRequestError::UnknownKey("NoSuchOption".to_owned()))
    );
    assert!(
        decode_tunnel_request(&params(serde_json::json!({
            "Action": "create", "Name": "a", "Type": "client",
            "Port": null,
        })))
        .is_err(),
        "null option value fails"
    );
    assert!(
        decode_tunnel_request(&params(serde_json::json!({
            "Action": "create", "Name": "a", "Type": "client",
            "Port": [1],
        })))
        .is_err(),
        "array option value fails"
    );

    // edit: type immutable, rename via new_name, something must change.
    let request = decode_tunnel_request(&params(serde_json::json!({
        "Action": "edit", "Name": "a", "NewName": "b",
    })))
    .expect("rename edit decodes");
    assert_eq!(request.new_name.as_deref(), Some("b"));
    assert_eq!(
        decode_tunnel_request(&params(serde_json::json!({"Action": "edit", "Name": "a"}))),
        Err(TunnelRequestError::NothingToChange)
    );
    assert!(
        decode_tunnel_request(&params(serde_json::json!({
            "Action": "edit", "Name": "a", "Type": "server",
            "TargetHost": "127.0.0.1",
        })))
        .is_err(),
        "edit cannot change type"
    );

    // action spelling is exact and case-sensitive.
    assert!(decode_tunnel_request(&params(serde_json::json!({"Action": "Get"}))).is_err());
    assert!(decode_tunnel_request(&params(serde_json::json!({"Action": "launch"}))).is_err());
    assert!(decode_tunnel_request(&params(serde_json::json!({"Name": "a"}))).is_err());

    // Lifecycle actions require exactly a name.
    for action in ["delete", "start", "stop", "restart"] {
        let request = decode_tunnel_request(&params(serde_json::json!({
            "Action": action, "Name": "a",
        })))
        .expect("lifecycle decodes");
        assert_eq!(request.name.as_deref(), Some("a"));
        assert!(decode_tunnel_request(&params(serde_json::json!({"Action": action}))).is_err());
        assert!(
            decode_tunnel_request(&params(serde_json::json!({
                "Action": action, "Name": "a", "Port": 1,
            })))
            .is_err(),
            "{action} forbids options"
        );
    }

    // Closed envelope: unknown top-level keys fail.
    assert!(
        decode_tunnel_request(&params(serde_json::json!({
            "Action": "get", "verbose": true,
        })))
        .is_err(),
        "unknown keys fail"
    );

    // Names are validated; over-ceiling option maps fail.
    assert!(
        decode_tunnel_request(&params(serde_json::json!({
            "Action": "delete", "Name": "a/b",
        })))
        .is_err(),
        "path separators fail"
    );
    // The prior lowercase/nested-options extension is deliberately not
    // accepted in the canonical namespace.
    assert!(
        decode_tunnel_request(&params(serde_json::json!({
            "action": "create", "name": "a", "type": "client",
        })))
        .is_err()
    );
    assert_eq!(
        decode_tunnel_request(&params(serde_json::json!({
            "Action": "create", "Name": "a", "Type": "client",
            "options": {"Port": 1},
        }))),
        Err(TunnelRequestError::UnknownKey("options".to_owned()))
    );
    assert_eq!(
        decode_tunnel_request(&params(serde_json::json!({
            "Action": "create", "Name": "a", "Type": "client",
            "Host": "127.0.0.1", "TargetHost": "127.0.0.1",
        }))),
        Err(TunnelRequestError::DuplicateAlias("TargetHost".to_owned()))
    );
    let all = decode_tunnel_request(&params(serde_json::json!({
        "Action": "stop", "All": true,
    })))
    .expect("All is a canonical stop parameter");
    assert!(all.all);
    assert_eq!(all.name, None);
    assert_eq!(
        decode_tunnel_request(&params(serde_json::json!({
            "Action": "stop", "All": true, "Name": "a",
        }))),
        Err(TunnelRequestError::UnexpectedField("name"))
    );
    let description = decode_tunnel_request(&params(serde_json::json!({
        "Action": "create", "Name": "a", "Type": "server",
        "Description": "valid Proposal field",
    })))
    .expect("Description has a typed control-plane metadata owner");
    assert_eq!(
        description.options.get("description").map(String::as_str),
        Some("valid Proposal field")
    );
    let max_concurrent = decode_tunnel_request(&params(serde_json::json!({
        "Action": "create", "Name": "a", "Type": "server",
        "MaxConcurrentConns": 24,
    })))
    .expect("MaxConcurrentConns has a bounded service admission owner");
    assert_eq!(
        max_concurrent
            .options
            .get("max_streams")
            .map(String::as_str),
        Some("24")
    );
    let proxy_auth = decode_tunnel_request(&params(serde_json::json!({
        "Action": "create", "Name": "a", "Type": "socks",
        "ProxyAuth": true,
    })))
    .expect("ProxyAuth has a typed proxy credential owner");
    assert_eq!(
        proxy_auth.options.get("proxy_auth").map(String::as_str),
        Some("true")
    );
    let multihoming = decode_tunnel_request(&params(serde_json::json!({
        "Action": "create", "Name": "a", "Type": "server",
        "MultiHoming": false,
    })))
    .expect("MultiHoming has an explicit capitalization adapter");
    assert_eq!(
        multihoming.options.get("multihoming").map(String::as_str),
        Some("false")
    );
    assert_eq!(
        decode_tunnel_request(&params(serde_json::json!({
            "Action": "create", "Name": "a", "Type": "server",
            "MaxConcurrentConns": "24",
        }))),
        Err(TunnelRequestError::BadValue(
            "MaxConcurrentConns".to_owned()
        ))
    );
    assert_eq!(
        decode_tunnel_request(&params(serde_json::json!({
            "Action": "create", "Name": "a", "Type": "server",
            "Description": "x".repeat(4097),
        }))),
        Err(TunnelRequestError::ValueOverBound("Description".to_owned()))
    );
    assert_eq!(
        decode_tunnel_request(&params(serde_json::json!({
            "Action": "create", "Name": "a", "Type": "server",
            "JumpList": "false",
        }))),
        Err(TunnelRequestError::UnavailableOption("JumpList".to_owned()))
    );
    for key in ["CustomOptions", "PrivKeyFile"] {
        assert_eq!(
            decode_tunnel_request(&params(serde_json::json!({
                "Action": "create", "Name": "a", "Type": "server",
                (key): "operator supplied value",
            }))),
            Err(TunnelRequestError::UnavailableOption(key.to_owned())),
            "{key} must not bypass typed validation or accept an arbitrary path"
        );
    }
    assert_eq!(
        decode_tunnel_request(&params(serde_json::json!({
            "Action": "create", "Name": "a", "Type": "server",
            "JumpList": false,
        }))),
        Err(TunnelRequestError::BadValue("JumpList".to_owned()))
    );
    assert_eq!(
        decode_tunnel_request(&params(serde_json::json!({
            "Action": "create", "Name": "a", "Type": "server",
            "EncryptLeaseSet": "encrypted (psk)",
        }))),
        Err(TunnelRequestError::UnavailableOption(
            "EncryptLeaseSet".to_owned()
        ))
    );
    assert_eq!(
        decode_tunnel_request(&params(serde_json::json!({
            "Action": "create", "Name": "a", "Type": "server",
            "LeaseSetClientAuths": [{"Name": "client", "Key": "secret"}],
        }))),
        Err(TunnelRequestError::UnavailableOption(
            "LeaseSetClientAuths".to_owned()
        ))
    );
    assert_eq!(
        decode_tunnel_request(&params(serde_json::json!({
            "Action": "create", "Name": "a", "Type": "server",
            "WebsiteHostname": "site.i2p", "SpoofedHost": "alias.i2p",
        }))),
        Err(TunnelRequestError::DuplicateAlias(
            "WebsiteHostname".to_owned()
        ))
    );
}
