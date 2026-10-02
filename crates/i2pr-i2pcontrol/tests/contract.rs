//! Plan 286 contract tests: exact inventories, deterministic parsing,
//! typed literal failures, max/max+1 bounds, secret classification.

use i2pr_i2pcontrol::{
    ADDRESS_BOOK_FIELDS, AddressBookField, AuthErrorCode, BOOK_TYPES, BookType, CLIENT_SERVICES,
    ClientService, ContractError, ContractInventory, JsonRpcErrorCode, JsonRpcRequest,
    MAX_BATCH_ELEMENTS, MAX_DESTINATION_LEN, MAX_HOSTNAME_LEN, MAX_HTTP_BODY_BYTES,
    MAX_ID_STRING_LEN, MAX_INFLIGHT_REQUESTS, MAX_LIST_ITEMS, MAX_LIVE_TOKENS, MAX_MAP_ENTRIES,
    MAX_MAP_KEY_LEN, MAX_METHOD_NAME_LEN, MAX_OPTION_NAME_LEN, MAX_OPTION_VALUE_LEN,
    MAX_OPTIONS_PER_TUNNEL, MAX_PARAMS_KEYS, MAX_PASSWORD_LEN, MAX_PRESENTED_TOKEN_LEN,
    MAX_SELECTOR_LEN, MAX_STRING_LEN, MAX_SUBSCRIPTION_URL_LEN, MAX_SUBSCRIPTION_URLS,
    MAX_TUNNEL_DEFS, MAX_TUNNEL_NAME_LEN, METHODS, Method, ROUTER_INFO_SELECTORS, RequestId,
    ReturnType, RouterInfoSelector, SECRET_OPTIONS, SET_CONFIG_KEYS, TOKEN_BYTES,
    TOKEN_LIFETIME_SECS, TUNNEL_ACTIONS, TUNNEL_OPTIONS, TUNNEL_TYPES, TunnelAction, TunnelStatus,
    TunnelType, auth, conformance, jsonrpc, limits, tunnel, tunnel_options,
};

#[test]
fn frozen_counts_match_plan_286() {
    assert_eq!(METHODS.len(), 5);
    assert_eq!(ROUTER_INFO_SELECTORS.len(), 30);
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
        i2pr_i2pcontrol::address_book::parse_set_config_key("PRIVATE_BOOK"),
        Err(ContractError::CaseMismatch)
    );
    assert_eq!(
        i2pr_i2pcontrol::address_book::parse_set_config_key("nope"),
        Err(ContractError::UnknownLiteral)
    );
    // Path-like and inert classifications are disjoint and total on their rows.
    assert!(i2pr_i2pcontrol::address_book::is_path_like_config_key(
        "private_book"
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
