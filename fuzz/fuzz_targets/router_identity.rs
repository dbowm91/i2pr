#![no_main]

mod support;

use i2pr_proto::RouterIdentity;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    if let Some(input) = support::within(input, support::COMMON_MAX) {
        let _ = RouterIdentity::decode(input, support::COMMON_MAX);
    }
});
