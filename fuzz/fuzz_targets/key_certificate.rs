#![no_main]

mod support;

use i2pr_proto::Certificate;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    // KeyCertificate's public decode path is Certificate::decode(type 5).
    if input.len() > support::COMMON_MAX.saturating_sub(3) {
        return;
    }
    let length = u16::try_from(input.len());
    let Ok(length) = length else { return };
    let mut certificate = Vec::with_capacity(input.len() + 3);
    certificate.push(5);
    certificate.extend_from_slice(&length.to_be_bytes());
    certificate.extend_from_slice(input);
    let _ = Certificate::decode(&certificate, support::COMMON_MAX);
});
