use i2pr_crypto::{OsRng, RouterIdentityBundle, X25519PrivateKey};
use i2pr_proto::{Date, Mapping, RouterAddress};
use i2pr_transport_ntcp2::constants::MIN_HANDSHAKE_MESSAGE_LENGTH;
use i2pr_transport_ntcp2::crypto::{PublicKeyBytes, Role};
use i2pr_transport_ntcp2::handshake::{
    ClockSkewPolicy, HandshakeError, ReplayDecision, SessionConfirmed, validate_router_info,
};
use i2pr_transport_ntcp2::state_machine::{
    AuthenticatedHandshake, HandshakeAction, HandshakeInput, InitiatorState, PaddingMessage,
    ResponderState,
};

fn i2p_base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-~";
    let mut output = String::new();
    let mut offset = 0;
    while offset < bytes.len() {
        let remaining = bytes.len() - offset;
        let a = bytes[offset];
        let b = if remaining > 1 { bytes[offset + 1] } else { 0 };
        let c = if remaining > 2 { bytes[offset + 2] } else { 0 };
        output.push(ALPHABET[(a >> 2) as usize] as char);
        output.push(ALPHABET[((a & 3) << 4 | b >> 4) as usize] as char);
        output.push(if remaining > 1 {
            ALPHABET[((b & 15) << 2 | c >> 6) as usize] as char
        } else {
            '='
        });
        output.push(if remaining > 2 {
            ALPHABET[(c & 63) as usize] as char
        } else {
            '='
        });
        offset += 3;
    }
    output
}

fn router_info(bundle: &RouterIdentityBundle, transport_static: [u8; 32]) -> Vec<u8> {
    let mut options = Mapping::builder();
    options
        .insert("s".to_owned(), i2p_base64(&transport_static))
        .expect("static key option");
    options
        .insert("v".to_owned(), "2".to_owned())
        .expect("version option");
    let address = RouterAddress::new(
        1,
        Date::from_millis(1),
        "NTCP2".to_owned(),
        options.build().expect("address options"),
    )
    .expect("router address");
    bundle
        .sign_router_info(
            Date::from_millis(1_000),
            vec![address],
            Vec::new(),
            Mapping::empty(),
        )
        .expect("signed RouterInfo")
        .encode_to_vec(i2pr_proto::MAX_COMMON_STRUCTURE_SIZE)
        .expect("RouterInfo bytes")
}

fn write_bytes(actions: Vec<HandshakeAction>) -> Vec<u8> {
    actions
        .into_iter()
        .find_map(|action| match action {
            HandshakeAction::Write(bytes) => Some(bytes.into_bytes()),
            _ => None,
        })
        .expect("write action")
}

fn write_and_authenticated(actions: Vec<HandshakeAction>) -> (Vec<u8>, AuthenticatedHandshake) {
    let mut write = None;
    let mut authenticated = None;
    for action in actions {
        match action {
            HandshakeAction::Write(bytes) => write = Some(bytes.into_bytes()),
            HandshakeAction::Authenticated(result) => authenticated = Some(result),
            _ => {}
        }
    }
    (
        write.expect("write action"),
        authenticated.expect("authenticated action"),
    )
}

fn authenticated(actions: Vec<HandshakeAction>) -> AuthenticatedHandshake {
    actions
        .into_iter()
        .find_map(|action| match action {
            HandshakeAction::Authenticated(result) => Some(result),
            _ => None,
        })
        .expect("authenticated action")
}

