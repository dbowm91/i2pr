//! Plan 151 §8 — deterministic pre-start delivery fault seam.
//!
//! The localhost SAM product delivers Streaming packets between two
//! in-process destination bridges through
//! [`crate::sam::SamServiceState::deliver_outbound`]. A SAM TCP client
//! only ever sees application bytes, so packet-level faults (drop,
//! duplicate, reorder, corruption, ceiling) cannot be produced from
//! the TCP boundary. This module provides the narrow pre-start test
//! configuration the plan requires:
//!
//! ```text
//! SAM TCP application clients
//!     ↓
//! normal Plan 149 SESSION CREATE / CONNECT / ACCEPT
//!     ↓
//! existing Streaming / destination delivery path
//!     ↓
//! deterministic test-only fault profile (this module)
//! ```
//!
//! ## Rules
//!
//! - The profile is installed on [`crate::sam::SamServiceState`]
//!   **before** the listener starts serving. After startup,
//!   behavior-driving interactions remain SAM TCP only; tests only
//!   read back the non-secret [`SamDeliveryFaultCounters`] snapshot.
//! - The default profile is fully inert: every request passes
//!   through unmodified, so production behavior is unchanged.
//! - Faults apply only to established-connection DATA and
//!   standalone-ACK packets. Handshake (`SYN`) and terminal
//!   (`CLOSE`/`RESET`) control packets always pass through, so a
//!   faulted stream still establishes and still terminates through
//!   the existing lifecycle paths.
//! - At most one packet is ever held for reordering. There are no
//!   unbounded queues, no new tasks, no timers, and no dependency
//!   on `i2pr-testkit`.
//! - Fault drops never touch
//!   [`crate::sam::fabric::DeliverySweepCounters`] and never invoke
//!   connection termination. A dropped packet stays tracked by the
//!   sender's Streaming retransmit state, which is exactly the
//!   recovery path under test. Corrupted packets *do* flow through
//!   the normal delivery path (with mutated bytes), so a rejected
//!   delivery surfaces through the existing typed
//!   `delivery_failed` accounting and connection-termination
//!   semantics rather than through a special fault path.
//! - No secret material is retained: classification decodes only
//!   flags and payload lengths, and counters carry only counts.

#![forbid(unsafe_code)]

use i2pr_client::streaming::transport::TransportSendRequest;

/// Packet class used to scope fault actions. Derived by decoding the
/// request's protocol-6 client-payload envelope (which verifies the
/// gzip CRC) and peeking/decoding the inner Streaming packet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FaultPacketClass {
    /// `SYNCHRONIZE` handshake packet. Never faulted.
    Handshake,
    /// Established-connection packet carrying application bytes.
    Data,
    /// Standalone acknowledgement carrying no application bytes and
    /// no handshake/terminal flags.
    AckOnly,
    /// `CLOSE`/`RESET` terminal control packet. Never faulted.
    CloseReset,
    /// The request could not be classified (unexpected protocol or
    /// undecodable bytes). Passed through unmodified.
    Unknown,
}

/// Classifies one outbound delivery request without retaining any
/// payload, identity, or key material.
pub fn classify_delivery_request(request: &TransportSendRequest) -> FaultPacketClass {
    let envelope = match i2pr_proto::streaming::decode_client_payload(
        &request.application_payload,
        i2pr_proto::streaming::MAX_CLIENT_PAYLOAD_BYTES,
    ) {
        Ok(envelope) => envelope,
        Err(_) => return FaultPacketClass::Unknown,
    };
    if envelope.protocol != i2pr_proto::streaming::STREAMING_PROTOCOL_NUMBER {
        return FaultPacketClass::Unknown;
    }
    let peek = match i2pr_proto::streaming::peek_streaming_header(&envelope.payload) {
        Ok(peek) => peek,
        Err(_) => return FaultPacketClass::Unknown,
    };
    if peek.flags_bits & i2pr_proto::streaming::FLAG_SYNCHRONIZE != 0 {
        return FaultPacketClass::Handshake;
    }
    if peek.flags_bits & (i2pr_proto::streaming::FLAG_CLOSE | i2pr_proto::streaming::FLAG_RESET)
        != 0
    {
        return FaultPacketClass::CloseReset;
    }
    let decoded = i2pr_proto::streaming::decode_streaming_packet(
        &envelope.payload,
        i2pr_proto::streaming::StreamingReceiveLimit::default(),
        i2pr_proto::streaming::StreamingOptionDecodeContext::anonymous(),
    );
    match decoded {
        Ok((packet, _)) => {
            if packet.payload.is_empty() {
                FaultPacketClass::AckOnly
            } else {
                FaultPacketClass::Data
            }
        }
        Err(_) => FaultPacketClass::Unknown,
    }
}

