//! Deterministic, synchronous NTCP2 data-phase stream driving.
//!
//! This module models only the byte boundaries around the runtime-neutral
//! frame owners. It does not open sockets, wait on a clock, or run tasks.

use std::collections::VecDeque;
use std::fmt;

use i2pr_transport_ntcp2::frame::{
    FRAME_OVERHEAD, FrameError, MAX_PLAINTEXT_LENGTH, ReceiveState, TransmitState,
};

/// Maximum bytes this testkit driver may retain in its in-memory stream.
pub const MAX_NTCP2_DRIVER_BUFFERED_BYTES: usize = 1 << 20;

/// Counters for one deterministic NTCP2 data-phase driver.
///
/// `buffered_bytes` is the current total across pending writes, stream bytes,
/// and the partially read frame. `released_bytes` includes successfully
/// delivered frames and bytes discarded during disconnect cleanup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ntcp2DriverCounters {
    /// Frames accepted by the transmit owner and queued for writing.
    pub queued_frames: u64,
    /// Bytes transferred from the writer into the in-memory stream.
    pub written_bytes: u64,
    /// Bytes consumed by the reader's one-byte reads.
    pub read_bytes: u64,
    /// Frames authenticated and returned by the reader.
    pub received_frames: u64,
    /// Bytes currently retained by the driver.
    pub buffered_bytes: usize,
    /// Highest retained-byte count observed by the driver.
    pub peak_buffered_bytes: usize,
    /// Bytes released after delivery or cleanup.
    pub released_bytes: u64,
    /// Bytes discarded because the stream disconnected.
    pub discarded_bytes: u64,
    /// Whether the driver has been disconnected or failed terminally.
    pub disconnected: bool,
}

impl Ntcp2DriverCounters {
    const fn new() -> Self {
        Self {
            queued_frames: 0,
            written_bytes: 0,
            read_bytes: 0,
            received_frames: 0,
            buffered_bytes: 0,
            peak_buffered_bytes: 0,
            released_bytes: 0,
            discarded_bytes: 0,
            disconnected: false,
        }
    }
}

/// Errors from the bounded synchronous NTCP2 data-phase driver.
#[derive(Debug, Eq, PartialEq)]
pub enum Ntcp2DriverError {
    /// The requested retention bound was outside the driver's explicit range.
    InvalidBufferLimit,
    /// A complete frame would exceed the remaining retention budget.
    BufferLimit {
        /// Bytes already retained by the driver.
        buffered: usize,
        /// Bytes the caller attempted to add.
        requested: usize,
        /// Configured retention limit.
        maximum: usize,
    },
    /// The driver was disconnected or entered a terminal failure state.
    Disconnected,
    /// Disconnect occurred while a frame was being read.
    TruncatedFrame,
    /// The underlying runtime-neutral frame owner rejected an operation.
    Frame(FrameError),
    /// The bounded pump reached its caller-supplied step limit.
    StepLimit {
        /// Maximum one-byte transfers/read operations allowed by the caller.
        maximum: usize,
    },
}

impl fmt::Display for Ntcp2DriverError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBufferLimit => formatter.write_str("invalid NTCP2 driver buffer limit"),
            Self::BufferLimit { .. } => formatter.write_str("NTCP2 driver buffer limit reached"),
            Self::Disconnected => formatter.write_str("NTCP2 driver is disconnected"),
            Self::TruncatedFrame => formatter.write_str("truncated NTCP2 frame on disconnect"),
            Self::Frame(error) => error.fmt(formatter),
            Self::StepLimit { maximum } => {
                write!(formatter, "NTCP2 driver step limit {maximum} reached")
            }
        }
    }
}

impl std::error::Error for Ntcp2DriverError {}

impl From<FrameError> for Ntcp2DriverError {
    fn from(error: FrameError) -> Self {
        Self::Frame(error)
    }
}

