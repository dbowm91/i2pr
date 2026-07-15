#![no_main]

use i2pr_storage::{decode_transport_static_key, MAX_NTCP2_TRANSPORT_KEY_FILE_SIZE};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    if input.len() <= MAX_NTCP2_TRANSPORT_KEY_FILE_SIZE {
        let _ = decode_transport_static_key(input);
    }
});