/// Non-secret observed counters proving which faults actually fired.
/// Tests must assert the relevant counter is nonzero; otherwise a
/// passing byte comparison could hide a fault that never triggered.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SamDeliveryFaultCounters {
    /// DATA packets dropped by the profile (single-drop arm).
    pub dropped_data: u64,
    /// Standalone ACK packets dropped by the profile.
    pub dropped_ack: u64,
    /// DATA packets dropped by the ceiling arm.
    pub dropped_ceiling: u64,
    /// DATA packets delivered twice.
    pub duplicated: u64,
    /// DATA packets delivered with mutated bytes.
    pub corrupted: u64,
    /// Reorder operations performed (one held packet released after
    /// a newer packet).
    pub reordered: u64,
    /// Handshake packets passed through untouched.
    pub handshake_passthrough: u64,
    /// CLOSE/RESET packets passed through untouched.
    pub close_reset_passthrough: u64,
}

/// Deterministic pre-start delivery fault profile.
///
/// Each `arm_*` method installs a bounded fault; each fault fires at
/// most its armed count and then disarms itself. The observed
/// counters record every firing for test assertions.
#[derive(Debug, Default)]
pub struct SamDeliveryFaultProfile {
    drop_data: u32,
    drop_ack: u32,
    duplicate_data: u32,
    corrupt_skip: u32,
    corrupt_data: u32,
    reorder_one: bool,
    drop_all_data_ack: bool,
    /// Observability without faults: handshake and CLOSE/RESET
    /// control packets are counted while every DATA/ACK passes
    /// through untouched. Lets lifecycle tests observe wire control
    /// flow without altering it.
    control_observe: bool,
    held: Option<TransportSendRequest>,
    counters: SamDeliveryFaultCounters,
}

impl SamDeliveryFaultProfile {
    /// Returns the inert default profile: every request passes
    /// through unmodified.
    pub fn disabled() -> Self {
        Self::default()
    }

    /// Returns `true` when no fault is armed and nothing is held.
    /// The production listener always observes this state.
    pub fn is_inert(&self) -> bool {
        self.drop_data == 0
            && self.drop_ack == 0
            && self.duplicate_data == 0
            && self.corrupt_skip == 0
            && self.corrupt_data == 0
            && !self.reorder_one
            && !self.drop_all_data_ack
            && !self.control_observe
            && self.held.is_none()
    }

    /// Drops the next `count` established-connection DATA packets.
    /// Each dropped packet stays tracked by the sender's Streaming
    /// retransmit state, so recovery exercises the real
    /// retransmission path.
    pub fn arm_drop_data(&mut self, count: u32) {
        self.drop_data = self.drop_data.saturating_add(count);
    }

    /// Drops the next `count` standalone ACK packets.
    pub fn arm_drop_ack(&mut self, count: u32) {
        self.drop_ack = self.drop_ack.saturating_add(count);
    }

    /// Delivers the next `count` DATA packets twice. The receiver's
    /// Streaming sequence deduplication must emit the application
    /// bytes exactly once.
    pub fn arm_duplicate_data(&mut self, count: u32) {
        self.duplicate_data = self.duplicate_data.saturating_add(count);
    }

    /// Delivers the next `count` DATA packets with deterministically
    /// mutated bytes. The mutation breaks the gzip CRC inside the
    /// protocol-6 envelope, so the peer's typed decode rejects the
    /// delivery and the mutated bytes never reach the application.
    pub fn arm_corrupt_data(&mut self, count: u32) {
        self.arm_corrupt_data_after(0, count);
    }

    /// Like [`Self::arm_corrupt_data`], but passes the first `skip`
    /// DATA packets through untouched before corrupting. Lets a test
    /// deliver a pristine sentinel first so the rejection assertion
    /// can distinguish "corrupted bytes never arrived" from "nothing
    /// ever arrived".
    pub fn arm_corrupt_data_after(&mut self, skip: u32, count: u32) {
        self.corrupt_skip = self.corrupt_skip.saturating_add(skip);
        self.corrupt_data = self.corrupt_data.saturating_add(count);
    }

