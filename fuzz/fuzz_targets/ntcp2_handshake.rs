#![no_main]

use i2pr_transport_ntcp2::handshake::{
    validate_router_info, ClockSkewPolicy, ConfirmedPayload,
    ReferenceReplayCache, ReplayToken, SessionConfirmed, SessionCreated, SessionRequest,
};
use i2pr_crypto::X25519PrivateKey;
use i2pr_transport_ntcp2::crypto::PublicKeyBytes;
use i2pr_transport_ntcp2::state_machine::{HandshakeInput, InitiatorState};
use libfuzzer_sys::fuzz_target;

const MAX_INPUT: usize = 65_535;

fuzz_target!(|input: &[u8]| {
    if input.len() > MAX_INPUT {
        return;
    }
    let _ = SessionRequest::decode(input, MAX_INPUT);
    let _ = SessionCreated::decode(input, MAX_INPUT);

    if input.len() >= 2 {
        let expected = usize::from(u16::from_be_bytes([input[0], input[1]]));
        let _ = SessionConfirmed::decode(input, expected, MAX_INPUT);
    }

    let _ = ConfirmedPayload::decode(input, 64 * 1024);

    let _ = validate_router_info(
        input,
        64 * 1024,
        None,
        PublicKeyBytes::from_bytes_for_test([1; 32]),
    );
    let policy = ClockSkewPolicy::default_compatibility();
    let local_bytes: [u8; 8] = input.get(..8).unwrap_or(&[0; 8]).try_into().unwrap();
    let _ = policy.classify(
        u64::from_le_bytes(local_bytes),
        input.get(8).copied().unwrap_or_default() as u32,
    );
    let mut cache = ReferenceReplayCache::new(1, policy.replay_retention()).expect("fixed cache");
    let token = ReplayToken::from_ephemeral_bytes(input);
    let _ = cache.check_and_record(token, 0);

    // Exercise bounded command sequencing with no valid RouterInfo source.
    // Any error ends the sequence; a failed transition cannot be resumed.
    let peer = PublicKeyBytes::from_bytes_for_test(
        X25519PrivateKey::from_bytes([2; 32]).public_bytes(),
    );
    let mut state = Some(
        InitiatorState::new(
            X25519PrivateKey::from_bytes([3; 32]),
            X25519PrivateKey::from_bytes([4; 32]),
            peer,
            None,
            [5; 32],
            [6; 16],
            2,
            policy,
        )
        .expect("fixed state inputs"),
    );
    for command in input.chunks(3).take(32) {
        let Some(current) = state.take() else {
            break;
        };
        let command = match command.first().copied().unwrap_or_default() % 8 {
            0 => HandshakeInput::Bytes(command.to_vec()),
            1 => HandshakeInput::Timestamp(u64::from(command.first().copied().unwrap_or_default())),
            2 => HandshakeInput::Replay(i2pr_transport_ntcp2::handshake::ReplayDecision::Fresh),
            3 => HandshakeInput::Padding(command.to_vec()),
            4 => HandshakeInput::RouterInfo(command.to_vec()),
            5 => HandshakeInput::Cancelled,
            6 => HandshakeInput::Disconnected,
            _ => HandshakeInput::DeadlineExpired,
        };
        match current.transition(command) {
            Ok(next) => {
                for action in &next.actions {
                    assert!(!matches!(action, i2pr_transport_ntcp2::state_machine::HandshakeAction::Authenticated(_)),
                        "random command sequence authenticated without required checks");
                }
                state = Some(next.state);
            }
            Err(_) => break,
        }
    }
});
