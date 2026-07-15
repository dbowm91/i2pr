//! Typed public-key and signature representations.

use super::*;

/// A protocol signing-key type and its known public/signature lengths.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SigningKeyType {
    DsaSha1,
    EcdsaSha256P256,
    EcdsaSha384P384,
    EcdsaSha512P521,
    RsaSha2562048,
    RsaSha3843072,
    RsaSha5124096,
    EdDsaSha512Ed25519,
    EdDsaSha512Ed25519ph,
    RedDsaSha512Ed25519,
    Unknown(u16),
}

impl SigningKeyType {
    /// Decodes the numeric protocol identifier without mapping unknown values
    /// to a default algorithm.
    pub const fn from_code(value: u16) -> Self {
        match value {
            0 => Self::DsaSha1,
            1 => Self::EcdsaSha256P256,
            2 => Self::EcdsaSha384P384,
            3 => Self::EcdsaSha512P521,
            4 => Self::RsaSha2562048,
            5 => Self::RsaSha3843072,
            6 => Self::RsaSha5124096,
            7 => Self::EdDsaSha512Ed25519,
            8 => Self::EdDsaSha512Ed25519ph,
            11 => Self::RedDsaSha512Ed25519,
            other => Self::Unknown(other),
        }
    }

    /// Returns the numeric protocol identifier.
    pub const fn code(self) -> u16 {
        match self {
            Self::DsaSha1 => 0,
            Self::EcdsaSha256P256 => 1,
            Self::EcdsaSha384P384 => 2,
            Self::EcdsaSha512P521 => 3,
            Self::RsaSha2562048 => 4,
            Self::RsaSha3843072 => 5,
            Self::RsaSha5124096 => 6,
            Self::EdDsaSha512Ed25519 => 7,
            Self::EdDsaSha512Ed25519ph => 8,
            Self::RedDsaSha512Ed25519 => 11,
            Self::Unknown(value) => value,
        }
    }

    /// Returns the encoded public-key length when this type is known.
    pub const fn public_key_len(self) -> Option<usize> {
        match self {
            Self::DsaSha1 => Some(128),
            Self::EcdsaSha256P256 => Some(64),
            Self::EcdsaSha384P384 => Some(96),
            Self::EcdsaSha512P521 => Some(132),
            Self::RsaSha2562048 => Some(256),
            Self::RsaSha3843072 => Some(384),
            Self::RsaSha5124096 => Some(512),
            Self::EdDsaSha512Ed25519 | Self::EdDsaSha512Ed25519ph | Self::RedDsaSha512Ed25519 => {
                Some(32)
            }
            Self::Unknown(_) => None,
        }
    }

    /// Returns the encoded signature length when this type is known.
    pub const fn signature_len(self) -> Option<usize> {
        match self {
            Self::DsaSha1 => Some(40),
            Self::EcdsaSha256P256 => Some(64),
            Self::EcdsaSha384P384 => Some(96),
            Self::EcdsaSha512P521 => Some(132),
            Self::RsaSha2562048 => Some(256),
            Self::RsaSha3843072 => Some(384),
            Self::RsaSha5124096 => Some(512),
            Self::EdDsaSha512Ed25519 | Self::EdDsaSha512Ed25519ph | Self::RedDsaSha512Ed25519 => {
                Some(64)
            }
            Self::Unknown(_) => None,
        }
    }
}

/// A protocol encryption-key type and its known public-key length.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CryptoKeyType {
    ElGamal,
    P256,
    P384,
    P521,
    X25519,
    MlKem512X25519,
    MlKem768X25519,
    MlKem1024X25519,
    Unknown(u16),
}

impl CryptoKeyType {
    /// Decodes a numeric protocol identifier with an explicit unknown path.
    pub const fn from_code(value: u16) -> Self {
        match value {
            0 => Self::ElGamal,
            1 => Self::P256,
            2 => Self::P384,
            3 => Self::P521,
            4 => Self::X25519,
            5 => Self::MlKem512X25519,
            6 => Self::MlKem768X25519,
            7 => Self::MlKem1024X25519,
            other => Self::Unknown(other),
        }
    }

