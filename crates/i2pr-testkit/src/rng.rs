use std::fmt;
use std::str::FromStr;

use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};
use sha2::{Digest, Sha256};

/// Maximum domain label retained by deterministic seed derivation.
pub const MAX_DOMAIN_LABEL_BYTES: usize = 64;

/// A stable 128-bit seed used to reproduce deterministic tests.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ReproducibilitySeed([u8; 16]);

impl ReproducibilitySeed {
    /// Creates a seed from raw bytes.
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Creates a seed from a numeric value using big-endian formatting.
    pub const fn from_u128(value: u128) -> Self {
        Self(value.to_be_bytes())
    }

    /// Returns raw seed bytes.
    pub const fn as_bytes(self) -> [u8; 16] {
        self.0
    }

    /// Derives an independent seed for a bounded domain label.
    pub fn derive(self, label: &str) -> Result<Self, SeedDerivationError> {
        if label.is_empty() {
            return Err(SeedDerivationError::EmptyLabel);
        }
        if label.len() > MAX_DOMAIN_LABEL_BYTES {
            return Err(SeedDerivationError::LabelTooLong {
                maximum: MAX_DOMAIN_LABEL_BYTES,
            });
        }
        Ok(Self::derive_bytes(self, label.as_bytes()))
    }

    pub(crate) fn derive_bytes(seed: Self, label: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"i2pr-testkit/domain-separation/v1\0");
        hasher.update(seed.0);
        hasher.update((label.len() as u16).to_be_bytes());
        hasher.update(label);
        let digest = hasher.finalize();
        let mut bytes = [0_u8; 16];
        bytes.copy_from_slice(&digest[..16]);
        Self(bytes)
    }
}

impl fmt::Display for ReproducibilitySeed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl FromStr for ReproducibilitySeed {
    type Err = SeedParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.strip_prefix("0x").unwrap_or(value);
        if value.len() != 32 {
            return Err(SeedParseError::WrongLength {
                actual: value.len(),
            });
        }
        let mut bytes = [0_u8; 16];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            let high = hex_value(pair[0]).ok_or(SeedParseError::InvalidHex { index: index * 2 })?;
            let low = hex_value(pair[1]).ok_or(SeedParseError::InvalidHex {
                index: index * 2 + 1,
            })?;
            bytes[index] = (high << 4) | low;
        }
        Ok(Self(bytes))
    }
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

/// Error returned when a seed domain is invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SeedDerivationError {
    /// The label is empty.
    EmptyLabel,
    /// The label exceeds the deterministic bound.
    LabelTooLong { maximum: usize },
}

impl fmt::Display for SeedDerivationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyLabel => formatter.write_str("seed domain label must not be empty"),
            Self::LabelTooLong { maximum } => {
                write!(formatter, "seed domain label exceeds {maximum} bytes")
            }
        }
    }
}

impl std::error::Error for SeedDerivationError {}

/// Error returned when a reproducibility seed is not exactly 128 bits of hex.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SeedParseError {
    /// The input did not contain 32 hexadecimal characters.
    WrongLength { actual: usize },
    /// A non-hexadecimal byte occurred at this zero-based character offset.
    InvalidHex { index: usize },
}

impl fmt::Display for SeedParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongLength { actual } => {
                write!(
                    formatter,
                    "seed must contain 32 hex characters, got {actual}"
                )
            }
            Self::InvalidHex { index } => write!(formatter, "seed contains invalid hex at {index}"),
        }
    }
}

impl std::error::Error for SeedParseError {}

/// A deterministic ChaCha8 generator for tests and simulations only.
#[derive(Debug)]
pub struct DeterministicRng {
    seed: ReproducibilitySeed,
    rng: ChaCha8Rng,
}

impl DeterministicRng {
    /// Creates a generator from a reproducibility seed.
    pub fn new(seed: ReproducibilitySeed) -> Self {
        let mut expanded_seed = [0_u8; 32];
        expanded_seed[..16].copy_from_slice(&seed.as_bytes());
        expanded_seed[16..].copy_from_slice(&seed.as_bytes());
        Self {
            seed,
            rng: ChaCha8Rng::from_seed(expanded_seed),
        }
    }

    /// Creates an independent child generator without sharing mutable state.
    pub fn child(&self, label: &str) -> Result<Self, SeedDerivationError> {
        Ok(Self::new(self.seed.derive(label)?))
    }

    /// Returns the generator's domain seed.
    pub const fn seed(&self) -> ReproducibilitySeed {
        self.seed
    }

    /// Generates the next deterministic 64-bit value.
    pub fn next_u64(&mut self) -> u64 {
        self.rng.next_u64()
    }

    /// Fills a caller-provided buffer deterministically.
    pub fn fill_bytes(&mut self, bytes: &mut [u8]) {
        self.rng.fill_bytes(bytes);
    }
}
