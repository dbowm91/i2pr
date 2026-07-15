//! Publicly grouped common-structure namespaces.
//!
//! The implementation remains behind the crate-root compatibility façade so
//! existing `i2pr_proto::*` imports remain stable. These private leaf modules
//! make ownership visible to later protocol work without widening helper
//! visibility or introducing a universal wire-codec trait.

mod certificate;
mod date;
mod identity;
mod keys;
mod lease;
mod mapping;
mod router_info;

pub use certificate::*;
pub use date::*;
pub use identity::*;
pub use keys::*;
pub use lease::*;
pub use mapping::*;
pub use router_info::*;

pub use crate::common_impl::{
    MAX_COMMON_STRUCTURE_SIZE, MAX_ENCRYPTION_KEYS, MAX_LEASES, MAX_MAPPING_BODY_SIZE,
    MAX_ROUTER_ADDRESSES,
};
