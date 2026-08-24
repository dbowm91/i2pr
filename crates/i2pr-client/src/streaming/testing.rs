#![allow(dead_code)]

//! Test helpers for the streaming layer.

use crate::streaming::clock::ManualClock;
use crate::streaming::config::StreamingConfig;
use crate::streaming::manager::StreamingManager;

/// Returns a balanced test configuration.
pub fn test_config() -> StreamingConfig {
    StreamingConfig::balanced()
}

/// Returns a minimal test configuration with small windows.
pub fn minimal_config() -> StreamingConfig {
    StreamingConfig::try_new(4, 2, 2, 4, 4, 4, 8, 4, 5_000, 30_000, 5_000, 2)
        .expect("minimal streaming config")
}

/// Creates a manual clock starting at zero.
pub fn test_clock() -> ManualClock {
    ManualClock::new(0)
}

/// Creates a test StreamingManager.
pub fn test_manager() -> StreamingManager {
    StreamingManager::new(test_config())
}

/// Creates a minimal test StreamingManager.
pub fn minimal_manager() -> StreamingManager {
    StreamingManager::new(minimal_config())
}
