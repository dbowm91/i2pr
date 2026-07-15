#![no_main]

mod support;

use i2pr_proto::Date32;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    if let Some(input) = support::within(input, 4) {
        let _ = Date32::decode(input, 4);
    }
});
