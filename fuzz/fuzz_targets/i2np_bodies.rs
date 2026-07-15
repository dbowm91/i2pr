#![no_main]

mod support;

use i2pr_proto::{Hash, I2npMessage, MAX_I2NP_PAYLOAD_SIZE};
use libfuzzer_sys::fuzz_target;

// One dispatch target keeps the corpus small while ensuring every complex
// body parser receives independent arbitrary input. The selected type is not
// a support claim; successful parsing still requires the body's own framing.
const BODY_TYPES: [u8; 14] = [1, 2, 3, 10, 11, 18, 19, 20, 21, 22, 23, 24, 25, 26];

fuzz_target!(|input: &[u8]| {
    if input.is_empty() {
        return;
    }
    let body = &input[1..input.len().min(MAX_I2NP_PAYLOAD_SIZE + 1)];
    let message_type = BODY_TYPES[usize::from(input[0]) % BODY_TYPES.len()];
    let Ok(body_length) = u16::try_from(body.len()) else {
        return;
    };
    let mut frame = Vec::with_capacity(16 + body.len());
    frame.push(message_type);
    frame.extend_from_slice(&[0; 4]);
    frame.extend_from_slice(&[0; 8]);
    frame.extend_from_slice(&body_length.to_be_bytes());
    frame.push(Hash::digest(body).as_bytes()[0]);
    frame.extend_from_slice(body);
    let _ = I2npMessage::decode_standard(&frame, frame.len());
});
