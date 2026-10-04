//! Plan 294 canonical address-book owner.
//!
//! This crate owns i2pr `.i2p` naming: four independent administrative
//! books (private, local, router, published) with fixed lookup
//! precedence, a subscription-derived table consulted last, typed
//! hostname and full-Destination validation, the Proposal `SetConfig`
//! domain, bounded subscription ingestion, deterministic versioned
//! generations, and a narrow read-only resolver handle.
//!
//! It owns no sockets, no Tokio tasks, no timers, no filesystem access,
//! no HTTP client, and no UI behavior. `i2pr-storage` persists opaque
//! generations; `i2pr-daemon` owns refresh tasks, timers, download
//! composition, and resolver-handle installation into SAM, service
//! tunnel, and control consumers. Administrative mutation goes only
//! through [`AddressBookControl`]; ordinary lookup goes only through
//! [`AddressBookResolver`], which cannot mutate.

#![forbid(unsafe_code)]

mod book;
mod config;
mod destination_text;
mod error;
mod generation;
mod hostname;
mod refresh;
mod resolver;
mod subscription;

pub use book::{
    AddressBook, AddressBookControl, BookKind, EntryMutation, EntryOutcome, Provenance,
    ResolvedEntry,
};
pub use config::{
    AddressBookConfig, ConfigKey, LogLevel, MAX_CONFIG_PATH_LEN, MAX_CONFIG_VALUE_LEN,
    parse_config_key,
};
pub use destination_text::{
    MAX_DESTINATION_BYTES, MAX_DESTINATION_TEXT_LEN, validate_destination_text,
};
pub use error::AddressBookError;
pub use generation::{
    GENERATION_VERSION, MAX_ENTRIES_PER_BOOK, MAX_GENERATION_BYTES, decode_generation,
    encode_generation,
};
pub use hostname::{Hostname, MAX_HOSTNAME_LEN, MAX_LABEL_LEN, parse_hostname};
pub use refresh::{RefreshDiagnostic, RefreshOutcome, RefreshQueue, RefreshReason};
pub use resolver::{AddressBookResolver, AddressBookSnapshot};
pub use subscription::{
    MAX_SUBSCRIBED_ENTRIES, MAX_SUBSCRIPTION_BODY_BYTES, MAX_SUBSCRIPTION_LINE_LEN,
    MAX_SUBSCRIPTION_URL_LEN, MAX_SUBSCRIPTION_URLS, SubscriptionSet, ingest_subscription_body,
    validate_subscription_url,
};
