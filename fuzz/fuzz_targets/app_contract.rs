#![no_main]

use i2pr_app_proto::{
    AppId, AppInstanceId, Frame, Handshake, Manifest, Role, MAX_CONTROL_BYTES,
    MAX_MANIFEST_BYTES,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    let _ = Frame::decode(input);
    let _ = Handshake::decode(input);
    if input.len() <= MAX_CONTROL_BYTES {
        let app = Handshake {
            role: Role::Application,
            major: 1,
            minor: 0,
        };
        let _ = app.decode_app_message(input);
        let admin = Handshake {
            role: Role::Administrator,
            major: 1,
            minor: 0,
        };
        let _ = admin.decode_admin_message(input);
    }
    if input.len() <= MAX_MANIFEST_BYTES {
        let _ = Manifest::decode(input);
    }
    if input.len() <= 128 {
        if let Ok(value) = std::str::from_utf8(input) {
            let _ = AppId::parse(value);
        }
    }
    let _ = AppInstanceId::new(u128::from_le_bytes(
        input.get(..16).and_then(|bytes| bytes.try_into().ok()).unwrap_or([0; 16]),
    ));
});
