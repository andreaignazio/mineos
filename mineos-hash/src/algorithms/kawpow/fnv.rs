/// FNV1a hash implementation for ProgPoW/KawPow
/// 
/// FNV1a provides better distribution properties than FNV1 used in Ethash.
/// This implementation uses 32-bit variant for GPU efficiency.

/// FNV1a constants for 32-bit variant
pub const FNV_PRIME: u32 = 0x0100_0193;
pub const FNV_OFFSET_BASIS: u32 = 0x811c_9dc5;

/// Compute FNV1a hash of two 32-bit values
#[inline(always)]
pub fn fnv1a(h: u32, d: u32) -> u32 {
    (h ^ d).wrapping_mul(FNV_PRIME)
}

/// Compute FNV1a hash of a byte slice
#[inline]
pub fn fnv1a_bytes(data: &[u8]) -> u32 {
    let mut hash = FNV_OFFSET_BASIS;
    for &byte in data {
        hash = fnv1a(hash, byte as u32);
    }
    hash
}

/// Compute FNV1a hash of multiple u32 values
#[inline]
pub fn fnv1a_u32s(values: &[u32]) -> u32 {
    let mut hash = FNV_OFFSET_BASIS;
    for &val in values {
        hash = fnv1a(hash, val);
    }
    hash
}

/// Mix two values using FNV1a
#[inline(always)]
pub fn fnv1a_mix(a: u32, b: u32) -> u32 {
    fnv1a(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fnv1a_basic() {
        // Test with known values
        let h = FNV_OFFSET_BASIS;
        let d = 0x12345678;
        let result = fnv1a(h, d);
        
        // Verify the operation
        assert_eq!(result, (h ^ d).wrapping_mul(FNV_PRIME));
    }

    #[test]
    fn test_fnv1a_bytes() {
        // Test with "hello"
        let data = b"hello";
        let hash = fnv1a_bytes(data);
        
        // The hash should be deterministic
        let hash2 = fnv1a_bytes(data);
        assert_eq!(hash, hash2);
        
        // Different data should produce different hash
        let hash3 = fnv1a_bytes(b"world");
        assert_ne!(hash, hash3);
    }

    #[test]
    fn test_fnv1a_distribution() {
        // Test that small changes in input create large changes in output
        let base = 0x12345678u32;
        let hash1 = fnv1a(FNV_OFFSET_BASIS, base);
        let hash2 = fnv1a(FNV_OFFSET_BASIS, base + 1);
        
        // The hashes should be very different
        let diff = (hash1 ^ hash2).count_ones();
        assert!(diff > 8); // At least 8 bits should differ
    }
}