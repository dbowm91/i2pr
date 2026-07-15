//! I2NP façade grouped by header, NetDB, delivery, tunnel, and deferred data.
//!
//! The strict top-level registry remains in the compatibility implementation
//! module. Leaf namespaces below expose the stable public types without
//! making decode helpers public or moving policy into the codec.

mod deferred;
mod delivery;
mod header;
mod netdb;
mod tunnel;

pub use crate::i2np_impl::{I2npBody, I2npMessage};
pub use deferred::*;
pub use delivery::*;
pub use header::*;
pub use netdb::*;
pub use tunnel::*;