#[test]
fn deterministic_initiator_and_responder_complete_with_matching_data_keys() {
    let alice_identity = RouterIdentityBundle::from_private_bytes([1; 32], [2; 32], &mut OsRng)
        .expect("Alice identity");
    let bob_identity = RouterIdentityBundle::from_private_bytes([3; 32], [4; 32], &mut OsRng)
        .expect("Bob identity");
    let alice_static = X25519PrivateKey::from_bytes([0x24; 32]);
    let bob_static = X25519PrivateKey::from_bytes([0x42; 32]);
    let alice_ephemeral = X25519PrivateKey::from_bytes([0x13; 32]);
    let bob_ephemeral = X25519PrivateKey::from_bytes([0x31; 32]);
    let alice_info = router_info(&alice_identity, alice_static.public_bytes());
    let bob_hash = bob_identity.identity().hash().expect("Bob hash");
    let alice_hash = alice_identity.identity().hash().expect("Alice hash");
    let bob_public = PublicKeyBytes::new(bob_static.public_bytes()).expect("Bob public");
    let skew = ClockSkewPolicy::default_compatibility();

    let initiator = InitiatorState::new(
        alice_static,
        alice_ephemeral,
        bob_public,
        Some(bob_hash),
        *bob_hash.as_bytes(),
        [0x55; 16],
        2,
        skew,
    )
    .expect("initiator");
    let step = initiator.start().expect("initiator start");
    assert!(matches!(
        step.actions.first(),
        Some(HandshakeAction::RequestRouterInfo { .. })
    ));
    let step = step
        .state
        .transition(HandshakeInput::RouterInfo(alice_info))
        .expect("local RouterInfo");
    assert!(matches!(
        step.actions.first(),
        Some(HandshakeAction::RequestTimestamp { .. })
    ));
    let step = step
        .state
        .transition(HandshakeInput::Timestamp(1_000))
        .expect("request timestamp");
    assert!(matches!(
        step.actions.first(),
        Some(HandshakeAction::RequestPadding {
            message: PaddingMessage::SessionRequest,
            ..
        })
    ));
    let step = step
        .state
        .transition(HandshakeInput::Padding(vec![0xaa; 3]))
        .expect("request padding");
    let step = step
        .state
        .transition(HandshakeInput::Padding(vec![0xbb; 5]))
        .expect("confirmed padding");
    let request = write_bytes(step.actions);
    let initiator_after_request = step.state;

    let responder = ResponderState::new(
        bob_static,
        bob_ephemeral,
        Some(alice_hash),
        *bob_hash.as_bytes(),
        [0x55; 16],
        2,
        skew,
    )
    .expect("responder");
    let step = responder.start().expect("responder start");
    let step = step
        .state
        .transition(HandshakeInput::Bytes(
            request[..MIN_HANDSHAKE_MESSAGE_LENGTH].to_vec(),
        ))
        .expect("request read");
    let step = step
        .state
        .transition(HandshakeInput::Replay(ReplayDecision::Fresh))
        .expect("request replay");
    let step = step
        .state
        .transition(HandshakeInput::Bytes(
            request[MIN_HANDSHAKE_MESSAGE_LENGTH..].to_vec(),
        ))
        .expect("request padding read");
    let step = step
        .state
        .transition(HandshakeInput::Timestamp(1_000))
        .expect("peer timestamp");
    let step = step
        .state
        .transition(HandshakeInput::Padding(vec![0xcc; 7]))
        .expect("created padding");
    let created = write_bytes(step.actions);
    let responder_after_created = step.state;

    let step = initiator_after_request
        .transition(HandshakeInput::Bytes(
            created[..MIN_HANDSHAKE_MESSAGE_LENGTH].to_vec(),
        ))
        .expect("created read");
    let step = step
        .state
        .transition(HandshakeInput::Bytes(
            created[MIN_HANDSHAKE_MESSAGE_LENGTH..].to_vec(),
        ))
        .expect("created padding read");
    let step = step
        .state
        .transition(HandshakeInput::Timestamp(1_000))
        .expect("created timestamp");
    let step = step
        .state
        .transition(HandshakeInput::Replay(ReplayDecision::Fresh))
        .expect("created replay");
    let (confirmed, initiator_result) = write_and_authenticated(step.actions);
    let responder_result = authenticated(
        responder_after_created
            .transition(HandshakeInput::Bytes(confirmed))
            .expect("confirmed read")
            .actions,
    );

    assert_eq!(initiator_result.role(), Role::Initiator);
    assert_eq!(responder_result.role(), Role::Responder);
    assert_eq!(initiator_result.peer().router_hash, bob_hash);
    assert_eq!(responder_result.peer().router_hash, alice_hash);

    let mut initiator_keys = initiator_result;
    let mut responder_keys = responder_result;
    let frame = initiator_keys
        .split_keys()
        .transmit()
        .seal(b"deterministic-data", &[])
        .expect("seal data");
    assert_eq!(
        responder_keys
            .split_keys()
            .receive()
            .open(&frame, &[])
            .expect("open data"),
        b"deterministic-data"
    );
}

