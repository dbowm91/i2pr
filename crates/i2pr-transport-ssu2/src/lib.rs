//! Runtime-neutral SSU2 v2 protocol foundation.
//!
//! Plan 155 establishes strict, vector-backed address/header/block
//! primitives without implementing a live handshake or opening UDP
//! sockets. This crate owns protocol values only: no Tokio, no
//! sockets, no filesystem I/O, no async functions, no timers, and no task
//! ownership. Later plans add the Noise handshake (156), the data
//! phase (157), and the runtime UDP adapter in `i2pr-runtime` (158).
//!
//! Normative traceability: `specs/protocols/09-ssu2.md` and the
//! Milestone 8 source refresh in `specs/SOURCES.md` (I2P website
//! commit `88596022920bdf99f27db27688faf4f204792fcd`; SSU2 page
//! `Completed`, accurate for 0.9.69). Classical SSU2 v2 is the
//! target; PQ-hybrid v3/v4 is deferred compatibility-watch debt and
//! SSU1 remains unsupported. No handshake/data-phase
//! interoperability is claimed by this crate.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod address;
pub mod block;
pub mod constants;
pub mod header;
pub mod packet;

pub use address::{
    ConfiguredListenAddress, ResolvedDialTarget, Ssu2AddressClass, Ssu2AddressError,
    Ssu2AddressMaterial, Ssu2Capabilities, Ssu2Endpoint, Ssu2Introducer, Ssu2RouterAddress,
    Ssu2TransportStyle,
};
pub use block::{
    AckBlock, AddressBlock, Block, BlockError, CongestionBlock, DecodedBlock, FirstFragmentBlock,
    FirstPacketNumberBlock, FollowOnFragmentBlock, I2npMessageBlock, NewTokenBlock, OptionsBlock,
    PaddingBlock, ParsedBlocks, PathChallengeBlock, PathResponseBlock, PeerTestBlock,
    RelayIntroBlock, RelayRequestBlock, RelayResponseBlock, RelayResponseCode, RelayTagBlock,
    RouterInfoBlock, TerminationBlock, TerminationReason, TimestampBlock,
};
pub use header::{
    DataHeader, HeaderError, HeaderForm, LongHeader, MessageType, SessionConfirmedHeader,
};
pub use packet::{DatagramLengthClass, PacketError};
