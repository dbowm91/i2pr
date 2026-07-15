#![no_main]

mod support;

use i2pr_proto::Hash;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    if let Some(input) = support::within(input, 32) {
        let _ = Hash::decode(input, 32);
    }
});