    /// Returns the numeric protocol identifier.
    pub const fn code(self) -> u16 {
        match self {
            Self::ElGamal => 0,
            Self::P256 => 1,
            Self::P384 => 2,
            Self::P521 => 3,
            Self::X25519 => 4,
            Self::MlKem512X25519 => 5,
            Self::MlKem768X25519 => 6,
            Self::MlKem1024X25519 => 7,
            Self::Unknown(value) => value,
        }
    }

    /// Returns the encoded public-key length when known for a public-key field.
    pub const fn public_key_len(self) -> Option<usize> {
        match self {
            Self::ElGamal => Some(256),
            Self::P256 => Some(64),
            Self::P384 => Some(96),
            Self::P521 => Some(132),
            Self::X25519 | Self::MlKem512X25519 | Self::MlKem768X25519 | Self::MlKem1024X25519 => {
                Some(32)
            }
            Self::Unknown(_) => None,
        }
    }

    pub(super) const fn allowed_in_identity(self) -> bool {
        matches!(self, Self::ElGamal | Self::X25519)
    }
}

/// A validated public encryption key.
#[derive(Clone, Eq, PartialEq)]
pub struct PublicKey {
    key_type: CryptoKeyType,
    bytes: Vec<u8>,
}

impl PublicKey {
    /// Creates a key after checking its algorithm-specific encoded length.
    pub fn new(key_type: CryptoKeyType, bytes: Vec<u8>) -> Result<Self, CodecError> {
        let expected = key_type
            .public_key_len()
            .ok_or_else(|| unsupported(0, "encryption key type", key_type.code() as u64))?;
        if bytes.len() != expected {
            return Err(invalid(0, "encryption public key length"));
        }
        Ok(Self { key_type, bytes })
    }

    /// Returns the key type.
    pub const fn key_type(&self) -> CryptoKeyType {
        self.key_type
    }

    /// Returns the validated key bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl fmt::Debug for PublicKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PublicKey")
            .field("key_type", &self.key_type)
            .field("length", &self.bytes.len())
            .finish()
    }
}

/// A validated public signing key.
#[derive(Clone, Eq, PartialEq)]
pub struct SigningPublicKey {
    key_type: SigningKeyType,
    bytes: Vec<u8>,
}

impl SigningPublicKey {
    /// Creates a key after checking its algorithm-specific encoded length.
    pub fn new(key_type: SigningKeyType, bytes: Vec<u8>) -> Result<Self, CodecError> {
        let expected = key_type
            .public_key_len()
            .ok_or_else(|| unsupported(0, "signing key type", key_type.code() as u64))?;
        if bytes.len() != expected {
            return Err(invalid(0, "signing public key length"));
        }
        Ok(Self { key_type, bytes })
    }

    /// Returns the key type.
    pub const fn key_type(&self) -> SigningKeyType {
        self.key_type
    }

    /// Returns the validated key bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl fmt::Debug for SigningPublicKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SigningPublicKey")
            .field("key_type", &self.key_type)
            .field("length", &self.bytes.len())
            .finish()
    }
}

/// A signature with its inferred signing-key type made explicit.
#[derive(Clone, Eq, PartialEq)]
pub struct SignatureValue {
    key_type: SigningKeyType,
    bytes: Vec<u8>,
}

impl SignatureValue {
    /// Creates a signature after checking the type-specific encoded length.
    pub fn new(key_type: SigningKeyType, bytes: Vec<u8>) -> Result<Self, CodecError> {
        let expected = key_type
            .signature_len()
            .ok_or_else(|| unsupported(0, "signature type", key_type.code() as u64))?;
        if bytes.len() != expected {
            return Err(invalid(0, "signature length"));
        }
        Ok(Self { key_type, bytes })
    }

    /// Returns the signing-key type.
    pub const fn key_type(&self) -> SigningKeyType {
        self.key_type
    }

    /// Returns the signature bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl fmt::Debug for SignatureValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SignatureValue")
            .field("key_type", &self.key_type)
            .field("length", &self.bytes.len())
            .finish()
    }
}
