#![no_main]

mod support;

use i2pr_proto::I2npMessage;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    if let Some(input) = support::within(input, support::I2NP_MAX) {
        let _ = I2npMessage::decode_short_transport(input, support::I2NP_MAX);
    }
});