#[test]
fn session_confirmed_decode_is_exact_at_every_partial_boundary() {
    let message = SessionConfirmed::new(vec![0; 48], vec![0; 16]).expect("message");
    let encoded = message.encode();
    for split in 0..encoded.len() {
        assert_eq!(
            SessionConfirmed::decode(&encoded[..split], 16, 65_535),
            Err(HandshakeError::Truncated),
            "split {split}"
        );
    }
    assert!(SessionConfirmed::decode(&encoded, 16, 65_535).is_ok());
    let mut extra = encoded;
    extra.push(0);
    assert_eq!(
        SessionConfirmed::decode(&extra, 16, 65_535),
        Err(HandshakeError::InvalidFixedLength)
    );
}

#[test]
fn cancellation_deadline_and_disconnect_are_terminal_actions() {
    for (input, expected) in [
        (HandshakeInput::Cancelled, HandshakeError::Cancelled),
        (
            HandshakeInput::DeadlineExpired,
            HandshakeError::DeadlineExpired,
        ),
        (HandshakeInput::Disconnected, HandshakeError::Disconnected),
    ] {
        let key = X25519PrivateKey::from_bytes([0x24; 32]);
        let peer = PublicKeyBytes::new(X25519PrivateKey::from_bytes([0x42; 32]).public_bytes())
            .expect("peer");
        let state = InitiatorState::new(
            key,
            X25519PrivateKey::from_bytes([0x13; 32]),
            peer,
            None,
            [0x55; 32],
            [0x55; 16],
            2,
            ClockSkewPolicy::default_compatibility(),
        )
        .expect("state");
        let action = state
            .transition(input)
            .expect("terminal transition")
            .actions
            .into_iter()
            .next()
            .expect("terminate action");
        assert!(matches!(action, HandshakeAction::Terminate(error) if error == expected));
    }
}

#[test]
fn router_info_signature_and_transport_key_binding_fail_closed() {
    let identity =
        RouterIdentityBundle::from_private_bytes([7; 32], [8; 32], &mut OsRng).expect("identity");
    let transport_static = X25519PrivateKey::from_bytes([0x61; 32]);
    let bytes = router_info(&identity, transport_static.public_bytes());
    let hash = identity.identity().hash().expect("identity hash");
    let expected = PublicKeyBytes::new(transport_static.public_bytes()).expect("static");
    let peer = validate_router_info(
        &bytes,
        i2pr_transport_ntcp2::constants::MAX_ROUTER_INFO_PAYLOAD,
        Some(hash),
        expected,
    )
    .expect("valid binding");
    assert_eq!(peer.router_hash, hash);

    let wrong_static = PublicKeyBytes::new(X25519PrivateKey::from_bytes([0x62; 32]).public_bytes())
        .expect("wrong static");
    assert_eq!(
        validate_router_info(
            &bytes,
            i2pr_transport_ntcp2::constants::MAX_ROUTER_INFO_PAYLOAD,
            Some(hash),
            wrong_static,
        ),
        Err(HandshakeError::TransportStaticKeyMismatch)
    );

    let mut mutated = bytes;
    *mutated.last_mut().expect("signature byte") ^= 1;
    assert_eq!(
        validate_router_info(
            &mutated,
            i2pr_transport_ntcp2::constants::MAX_ROUTER_INFO_PAYLOAD,
            Some(hash),
            expected,
        ),
        Err(HandshakeError::RouterInfoSignatureInvalid)
    );
}