/// A bounded synchronous stream driver connecting one transmit owner to one
/// receive owner.
pub struct Ntcp2DataPhaseDriver {
    transmit: TransmitState,
    receive: ReceiveState,
    outbound: VecDeque<u8>,
    inbound: VecDeque<u8>,
    partial: Vec<u8>,
    expected_wire_length: Option<usize>,
    maximum_buffered_bytes: usize,
    counters: Ntcp2DriverCounters,
}

impl Ntcp2DataPhaseDriver {
    /// Creates a driver with an explicit bounded total-retention limit.
    pub fn new(
        transmit: TransmitState,
        receive: ReceiveState,
        maximum_buffered_bytes: usize,
    ) -> Result<Self, Ntcp2DriverError> {
        if !(FRAME_OVERHEAD..=MAX_NTCP2_DRIVER_BUFFERED_BYTES).contains(&maximum_buffered_bytes) {
            return Err(Ntcp2DriverError::InvalidBufferLimit);
        }
        Ok(Self {
            transmit,
            receive,
            outbound: VecDeque::new(),
            inbound: VecDeque::new(),
            partial: Vec::new(),
            expected_wire_length: None,
            maximum_buffered_bytes,
            counters: Ntcp2DriverCounters::new(),
        })
    }

    /// Queues one plaintext frame for one-byte-at-a-time writing.
    ///
    /// Plaintext validation remains the receive owner's responsibility; a
    /// successful call means only that the bounded frame was sealed and
    /// retained by this driver.
    pub fn queue_plaintext(&mut self, plaintext: &[u8]) -> Result<usize, Ntcp2DriverError> {
        self.ensure_connected()?;
        let wire_length = plaintext
            .len()
            .checked_add(FRAME_OVERHEAD)
            .ok_or(FrameError::PayloadTooLarge)?;
        if plaintext.len() > MAX_PLAINTEXT_LENGTH {
            return Err(FrameError::PayloadTooLarge.into());
        }
        self.ensure_capacity(wire_length)?;
        let frame = self.transmit.seal_plaintext(plaintext)?;
        let bytes = frame.into_bytes();
        debug_assert_eq!(bytes.len(), wire_length);
        self.outbound.extend(bytes);
        self.counters.queued_frames = self.counters.queued_frames.saturating_add(1);
        self.refresh_buffered();
        Ok(wire_length)
    }

    /// Transfers at most one byte from the pending writer to the stream.
    ///
    /// Returns `true` when one byte was transferred and `false` when the
    /// writer is currently empty.
    pub fn write_one(&mut self) -> Result<bool, Ntcp2DriverError> {
        self.ensure_connected()?;
        let Some(byte) = self.outbound.pop_front() else {
            return Ok(false);
        };
        self.inbound.push_back(byte);
        self.counters.written_bytes = self.counters.written_bytes.saturating_add(1);
        self.refresh_buffered();
        Ok(true)
    }

    /// Consumes at most one stream byte and returns a completed frame, if any.
    pub fn read_one(
        &mut self,
    ) -> Result<Option<i2pr_transport_ntcp2::frame::ReceivedFrame>, Ntcp2DriverError> {
        self.ensure_connected()?;
        if self.receive.is_terminated() {
            return Err(FrameError::StateViolation.into());
        }
        let Some(byte) = self.inbound.pop_front() else {
            return Ok(None);
        };
        self.partial.push(byte);
        self.counters.read_bytes = self.counters.read_bytes.saturating_add(1);

        if self.partial.len() == 2 {
            let prefix = [self.partial[0], self.partial[1]];
            let length = match self.receive.decode_length(prefix) {
                Ok(length) => length,
                Err(error) => return Err(self.fail_frame(error)),
            };
            self.expected_wire_length = Some(2 + length.ciphertext_length);
        }

        let Some(expected) = self.expected_wire_length else {
            self.refresh_buffered();
            return Ok(None);
        };
        if self.partial.len() < expected {
            self.refresh_buffered();
            return Ok(None);
        }

        debug_assert_eq!(self.partial.len(), expected);
        let result = self.receive.open_ciphertext(&self.partial[2..]);
        match result {
            Ok(frame) => {
                self.expected_wire_length = None;
                self.partial.clear();
                self.counters.received_frames = self.counters.received_frames.saturating_add(1);
                self.release_bytes(expected);
                Ok(Some(frame))
            }
            Err(error) => Err(self.fail_frame(error)),
        }
    }

