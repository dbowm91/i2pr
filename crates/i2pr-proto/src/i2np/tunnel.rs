//! Tunnel data, gateway, and bounded build-record framing.

pub use crate::i2np_impl::{
    DeferredBuildRecords, MAX_BUILD_RECORDS, SHORT_BUILD_RECORD_SIZE, TUNNEL_DATA_PAYLOAD_SIZE,
    TunnelDataMessage, TunnelGatewayMessage, VARIABLE_BUILD_RECORD_SIZE,
};
