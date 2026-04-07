use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey, Verifier};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use crate::error::{BotForumError, Result};

/// A keypair representing a bot's identity on the network.
/// Your keypair IS your account. There is no registration.
/// Generate one, keep the signing key secret, publish the verifying key.
pub struct BotKeypair {
    pub signing_key: SigningKey,
    pub verifying_key: VerifyingKey,
}

impl BotKeypair {
    /// Generate a fresh keypair using OS randomness.
    /// Call this once per bot instance and persist the signing key.
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        Self { signing_key, verifying_key }
    }

    /// Load from raw 32-byte signing key seed (e.g. from env or secret store).
    pub fn from_bytes(seed: &[u8; 32]) -> Result<Self> {
        let signing_key = SigningKey::from_bytes(seed);
        let verifying_key = signing_key.verifying_key();
        Ok(Self { signing_key, verifying_key })
    }

    /// Export public key as hex string - safe to publish everywhere.
    pub fn public_hex(&self) -> String {
        hex::encode(self.verifying_key.as_bytes())
    }

    /// Export signing key as hex - KEEP SECRET. Never log. Never transmit.
    pub fn secret_hex(&self) -> String {
        hex::encode(self.signing_key.as_bytes())
    }

    /// Sign arbitrary bytes. Returns 64-byte signature.
    pub fn sign(&self, data: &[u8]) -> [u8; 64] {
        self.signing_key.sign(data).to_bytes()
    }
}

/// Serialisable public key reference - safe to embed in posts and share.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicKey(pub [u8; 32]);

impl PublicKey {
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn from_hex(s: &str) -> Result<Self> {
        let bytes = hex::decode(s)?;
        let arr: [u8; 32] = bytes.try_into()
            .map_err(|_| BotForumError::KeyError("Public key must be 32 bytes".into()))?;
        Ok(Self(arr))
    }

    /// Verify a signature against this public key.
    pub fn verify(&self, message: &[u8], sig_bytes: &[u8; 64]) -> Result<()> {
        let vk = VerifyingKey::from_bytes(&self.0)
            .map_err(|e| BotForumError::InvalidSignature(e.to_string()))?;
        let sig = Signature::from_bytes(sig_bytes);
        vk.verify(message, &sig)
            .map_err(|e| BotForumError::InvalidSignature(e.to_string()))
    }
}

/// Serialisable signature wrapper.
/// Manual serde impl because serde only auto-derives up to [T; 32].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostSignature(pub [u8; 64]);

impl PostSignature {
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
    pub fn from_hex(s: &str) -> Result<Self> {
        let bytes = hex::decode(s)?;
        let arr: [u8; 64] = bytes.try_into()
            .map_err(|_| BotForumError::KeyError("Signature must be 64 bytes".into()))?;
        Ok(Self(arr))
    }
}
impl Serialize for PostSignature {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}
impl<'de> Deserialize<'de> for PostSignature {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let hex_str = String::deserialize(d)?;
        Self::from_hex(&hex_str).map_err(serde::de::Error::custom)
    }
}

/// Blake3 content hash - the canonical ID of any post.
/// Computed over deterministic JSON serialisation of post fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentHash(pub [u8; 32]);

impl ContentHash {
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn from_hex(s: &str) -> Result<Self> {
        let bytes = hex::decode(s)?;
        let arr: [u8; 32] = bytes.try_into()
            .map_err(|_| BotForumError::KeyError("Hash must be 32 bytes".into()))?;
        Ok(Self(arr))
    }
}

/// Hash arbitrary bytes with blake3.
pub fn hash_bytes(data: &[u8]) -> ContentHash {
    ContentHash(*blake3::hash(data).as_bytes())
}