    /// Transfers one byte and then reads one byte, preserving stream ordering.
    pub fn pump_one(
        &mut self,
    ) -> Result<Option<i2pr_transport_ntcp2::frame::ReceivedFrame>, Ntcp2DriverError> {
        let _ = self.write_one()?;
        self.read_one()
    }

    /// Pumps at most `maximum_steps` one-byte operations until the stream is idle.
    ///
    /// Returns the number of completed frames. A step includes one attempted
    /// write/read pair, so callers retain an explicit bound even for malformed
    /// or truncated input scenarios.
    pub fn pump_until_idle(&mut self, maximum_steps: usize) -> Result<usize, Ntcp2DriverError> {
        if maximum_steps == 0 {
            return Err(Ntcp2DriverError::StepLimit {
                maximum: maximum_steps,
            });
        }
        let mut received = 0;
        for _ in 0..maximum_steps {
            if self.outbound.is_empty() && self.inbound.is_empty() && self.partial.is_empty() {
                return Ok(received);
            }
            if self.pump_one()?.is_some() {
                received += 1;
            }
        }
        Err(Ntcp2DriverError::StepLimit {
            maximum: maximum_steps,
        })
    }

    /// Disconnects and releases every retained byte.
    ///
    /// A partial frame produces `TruncatedFrame`, but cleanup is performed
    /// before the result is returned. The counters remain available through
    /// [`Self::counters`] after either result.
    pub fn disconnect(&mut self) -> Result<(), Ntcp2DriverError> {
        if self.counters.disconnected {
            return Ok(());
        }
        let truncated = !self.inbound.is_empty()
            || !self.partial.is_empty()
            || self.expected_wire_length.is_some();
        let retained = self.retained_bytes();
        self.outbound.clear();
        self.inbound.clear();
        self.partial.clear();
        self.expected_wire_length = None;
        self.counters.disconnected = true;
        self.counters.discarded_bytes = self
            .counters
            .discarded_bytes
            .saturating_add(retained as u64);
        self.release_bytes(retained);
        if truncated {
            Err(Ntcp2DriverError::TruncatedFrame)
        } else {
            Ok(())
        }
    }

    /// Returns a copy of the bounded counters, including cleanup results.
    pub const fn counters(&self) -> Ntcp2DriverCounters {
        self.counters
    }

    fn ensure_connected(&self) -> Result<(), Ntcp2DriverError> {
        if self.counters.disconnected {
            Err(Ntcp2DriverError::Disconnected)
        } else {
            Ok(())
        }
    }

    fn ensure_capacity(&self, requested: usize) -> Result<(), Ntcp2DriverError> {
        let buffered = self.retained_bytes();
        if buffered.saturating_add(requested) > self.maximum_buffered_bytes {
            return Err(Ntcp2DriverError::BufferLimit {
                buffered,
                requested,
                maximum: self.maximum_buffered_bytes,
            });
        }
        Ok(())
    }

    fn retained_bytes(&self) -> usize {
        self.outbound
            .len()
            .saturating_add(self.inbound.len())
            .saturating_add(self.partial.len())
    }

    fn refresh_buffered(&mut self) {
        let buffered = self.retained_bytes();
        self.counters.buffered_bytes = buffered;
        self.counters.peak_buffered_bytes = self.counters.peak_buffered_bytes.max(buffered);
    }

    fn release_bytes(&mut self, released: usize) {
        self.counters.released_bytes = self.counters.released_bytes.saturating_add(released as u64);
        self.refresh_buffered();
    }

