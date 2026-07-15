#![allow(dead_code)]

pub const COMMON_MAX: usize = 1024 * 1024;
pub const I2NP_MAX: usize = 62_708 + 16;

pub fn within(input: &[u8], maximum: usize) -> Option<&[u8]> {
    (input.len() <= maximum).then_some(input)
}
