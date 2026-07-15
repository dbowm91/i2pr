//! Structural NetDB message bodies; reply encryption semantics are deferred.

pub use crate::i2np_impl::{
    DatabaseLookupMessage, DatabaseSearchReplyMessage, DatabaseStoreData, DatabaseStoreMessage,
    DatabaseStoreType, MAX_DATABASE_LOOKUP_EXCLUDED_PEERS, MAX_DATABASE_SEARCH_REPLY_PEERS,
    ReplyEncryption, ReplySecret,
};