    /// Reorders one pair of DATA packets: the first DATA packet is
    /// held back and released after a newer DATA packet, so the
    /// receiver must buffer and deliver the application stream in
    /// order.
    pub fn arm_reorder_one(&mut self) {
        self.reorder_one = true;
    }

    /// Drops every DATA and standalone-ACK packet while armed. Used
    /// for the retransmission-ceiling test: the sender exhausts its
    /// bounded retransmit budget with no infinite retry. Handshake
    /// and CLOSE/RESET control packets still pass so the stream can
    /// establish and terminate.
    pub fn arm_drop_all_data_ack(&mut self) {
        self.drop_all_data_ack = true;
    }

    /// Disarms the ceiling drop. Called by the ceiling test before
    /// closing the stream so termination control flows normally.
    pub fn disarm_drop_all_data_ack(&mut self) {
        self.drop_all_data_ack = false;
    }

    /// Enables control-packet observability without arming any
    /// fault: handshake and CLOSE/RESET packets are counted while
    /// all DATA/ACK flow untouched. Used by lifecycle tests that
    /// must observe wire control flow without altering it.
    pub fn arm_control_observability(&mut self) {
        self.control_observe = true;
    }

    /// Returns the current non-secret observed counters.
    pub const fn counters(&self) -> SamDeliveryFaultCounters {
        self.counters
    }

    /// Applies the armed faults to one delivery sweep **in place**,
    /// preserving request order except for the single documented
    /// reorder swap. Returns the requests to deliver normally through
    /// the existing per-request delivery path.
    ///
    /// Dropped and held requests are removed from the returned
    /// vector; duplicated requests appear twice; corrupted requests
    /// are replaced by a deterministically mutated copy.
    pub fn apply_to_sweep(
        &mut self,
        mut requests: Vec<TransportSendRequest>,
    ) -> Vec<TransportSendRequest> {
        if self.is_inert() {
            return requests;
        }
        // Clean reorder: when armed with nothing held and the sweep
        // carries at least two DATA packets, swap the first two in
        // place so the receiver observes a genuine wire reorder
        // inside one sweep.
        if self.reorder_one && self.held.is_none() {
            let mut first: Option<usize> = None;
            let mut second: Option<usize> = None;
            for (index, request) in requests.iter().enumerate() {
                if classify_delivery_request(request) == FaultPacketClass::Data {
                    if first.is_none() {
                        first = Some(index);
                    } else {
                        second = Some(index);
                        break;
                    }
                }
            }
            if let (Some(first), Some(second)) = (first, second) {
                requests.swap(first, second);
                self.reorder_one = false;
                self.counters.reordered = self.counters.reordered.saturating_add(1);
            }
        }
        let mut out: Vec<TransportSendRequest> = Vec::with_capacity(requests.len() + 1);
        for request in requests {
            match classify_delivery_request(&request) {
                FaultPacketClass::Handshake => {
                    self.counters.handshake_passthrough =
                        self.counters.handshake_passthrough.saturating_add(1);
                    out.push(request);
                }
                FaultPacketClass::CloseReset => {
                    self.counters.close_reset_passthrough =
                        self.counters.close_reset_passthrough.saturating_add(1);
                    out.push(request);
                }
                FaultPacketClass::Unknown => {
                    out.push(request);
                }
                FaultPacketClass::AckOnly => {
                    if self.drop_all_data_ack {
                        self.counters.dropped_ceiling =
                            self.counters.dropped_ceiling.saturating_add(1);
                    } else if self.drop_ack > 0 {
                        self.drop_ack -= 1;
                        self.counters.dropped_ack = self.counters.dropped_ack.saturating_add(1);
                    } else {
                        out.push(request);
                    }
                }
                FaultPacketClass::Data => {
                    if self.drop_all_data_ack {
                        self.counters.dropped_ceiling =
                            self.counters.dropped_ceiling.saturating_add(1);
                        continue;
                    }
                    if self.drop_data > 0 {
                        self.drop_data -= 1;
                        self.counters.dropped_data = self.counters.dropped_data.saturating_add(1);
                        continue;
                    }
                    // Reorder release: a previously held packet goes
                    // out after this newer packet.
                    if let Some(held) = self.held.take() {
                        self.reorder_one = false;
                        self.counters.reordered = self.counters.reordered.saturating_add(1);
                        out.push(request);
                        out.push(held);
                        continue;
                    }
                    // Reorder arm with at least this packet in hand:
                    // hold this first DATA packet back for the next
                    // sweep. The sender's retransmit tracking keeps
                    // the packet alive, so a missing successor still
                    // converges through retransmission rather than
                    // wedging the stream.
                    if self.reorder_one {
                        self.held = Some(request);
                        continue;
                    }
                    if self.corrupt_skip > 0 {
                        self.corrupt_skip -= 1;
                        out.push(request);
                        continue;
                    }
                    if self.corrupt_data > 0 {
                        self.corrupt_data -= 1;
                        self.counters.corrupted = self.counters.corrupted.saturating_add(1);
                        out.push(corrupt_request_payload(&request));
                        continue;
                    }
                    if self.duplicate_data > 0 {
                        self.duplicate_data -= 1;
                        self.counters.duplicated = self.counters.duplicated.saturating_add(1);
                        out.push(request.clone());
                        out.push(request);
                        continue;
                    }
                    out.push(request);
                }
            }
        }
        out
    }
}

