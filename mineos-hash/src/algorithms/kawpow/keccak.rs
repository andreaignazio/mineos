/// Keccak-f800 implementation for ProgPoW/KawPow
/// 
/// This is a 32-bit word variant of Keccak (vs standard 64-bit Keccak-f1600)
/// optimized for GPU efficiency. The 32-bit word size matches native GPU word size.

use byteorder::{ByteOrder, LittleEndian};

/// Number of rounds in Keccak-f800
const KECCAK_ROUNDS: usize = 22;

/// Keccak round constants for f800
const ROUND_CONSTANTS: [u32; KECCAK_ROUNDS] = [
    0x00000001, 0x00000082, 0x0000808a, 0x00008000,
    0x0000808b, 0x80000001, 0x80008081, 0x80008009,
    0x0000008a, 0x00000088, 0x80008009, 0x80000008,
    0x80008002, 0x80008003, 0x80008002, 0x80000080,
    0x0000800a, 0x8000000a, 0x80008081, 0x80008080,
    0x80000001, 0x80008008,
];

/// Rotation offsets for Keccak-f800
const RHO_OFFSETS: [u32; 25] = [
     0,  1, 62, 28, 27,
    36, 44,  6, 55, 20,
     3, 10, 43, 25, 39,
    41, 45, 15, 21,  8,
    18,  2, 61, 56, 14,
];

/// Pi step permutation indices
const PI_INDICES: [usize; 25] = [
     0,  6, 12, 18, 24,
     3,  9, 10, 16, 22,
     1,  7, 13, 19, 20,
     4,  5, 11, 17, 23,
     2,  8, 14, 15, 21,
];

/// Keccak-f800 state (25 x 32-bit words = 800 bits)
pub type KeccakState = [u32; 25];

/// Perform one round of Keccak-f800
#[inline]
fn keccak_f800_round(state: &mut KeccakState, round: usize) {
    // Theta step
    let mut c = [0u32; 5];
    for x in 0..5 {
        c[x] = state[x] ^ state[x + 5] ^ state[x + 10] ^ state[x + 15] ^ state[x + 20];
    }
    
    let mut d = [0u32; 5];
    for x in 0..5 {
        d[x] = c[(x + 4) % 5] ^ c[(x + 1) % 5].rotate_left(1);
    }
    
    for x in 0..5 {
        for y in 0..5 {
            state[y * 5 + x] ^= d[x];
        }
    }
    
    // Rho and Pi steps
    let mut b = [0u32; 25];
    for i in 0..25 {
        b[PI_INDICES[i]] = state[i].rotate_left(RHO_OFFSETS[i]);
    }
    
    // Chi step
    for y in 0..5 {
        let base = y * 5;
        let t = [b[base], b[base + 1], b[base + 2], b[base + 3], b[base + 4]];
        for x in 0..5 {
            state[base + x] = t[x] ^ ((!t[(x + 1) % 5]) & t[(x + 2) % 5]);
        }
    }
    
    // Iota step
    state[0] ^= ROUND_CONSTANTS[round];
}

/// Perform full Keccak-f800 permutation
pub fn keccak_f800(state: &mut KeccakState) {
    for round in 0..KECCAK_ROUNDS {
        keccak_f800_round(state, round);
    }
}

/// Hash32 type for Keccak output (256 bits)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hash32 {
    pub words: [u32; 8],
}

impl Hash32 {
    /// Create from bytes (little-endian)
    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        let mut words = [0u32; 8];
        for i in 0..8 {
            words[i] = LittleEndian::read_u32(&bytes[i * 4..]);
        }
        Self { words }
    }
    
    /// Convert to bytes (little-endian)
    pub fn to_bytes(&self) -> [u8; 32] {
        let mut bytes = [0u8; 32];
        for i in 0..8 {
            LittleEndian::write_u32(&mut bytes[i * 4..], self.words[i]);
        }
        bytes
    }
    
    /// Create from hex string
    pub fn from_hex(hex: &str) -> Result<Self, hex::FromHexError> {
        let bytes = hex::decode(hex)?;
        if bytes.len() != 32 {
            return Err(hex::FromHexError::InvalidStringLength);
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(Self::from_bytes(&arr))
    }
    
    /// Convert to hex string
    pub fn to_hex(&self) -> String {
        hex::encode(self.to_bytes())
    }
}

/// Keccak-f800 for ProgPoW (single absorb, fixed output)
pub fn keccak_f800_progpow(input: &[u32; 25]) -> Hash32 {
    let mut state = *input;
    keccak_f800(&mut state);
    
    // Extract first 8 words as output
    let mut words = [0u32; 8];
    words.copy_from_slice(&state[..8]);
    Hash32 { words }
}

/// Standard Keccak-256 using f800 (for compatibility testing)
pub fn keccak256_f800(data: &[u8]) -> Hash32 {
    let mut state = [0u32; 25];
    
    // Absorb phase (simplified for fixed-size input)
    // This is a simplified version - real implementation would handle arbitrary length
    let chunks = data.chunks_exact(72); // 72 bytes = 18 u32 words (rate for f800)
    
    for chunk in chunks {
        for i in 0..chunk.len() / 4 {
            state[i] ^= LittleEndian::read_u32(&chunk[i * 4..]);
        }
        keccak_f800(&mut state);
    }
    
    // Handle remainder and padding
    let remainder = data.len() % 72;
    if remainder > 0 {
        let last_chunk = &data[data.len() - remainder..];
        for i in 0..last_chunk.len() / 4 {
            state[i] ^= LittleEndian::read_u32(&last_chunk[i * 4..]);
        }
        // Add padding
        state[remainder / 4] ^= 0x01;
        state[17] ^= 0x80000000; // Last bit of rate
        keccak_f800(&mut state);
    }
    
    // Squeeze phase - extract 256 bits
    let mut words = [0u32; 8];
    words.copy_from_slice(&state[..8]);
    Hash32 { words }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keccak_f800_basic() {
        let mut state = [0u32; 25];
        state[0] = 0x12345678;
        let original = state;
        
        keccak_f800(&mut state);
        
        // State should be modified
        assert_ne!(state, original);
        
        // Should be deterministic
        let mut state2 = original;
        keccak_f800(&mut state2);
        assert_eq!(state, state2);
    }
    
    #[test]
    fn test_hash32_conversion() {
        let hash = Hash32 {
            words: [0x01234567, 0x89ABCDEF, 0xFEDCBA98, 0x76543210,
                   0x11111111, 0x22222222, 0x33333333, 0x44444444],
        };
        
        let bytes = hash.to_bytes();
        let hash2 = Hash32::from_bytes(&bytes);
        
        assert_eq!(hash, hash2);
    }
    
    #[test]
    fn test_hash32_hex() {
        let hash = Hash32 {
            words: [0x12345678, 0, 0, 0, 0, 0, 0, 0],
        };
        
        let hex = hash.to_hex();
        let hash2 = Hash32::from_hex(&hex).unwrap();
        
        assert_eq!(hash, hash2);
    }
    
    #[test]
    fn test_keccak_permutation_properties() {
        // Test that permutation has good diffusion
        let mut state1 = [0u32; 25];
        let mut state2 = [0u32; 25];
        state2[0] = 1; // Single bit difference
        
        keccak_f800(&mut state1);
        keccak_f800(&mut state2);
        
        // Count differing bits
        let mut diff_bits = 0;
        for i in 0..25 {
            diff_bits += (state1[i] ^ state2[i]).count_ones();
        }
        
        // Should have good avalanche effect (>400 bits different out of 800)
        assert!(diff_bits > 400, "Poor diffusion: only {} bits differ", diff_bits);
    }
}