    fn fail_frame(&mut self, error: FrameError) -> Ntcp2DriverError {
        let retained = self.retained_bytes();
        self.outbound.clear();
        self.inbound.clear();
        self.partial.clear();
        self.expected_wire_length = None;
        self.counters.disconnected = true;
        self.counters.discarded_bytes = self
            .counters
            .discarded_bytes
            .saturating_add(retained as u64);
        self.release_bytes(retained);
        Ntcp2DriverError::Frame(error)
    }
}

impl fmt::Debug for Ntcp2DataPhaseDriver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Ntcp2DataPhaseDriver")
            .field("counters", &self.counters)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use i2pr_transport_ntcp2::block::{Block, TimestampBlock, encode_blocks};
    use i2pr_transport_ntcp2::crypto::{CipherState, SipHashState};

    fn driver(maximum: usize) -> Ntcp2DataPhaseDriver {
        Ntcp2DataPhaseDriver::new(
            TransmitState::new(
                CipherState::from_key_for_test([0x11; 32]),
                SipHashState::from_material_for_test([0x22; 32]),
            ),
            ReceiveState::new(
                CipherState::from_key_for_test([0x11; 32]),
                SipHashState::from_material_for_test([0x22; 32]),
            ),
            maximum,
        )
        .expect("driver")
    }

    fn timestamp(seconds: u32) -> Vec<u8> {
        encode_blocks(vec![Block::Timestamp(TimestampBlock::new(seconds))]).expect("timestamp")
    }

    #[test]
    fn one_byte_pump_handles_a_frame_and_releases_buffers() {
        let plaintext = timestamp(7);
        let mut driver = driver(128);
        let frame_length = driver.queue_plaintext(&plaintext).expect("queue");

        let received = driver.pump_until_idle(128).expect("pump");
        assert_eq!(received, 1);
        assert_eq!(driver.counters().received_frames, 1);
        assert_eq!(driver.counters().written_bytes as usize, frame_length);
        assert_eq!(driver.counters().read_bytes as usize, frame_length);
        assert_eq!(driver.counters().buffered_bytes, 0);
        assert_eq!(driver.counters().released_bytes as usize, frame_length);
    }

    #[test]
    fn multiple_frames_share_one_stream_and_peak_is_bounded() {
        let first = timestamp(1);
        let second = timestamp(2);
        let wire_length = first.len() + FRAME_OVERHEAD;
        let second_wire_length = second.len() + FRAME_OVERHEAD;
        let mut driver = driver(wire_length + second_wire_length);
        driver.queue_plaintext(&first).expect("first queue");
        driver.queue_plaintext(&second).expect("second queue");

        assert_eq!(driver.pump_until_idle(256).expect("pump"), 2);
        assert_eq!(driver.counters().queued_frames, 2);
        assert_eq!(driver.counters().received_frames, 2);
        assert_eq!(driver.counters().buffered_bytes, 0);
        assert!(driver.counters().peak_buffered_bytes <= wire_length + second_wire_length);
    }

    #[test]
    fn disconnect_cleans_a_partial_frame_and_exposes_discard_count() {
        let mut driver = driver(128);
        let wire_length = driver.queue_plaintext(&timestamp(3)).expect("queue");
        assert!(driver.write_one().expect("write"));
        assert!(driver.read_one().expect("read").is_none());

        assert_eq!(driver.disconnect(), Err(Ntcp2DriverError::TruncatedFrame));
        let counters = driver.counters();
        assert!(counters.disconnected);
        assert_eq!(counters.buffered_bytes, 0);
        assert_eq!(counters.discarded_bytes, wire_length as u64);
        assert_eq!(counters.released_bytes, wire_length as u64);
    }

    #[test]
    fn queue_rejects_a_frame_that_would_exceed_the_byte_bound() {
        let mut driver = driver(FRAME_OVERHEAD);
        assert!(matches!(
            driver.queue_plaintext(&timestamp(4)),
            Err(Ntcp2DriverError::BufferLimit { .. })
        ));
        assert_eq!(driver.counters().queued_frames, 0);
    }
}