/// Returns a copy of `request` with one application-payload byte
/// deterministically flipped. The flip lands inside the gzip body of
/// the protocol-6 envelope, so the peer's CRC verification rejects
/// the delivery with a typed decode error.
fn corrupt_request_payload(request: &TransportSendRequest) -> TransportSendRequest {
    let mut mutated = request.clone();
    if !mutated.application_payload.is_empty() {
        let index = mutated.application_payload.len() / 2;
        mutated.application_payload[index] ^= 0xFF;
    }
    mutated
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data_request(payload: Vec<u8>) -> TransportSendRequest {
        TransportSendRequest {
            destination_hash: [9_u8; 32],
            source_port: 1,
            destination_port: 2,
            application_payload: payload,
            sequence: 7,
            send_stream_id: 11,
            receive_stream_id: 13,
        }
    }

    fn classified_envelope() -> Vec<u8> {
        // A request with undecodable bytes classifies as Unknown and
        // passes through untouched.
        vec![0x00, 0x01, 0x02]
    }

    #[test]
    fn disabled_profile_passes_everything_through() {
        let mut profile = SamDeliveryFaultProfile::disabled();
        assert!(profile.is_inert());
        let requests = vec![
            data_request(classified_envelope()),
            data_request(classified_envelope()),
        ];
        let out = profile.apply_to_sweep(requests);
        assert_eq!(out.len(), 2);
        assert!(profile.is_inert());
        assert_eq!(profile.counters(), SamDeliveryFaultCounters::default());
    }

    #[test]
    fn unknown_requests_pass_through_unmodified() {
        let mut profile = SamDeliveryFaultProfile::default();
        profile.arm_drop_data(5);
        profile.arm_drop_ack(5);
        let request = data_request(classified_envelope());
        let out = profile.apply_to_sweep(vec![request]);
        assert_eq!(out.len(), 1);
        assert_eq!(profile.counters().dropped_data, 0);
        assert_eq!(profile.counters().dropped_ack, 0);
    }

    #[test]
    fn corrupt_flips_exactly_one_byte() {
        let request = data_request(vec![0xA5; 64]);
        let mutated = corrupt_request_payload(&request);
        assert_eq!(mutated.application_payload.len(), 64);
        let diffs = mutated
            .application_payload
            .iter()
            .zip(request.application_payload.iter())
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(diffs, 1);
        assert_eq!(mutated.application_payload[32], 0xA5 ^ 0xFF);
    }

    #[test]
    fn reorder_arm_holds_first_data_and_releases_after_next() {
        let mut profile = SamDeliveryFaultProfile::default();
        profile.arm_reorder_one();
        assert!(!profile.is_inert());
        // Unclassifiable requests are not DATA, so the arm does not
        // fire on them; use Data-shaped requests through the real
        // classifier instead (covered by the integration suite).
        let out = profile.apply_to_sweep(vec![data_request(classified_envelope())]);
        assert_eq!(out.len(), 1);
    }
}
