//! Tunnel data, gateway, and bounded build-record framing.

use super::*;

/// TunnelData message body with its fixed 1024-byte payload.
#[derive(Clone, Eq, PartialEq)]
pub struct TunnelDataMessage {
    /// Nonzero destination tunnel identifier.
    pub tunnel_id: u32,
    /// Encrypted and fragmented tunnel data.
    pub data: [u8; TUNNEL_DATA_PAYLOAD_SIZE],
}

impl fmt::Debug for TunnelDataMessage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TunnelDataMessage")
            .field("tunnel_id", &self.tunnel_id)
            .field("data_length", &self.data.len())
            .finish()
    }
}

/// TunnelGateway message body containing a standard nested I2NP message.
#[derive(Debug, Eq, PartialEq)]
pub struct TunnelGatewayMessage {
    /// Nonzero destination tunnel identifier.
    pub tunnel_id: u32,
    /// Nested standard-header I2NP message.
    pub message: Box<I2npMessage>,
}

/// Fixed-size or variable-size tunnel-build records retained for later crypto.
#[derive(Clone, Eq, PartialEq)]
pub struct DeferredBuildRecords {
    pub(super) count: u8,
    pub(super) record_size: u16,
    pub(super) records: Vec<u8>,
}

impl fmt::Debug for DeferredBuildRecords {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DeferredBuildRecords")
            .field("count", &self.count)
            .field("record_size", &self.record_size)
            .field("total_length", &self.records.len())
            .finish()
    }
}

impl DeferredBuildRecords {
    pub(super) fn new(count: u8, record_size: usize, records: Vec<u8>) -> Result<Self, CodecError> {
        let expected =
            usize::from(count)
                .checked_mul(record_size)
                .ok_or(CodecError::ArithmeticOverflow {
                    offset: 0,
                    context: "tunnel-build record length",
                })?;
        if count == 0 || usize::from(count) > MAX_BUILD_RECORDS || records.len() != expected {
            return Err(CodecError::InvalidFieldValue {
                offset: 0,
                context: "tunnel-build record count or length",
            });
        }
        let record_size =
            u16::try_from(record_size).map_err(|_| CodecError::InvalidFieldValue {
                offset: 0,
                context: "tunnel-build record size",
            })?;
        Ok(Self {
            count,
            record_size,
            records,
        })
    }

    /// Returns the number of records.
    pub const fn count(&self) -> u8 {
        self.count
    }

    /// Returns the fixed record size.
    pub const fn record_size(&self) -> u16 {
        self.record_size
    }

    /// Returns the retained encrypted record bytes.
    pub fn records(&self) -> &[u8] {
        &self.records
    }
}
