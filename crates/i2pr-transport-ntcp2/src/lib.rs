//! NTCP2 ownership boundary reserved for later protocol plans.
//!
//! Plan 031 deliberately exposes no handshake, frame, block, socket, address,
//! or cryptographic behavior.  The private modules establish physical
//! ownership locations so later work does not leak Tokio or filesystem
//! responsibilities into this crate.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod address;
mod block;
mod constants;
mod crypto;
mod frame;
mod handshake;
mod state_machine;
