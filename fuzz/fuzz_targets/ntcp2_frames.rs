#![no_main]

use i2pr_transport_ntcp2::crypto::{CipherState, SipHashState};
use i2pr_transport_ntcp2::frame::{ReceiveState, TransmitState};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    let mut receive = ReceiveState::new(
        CipherState::from_key_for_test([0x11; 32]),
        SipHashState::from_material_for_test([0x22; 32]),
    );
    let _ = receive.open_wire_frame(input);
    let mut transmit = TransmitState::new(
        CipherState::from_key_for_test([0x11; 32]),
        SipHashState::from_material_for_test([0x22; 32]),
    );
    let bounded = &input[..input.len().min(65_519)];
    let _ = transmit.seal_plaintext(bounded);
});
