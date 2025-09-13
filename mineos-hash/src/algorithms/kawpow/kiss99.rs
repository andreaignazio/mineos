/// KISS99 (Keep It Simple Stupid) random number generator
/// 
/// This is the simplest random generator that passes the TestU01 statistical
/// test suite. It's used in ProgPoW for its minimal instruction count and
/// good statistical properties.

use super::fnv::{fnv1a, FNV_OFFSET_BASIS};

/// KISS99 random number generator state
#[derive(Debug, Clone, Copy)]
pub struct Kiss99State {
    pub z: u32,
    pub w: u32,
    pub jsr: u32,
    pub jcong: u32,
}

impl Kiss99State {
    /// Create a new KISS99 state from a seed
    pub fn new(seed: u64, lane_id: u32) -> Self {
        // Use FNV1a to expand the seed
        let fnv_hash = FNV_OFFSET_BASIS;
        
        Self {
            z: fnv1a(fnv_hash, seed as u32),
            w: fnv1a(fnv_hash, (seed >> 32) as u32),
            jsr: fnv1a(fnv_hash, lane_id),
            jcong: fnv1a(fnv_hash, lane_id.wrapping_add(1)),
        }
    }
    
    /// Generate the next random number
    #[inline(always)]
    pub fn next(&mut self) -> u32 {
        // Linear congruential generator
        self.z = 36969u32.wrapping_mul(self.z & 65535).wrapping_add(self.z >> 16);
        self.w = 18000u32.wrapping_mul(self.w & 65535).wrapping_add(self.w >> 16);
        
        // Xorshift
        self.jsr ^= self.jsr << 17;
        self.jsr ^= self.jsr >> 13;
        self.jsr ^= self.jsr << 5;
        
        // Congruential generator
        self.jcong = 69069u32.wrapping_mul(self.jcong).wrapping_add(1234567);
        
        // Combine all generators
        ((self.z << 16).wrapping_add(self.w)) ^ self.jcong ^ self.jsr
    }
    
    /// Generate multiple random numbers
    pub fn next_n(&mut self, n: usize) -> Vec<u32> {
        (0..n).map(|_| self.next()).collect()
    }
}

/// Fill a buffer with random values using KISS99
pub fn fill_mix(seed: u64, lane_id: u32, mix_size: usize) -> Vec<u32> {
    let mut state = Kiss99State::new(seed, lane_id);
    state.next_n(mix_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kiss99_deterministic() {
        let seed = 0x123456789ABCDEF0u64;
        let lane_id = 5;
        
        let mut state1 = Kiss99State::new(seed, lane_id);
        let mut state2 = Kiss99State::new(seed, lane_id);
        
        // Should produce same sequence
        for _ in 0..100 {
            assert_eq!(state1.next(), state2.next());
        }
    }
    
    #[test]
    fn test_kiss99_different_seeds() {
        let mut state1 = Kiss99State::new(1, 0);
        let mut state2 = Kiss99State::new(2, 0);
        
        let vals1: Vec<u32> = (0..10).map(|_| state1.next()).collect();
        let vals2: Vec<u32> = (0..10).map(|_| state2.next()).collect();
        
        // Different seeds should produce different sequences
        assert_ne!(vals1, vals2);
    }
    
    #[test]
    fn test_kiss99_distribution() {
        let mut state = Kiss99State::new(42, 0);
        let samples = 10000;
        let vals: Vec<u32> = (0..samples).map(|_| state.next()).collect();
        
        // Check that values are reasonably distributed
        let mean = vals.iter().map(|&x| x as f64).sum::<f64>() / samples as f64;
        let expected_mean = (u32::MAX as f64) / 2.0;
        
        // Mean should be close to expected (within 5%)
        let deviation = (mean - expected_mean).abs() / expected_mean;
        assert!(deviation < 0.05, "Mean deviation too high: {}", deviation);
    }
    
    #[test]
    fn test_fill_mix() {
        let mix = fill_mix(0xDEADBEEF, 0, 32);
        assert_eq!(mix.len(), 32);
        
        // All values should be different (high probability)
        let unique_count = mix.iter().collect::<std::collections::HashSet<_>>().len();
        assert!(unique_count > 30); // Allow for rare collisions
    }
}