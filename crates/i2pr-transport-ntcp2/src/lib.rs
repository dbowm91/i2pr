//! Runtime-neutral NTCP2 cryptographic foundation.
//!
//! Plan 032 adds protocol-specific cryptographic composition and deterministic
//! transcript stages; Plan 033 adds bounded handshake messages, blocks, and
//! consuming runtime-neutral state machines. Data frames, sockets, and link
//! management remain later-plan responsibilities.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod address;
mod block;
pub mod constants;
pub mod crypto;
mod frame;
pub mod handshake;
pub mod state_machine;
