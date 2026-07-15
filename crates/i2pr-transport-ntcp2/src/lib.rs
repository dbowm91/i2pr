//! Runtime-neutral NTCP2 cryptographic foundation.
//!
//! Plan 032 adds protocol-specific cryptographic composition and deterministic
//! transcript stages. Complete handshake messages, frames, blocks, sockets,
//! and link management remain later-plan responsibilities.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod address;
mod block;
pub mod constants;
pub mod crypto;
mod frame;
mod handshake;
mod state_machine;
