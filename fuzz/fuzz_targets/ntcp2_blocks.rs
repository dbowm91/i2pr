#![no_main]

use i2pr_transport_ntcp2::block::parse_blocks;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    let bounded = &input[..input.len().min(65_519)];
    let _ = parse_blocks(bounded);
});
