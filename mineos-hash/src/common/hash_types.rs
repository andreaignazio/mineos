/// Common hash types and utilities for mining algorithms

use serde::{Deserialize, Serialize};
use std::fmt;
use sha3::{Digest, Sha3_256};

/// 256-bit hash (32 bytes)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Hash256(pub [u8; 32]);

impl Hash256 {
    /// Create from bytes
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    
    /// Create from slice (must be 32 bytes)
    pub fn from_slice(slice: &[u8]) -> Result<Self, &'static str> {
        if slice.len() != 32 {
            return Err("Hash256 requires exactly 32 bytes");
        }
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(slice);
        Ok(Self(bytes))
    }
    
    /// Get as bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
    
    /// Get as mutable bytes
    pub fn as_bytes_mut(&mut self) -> &mut [u8; 32] {
        &mut self.0
    }
    
    /// Convert to hex string
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
    
    /// Parse from hex string
    pub fn from_hex(hex: &str) -> Result<Self, hex::FromHexError> {
        let bytes = hex::decode(hex)?;
        Self::from_slice(&bytes).map_err(|_| hex::FromHexError::InvalidStringLength)
    }
    
    /// Check if hash meets difficulty target
    pub fn meets_target(&self, target: &Hash256) -> bool {
        // Compare as little-endian integers
        for i in (0..32).rev() {
            if self.0[i] < target.0[i] {
                return true;
            }
            if self.0[i] > target.0[i] {
                return false;
            }
        }
        true // Equal means it meets target
    }
    
    /// Convert to u64 (using first 8 bytes)
    pub fn to_u64(&self) -> u64 {
        u64::from_le_bytes(self.0[..8].try_into().unwrap())
    }
}

impl fmt::Display for Hash256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl Default for Hash256 {
    fn default() -> Self {
        Self([0u8; 32])
    }
}

/// Double SHA-256 hash (used in Bitcoin/Ravencoin merkle trees)
pub fn double_sha256(data: &[u8]) -> Hash256 {
    let mut hasher = Sha3_256::new();
    hasher.update(data);
    let first = hasher.finalize();
    
    let mut hasher = Sha3_256::new();
    hasher.update(&first);
    let result = hasher.finalize();
    
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&result);
    Hash256::from_bytes(bytes)
}

/// Block header for mining
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
    /// Previous block hash
    pub prev_hash: Hash256,
    /// Merkle root of transactions
    pub merkle_root: Hash256,
    /// Block timestamp
    pub timestamp: u32,
    /// Difficulty bits
    pub bits: u32,
    /// Nonce value
    pub nonce: u64,
    /// Block height (for KawPow)
    pub height: u64,
}

impl BlockHeader {
    /// Serialize header to bytes (80 bytes standard + 8 bytes height for KawPow)
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(88);
        bytes.extend_from_slice(self.prev_hash.as_bytes());
        bytes.extend_from_slice(self.merkle_root.as_bytes());
        bytes.extend_from_slice(&self.timestamp.to_le_bytes());
        bytes.extend_from_slice(&self.bits.to_le_bytes());
        bytes.extend_from_slice(&self.nonce.to_le_bytes());
        bytes.extend_from_slice(&self.height.to_le_bytes());
        bytes
    }
    
    /// Create test header
    pub fn test_header(height: u64) -> Self {
        Self {
            prev_hash: Hash256::default(),
            merkle_root: Hash256::default(),
            timestamp: 1234567890,
            bits: 0x1d00ffff,
            nonce: 0,
            height,
        }
    }
}

/// Mining result
#[derive(Debug, Clone)]
pub struct MiningResult {
    /// The nonce that produced valid hash
    pub nonce: u64,
    /// The resulting hash
    pub hash: Hash256,
    /// Mix hash (for Ethash-like algorithms)
    pub mix_hash: Option<Hash256>,
}

/// Difficulty target utilities
pub struct Difficulty;

impl Difficulty {
    /// Convert bits to target hash
    pub fn bits_to_target(bits: u32) -> Hash256 {
        let exponent = (bits >> 24) as usize;
        let mantissa = bits & 0x00ffffff;
        
        let mut target = [0u8; 32];
        
        if exponent <= 3 {
            let shift = 8 * (3 - exponent);
            target[..3].copy_from_slice(&mantissa.to_be_bytes()[1..]);
            for i in 0..3 {
                target[i] >>= shift;
            }
        } else {
            let offset = exponent - 3;
            if offset < 29 {
                target[offset..offset + 3].copy_from_slice(&mantissa.to_be_bytes()[1..]);
            }
        }
        
        Hash256::from_bytes(target)
    }
    
    /// Calculate difficulty from target
    pub fn target_to_difficulty(target: &Hash256) -> f64 {
        // Max target (difficulty 1)
        let max_target = Self::bits_to_target(0x1d00ffff);
        
        // Calculate difficulty as max_target / target
        let max_val = u128::from_le_bytes(max_target.as_bytes()[..16].try_into().unwrap());
        let target_val = u128::from_le_bytes(target.as_bytes()[..16].try_into().unwrap());
        
        if target_val == 0 {
            return f64::MAX;
        }
        
        max_val as f64 / target_val as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash256_basics() {
        let hash = Hash256::from_bytes([1u8; 32]);
        assert_eq!(hash.as_bytes(), &[1u8; 32]);
        
        let hex = hash.to_hex();
        let hash2 = Hash256::from_hex(&hex).unwrap();
        assert_eq!(hash, hash2);
    }
    
    #[test]
    fn test_meets_target() {
        let hash = Hash256::from_bytes([0u8; 32]);
        let mut target = [0u8; 32];
        target[31] = 1; // Very easy target
        let target = Hash256::from_bytes(target);
        
        assert!(hash.meets_target(&target));
        
        let hash2 = Hash256::from_bytes([2u8; 32]);
        assert!(!hash2.meets_target(&target));
    }
    
    #[test]
    fn test_difficulty_conversion() {
        let bits = 0x1d00ffff;
        let target = Difficulty::bits_to_target(bits);
        
        // Should produce a reasonable target
        assert_ne!(target, Hash256::default());
        
        let difficulty = Difficulty::target_to_difficulty(&target);
        assert!(difficulty > 0.0);
        assert!(difficulty < f64::MAX);
    }
}