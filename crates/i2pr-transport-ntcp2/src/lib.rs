//! Runtime-neutral NTCP2 cryptographic foundation.
//!
//! Plan 032 adds protocol-specific cryptographic composition and deterministic
//! transcript stages; Plan 033 adds bounded handshake messages and consuming
//! runtime-neutral state machines; Plan 034 adds authenticated data frames,
//! bounded payload blocks, and deterministic frame-state owners. Sockets,
//! link management, and runtime scheduling remain later-plan responsibilities.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod address;
pub mod block;
pub mod constants;
pub mod crypto;
pub mod frame;
pub mod handshake;
pub mod state_machine;
