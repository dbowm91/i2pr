//! Minimal I2P Streaming core.
//!
//! Plan 123 owns the first reliable bidirectional byte-stream API in
//! `i2pr`. The streaming layer is destination-scoped: each local
//! destination owns one [`StreamingManager`] that mediates every
//! outbound and inbound streaming connection. The streaming layer never
//! owns sockets or network primitives; it composes with Plan 122's
//! destination routing for outbound composition and Plan 122's inbound
//! dispatch for inbound routing.
//!
//! # Layering
//!
//! ```text
//! StreamingManager
//!   -> Connection state machine (synchronous, deterministic clock)
//!     -> wire codec (i2pr-proto streaming)
//!     -> Plan 122 compose_outbound_delivery / DestinationDispatcher
//!     -> DestinationRegistry / DestinationTunnelPool
//! ```
//!
//! SAM and I2CP remain downstream of this module; this plan does not add
//! them.

#![forbid(unsafe_code)]

mod clock;
pub mod config;
mod congestion;
pub mod connection;
mod errors;
pub mod events;
pub mod manager;
mod recv_window;
mod retransmit;
mod send_window;
pub mod transport;

#[allow(unused_imports)]
pub use clock::{Clock, ManualClock, SystemClock};
#[allow(unused_imports)]
pub use config::{
    MAX_INBOUND_PENDING_STREAMS, MAX_OUTBOUND_PENDING_STREAMS, MAX_PACKET_PAYLOAD_BYTES,
    MAX_STREAMING_PAYLOAD_BYTES_PER_PACKET, MIN_STREAMING_PAYLOAD_BYTES_PER_PACKET,
    StreamingConfig, StreamingConfigError,
};
#[allow(unused_imports)]
pub use congestion::{
    CongestionConfig, CongestionDecision, CongestionPolicy, INITIAL_CONGESTION_WINDOW,
    MAX_CONGESTION_WINDOW, MIN_CONGESTION_WINDOW,
};
#[allow(unused_imports)]
pub use connection::{
    ConnectionId, ConnectionState, ConnectionTransition, StreamDirection, StreamingConnection,
    StreamingConnectionEvent,
};
pub use errors::StreamingError;
#[allow(unused_imports)]
pub use events::{AckObservation, InboundStreamEvent, OutboundStreamEvent, WirePacketObservation};
#[allow(unused_imports)]
pub use manager::{
    ConnectOutcome, ConnectionRefused, DEFAULT_ADVERTISED_MAX_PAYLOAD, DeliveredApplicationBytes,
    ListenerOutcome, MAX_STREAMS_PER_DESTINATION, StreamingEvent, StreamingManager,
    StreamingManagerError,
};
#[allow(unused_imports)]
pub use recv_window::{RecvWindowConfig, RecvWindowDecision, RecvWindowPolicy};
#[allow(unused_imports)]
pub use retransmit::{
    MAX_RETRANSMIT_ATTEMPTS, MAX_RTO_MILLIS, MIN_RTO_MILLIS, RetransmitConfig, RetransmitDecision,
    RetransmitPolicy, RttSample,
};
#[allow(unused_imports)]
pub use send_window::{SendWindowConfig, SendWindowDecision, SendWindowPolicy};
pub use transport::{StreamingTransport, TransportError, TransportOutcome, TransportSendRequest};

#[cfg(test)]
pub(crate) mod testing;
