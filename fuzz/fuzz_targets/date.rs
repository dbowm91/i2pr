#![no_main]

mod support;

use i2pr_proto::Date;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    if let Some(input) = support::within(input, 8) {
        let _ = Date::decode(input, 8);
    }
});
