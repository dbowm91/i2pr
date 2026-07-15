//! I2NP message identifiers and standard/short header variants.

pub use crate::i2np_impl::{
    I2npHeader, MAX_I2NP_PAYLOAD_SIZE, MessageType, SHORT_SSU_HEADER_SIZE,
    SHORT_TRANSPORT_HEADER_SIZE, STANDARD_HEADER_SIZE,
};
