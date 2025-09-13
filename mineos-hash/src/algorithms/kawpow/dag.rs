/// DAG (Directed Acyclic Graph) generation for KawPow/ProgPoW
/// 
/// The DAG is a large memory structure (~2.5GB for KawPow) that provides
/// ASIC resistance through memory-hard proof of work.

use crate::common::hash_types::Hash256;
use super::keccak::{keccak_f800};
use super::fnv::fnv1a;
use std::sync::Arc;
use tracing::{info, debug};
use rayon::prelude::*;

/// DAG parameters for KawPow
pub const DATASET_BYTES_INIT: u64 = 1 << 30;  // 1 GB
pub const DATASET_BYTES_GROWTH: u64 = 1 << 23; // 8 MB
pub const CACHE_BYTES_INIT: u64 = 1 << 24;    // 16 MB
pub const CACHE_BYTES_GROWTH: u64 = 1 << 17;   // 128 KB
pub const CACHE_MULTIPLIER: u64 = 1024;
pub const EPOCH_LENGTH: u64 = 7500;
pub const MIX_BYTES: usize = 128;
pub const HASH_BYTES: usize = 64;
pub const DATASET_PARENTS: usize = 256;
pub const CACHE_ROUNDS: usize = 3;
pub const ACCESSES: usize = 64;

/// Calculate the size of the cache for a given epoch
pub fn get_cache_size(epoch: u64) -> u64 {
    let size = CACHE_BYTES_INIT + CACHE_BYTES_GROWTH * epoch;
    // Ensure size is a prime number for better distribution
    let mut size = size - HASH_BYTES as u64;
    while !is_prime(size / HASH_BYTES as u64) {
        size -= 2 * HASH_BYTES as u64;
    }
    size
}

/// Calculate the size of the full dataset for a given epoch
pub fn get_dataset_size(epoch: u64) -> u64 {
    let size = DATASET_BYTES_INIT + DATASET_BYTES_GROWTH * epoch;
    // Ensure size is a multiple of MIX_BYTES
    let mut size = size - MIX_BYTES as u64;
    while !is_prime(size / MIX_BYTES as u64) {
        size -= 2 * MIX_BYTES as u64;
    }
    size
}

/// Check if a number is prime (simple implementation)
fn is_prime(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    if n == 2 {
        return true;
    }
    if n % 2 == 0 {
        return false;
    }
    let sqrt_n = (n as f64).sqrt() as u64;
    for i in (3..=sqrt_n).step_by(2) {
        if n % i == 0 {
            return false;
        }
    }
    true
}

/// Generate seed hash for epoch
pub fn get_seedhash(epoch: u64) -> Hash256 {
    let mut seed = Hash256::default();
    for _ in 0..epoch {
        // Use Keccak256 to generate seed (simplified version)
        let mut state = [0u32; 25];
        let bytes = seed.as_bytes();
        for i in 0..8 {
            state[i] = u32::from_le_bytes([
                bytes[i * 4],
                bytes[i * 4 + 1],
                bytes[i * 4 + 2],
                bytes[i * 4 + 3],
            ]);
        }
        keccak_f800(&mut state);
        
        let mut new_seed = [0u8; 32];
        for i in 0..8 {
            let word_bytes = state[i].to_le_bytes();
            new_seed[i * 4..i * 4 + 4].copy_from_slice(&word_bytes);
        }
        seed = Hash256::from_bytes(new_seed);
    }
    seed
}

/// DAG cache structure
pub struct DagCache {
    pub epoch: u64,
    pub cache: Vec<u8>,
    pub cache_size: u64,
}

impl DagCache {
    /// Generate cache for the given epoch
    pub fn new(epoch: u64) -> Self {
        let cache_size = get_cache_size(epoch);
        let seed = get_seedhash(epoch);
        
        println!("Generating DAG cache for epoch {}, size: {} MB", 
              epoch, cache_size / (1024 * 1024));
        
        // Initialize cache with sequential hashes
        let n = cache_size / HASH_BYTES as u64;
        let mut cache = vec![0u8; cache_size as usize];
        
        // First item is the seed hash
        cache[..32].copy_from_slice(seed.as_bytes());
        
        // Sequential hashing for initial cache
        for i in 1..n as usize {
            let prev_hash = &cache[(i - 1) * HASH_BYTES..(i - 1) * HASH_BYTES + 32];
            let mut state = [0u32; 25];
            
            // Load previous hash into state
            for j in 0..8 {
                state[j] = u32::from_le_bytes([
                    prev_hash[j * 4],
                    prev_hash[j * 4 + 1],
                    prev_hash[j * 4 + 2],
                    prev_hash[j * 4 + 3],
                ]);
            }
            
            keccak_f800(&mut state);
            
            // Store result
            for j in 0..8 {
                let bytes = state[j].to_le_bytes();
                cache[i * HASH_BYTES + j * 4..i * HASH_BYTES + j * 4 + 4]
                    .copy_from_slice(&bytes);
            }
        }
        
        // Perform cache rounds
        for _ in 0..CACHE_ROUNDS {
            for i in 0..n as usize {
                let v = i as u32;
                let parent1 = (v ^ 0) % (n as u32);
                let parent2 = (v ^ 1) % (n as u32);
                
                let mut mix = [0u32; 16]; // 64 bytes
                
                // XOR with parents
                for j in 0..16 {
                    let offset1 = (parent1 as usize) * HASH_BYTES + j * 4;
                    let offset2 = (parent2 as usize) * HASH_BYTES + j * 4;
                    
                    let val1 = u32::from_le_bytes([
                        cache[offset1],
                        cache[offset1 + 1],
                        cache[offset1 + 2],
                        cache[offset1 + 3],
                    ]);
                    
                    let val2 = u32::from_le_bytes([
                        cache[offset2],
                        cache[offset2 + 1],
                        cache[offset2 + 2],
                        cache[offset2 + 3],
                    ]);
                    
                    mix[j] = val1 ^ val2;
                }
                
                // Hash the mix
                let mut state = [0u32; 25];
                for j in 0..16 {
                    state[j] = mix[j];
                }
                keccak_f800(&mut state);
                
                // Store back to cache
                for j in 0..16 {
                    let bytes = state[j].to_le_bytes();
                    cache[i * HASH_BYTES + j * 4..i * HASH_BYTES + j * 4 + 4]
                        .copy_from_slice(&bytes);
                }
            }
        }
        
        DagCache {
            epoch,
            cache,
            cache_size,
        }
    }
    
    /// Calculate a single DAG item
    pub fn calc_dataset_item(&self, index: u64) -> Vec<u8> {
        let n = self.cache_size / HASH_BYTES as u64;
        let r = HASH_BYTES / 4; // word size
        
        let mut mix = vec![0u32; r];
        
        // Initialize mix with cache[index % n]
        let cache_index = (index % n) as usize;
        for i in 0..r {
            let offset = cache_index * HASH_BYTES + i * 4;
            mix[i] = u32::from_le_bytes([
                self.cache[offset],
                self.cache[offset + 1],
                self.cache[offset + 2],
                self.cache[offset + 3],
            ]);
        }
        
        mix[0] ^= index as u32;
        
        // Hash to initialize
        let mut state = [0u32; 25];
        for i in 0..16 {
            state[i] = mix[i];
        }
        keccak_f800(&mut state);
        for i in 0..16 {
            mix[i] = state[i];
        }
        
        // Dataset parents
        for j in 0..DATASET_PARENTS {
            let parent = fnv1a(index as u32 ^ j as u32, mix[j % r]) % (n as u32);
            
            // XOR with parent
            for i in 0..r {
                let offset = (parent as usize) * HASH_BYTES + i * 4;
                let parent_val = u32::from_le_bytes([
                    self.cache[offset],
                    self.cache[offset + 1],
                    self.cache[offset + 2],
                    self.cache[offset + 3],
                ]);
                mix[i] = fnv1a(mix[i], parent_val);
            }
        }
        
        // Final hash
        let mut state = [0u32; 25];
        for i in 0..16 {
            state[i] = mix[i];
        }
        keccak_f800(&mut state);
        
        // Convert to bytes
        let mut result = vec![0u8; HASH_BYTES];
        for i in 0..16 {
            let bytes = state[i].to_le_bytes();
            result[i * 4..i * 4 + 4].copy_from_slice(&bytes);
        }
        
        result
    }
}

/// Full DAG structure
pub struct Dag {
    pub epoch: u64,
    pub data: Arc<Vec<u8>>,
    pub size: u64,
    pub is_test: bool, // Flag for test mode with smaller DAG
}

impl Dag {
    /// Generate full DAG from cache
    pub fn from_cache(cache: &DagCache) -> Self {
        let dataset_size = get_dataset_size(cache.epoch);
        let n = dataset_size / HASH_BYTES as u64;
        
        println!("Generating full DAG for epoch {}, size: {} GB", 
              cache.epoch, dataset_size / (1024 * 1024 * 1024));
        
        // Pre-allocate the data
        let mut data = vec![0u8; dataset_size as usize];
        
        // Generate DAG items in parallel
        let chunk_size = 100000u64;
        let num_chunks = (n + chunk_size - 1) / chunk_size;
        
        for chunk_idx in 0..num_chunks {
            let start = chunk_idx * chunk_size;
            let end = ((chunk_idx + 1) * chunk_size).min(n);
            
            if chunk_idx % 10 == 0 {
                println!("Generating DAG chunk {}/{} ({:.1}%)", 
                    chunk_idx, num_chunks, 
                    (chunk_idx as f64 / num_chunks as f64) * 100.0);
            }
            
            // Generate items in parallel for this chunk
            let items: Vec<(u64, Vec<u8>)> = (start..end)
                .into_par_iter()
                .map(|i| (i, cache.calc_dataset_item(i)))
                .collect();
            
            // Copy results to the main data array
            for (i, item) in items {
                let offset = (i as usize) * HASH_BYTES;
                data[offset..offset + HASH_BYTES].copy_from_slice(&item);
            }
        }
        
        info!("DAG generation complete");
        
        Dag {
            epoch: cache.epoch,
            data: Arc::new(data),
            size: dataset_size,
            is_test: false,
        }
    }
    
    /// Create a small test DAG for quick testing (1MB instead of 1GB+)
    pub fn test_dag(epoch: u64) -> Self {
        let test_size = 1024 * 1024; // 1MB for testing
        let mut data = vec![0u8; test_size];
        
        // Fill with pseudo-random data
        for i in 0..test_size / 4 {
            let val = fnv1a(i as u32, epoch as u32);
            data[i * 4..(i + 1) * 4].copy_from_slice(&val.to_le_bytes());
        }
        
        println!("Created test DAG: {} bytes", test_size);
        
        Dag {
            epoch,
            data: Arc::new(data),
            size: test_size as u64,
            is_test: true,
        }
    }
    
    /// Get a dataset item
    pub fn get_item(&self, index: u64) -> &[u8] {
        let actual_index = if self.is_test {
            // For test DAG, wrap around the smaller size
            index % (self.size / HASH_BYTES as u64)
        } else {
            index
        };
        let offset = (actual_index as usize) * HASH_BYTES;
        &self.data[offset..offset + HASH_BYTES]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_size_calculation() {
        let epoch0_size = get_cache_size(0);
        let epoch1_size = get_cache_size(1);
        
        assert!(epoch0_size >= CACHE_BYTES_INIT);
        assert!(epoch1_size > epoch0_size);
        assert_eq!(epoch0_size % HASH_BYTES as u64, 0);
    }
    
    #[test]
    fn test_dataset_size_calculation() {
        let epoch0_size = get_dataset_size(0);
        let epoch1_size = get_dataset_size(1);
        
        assert!(epoch0_size >= DATASET_BYTES_INIT);
        assert!(epoch1_size > epoch0_size);
        assert_eq!(epoch0_size % MIX_BYTES as u64, 0);
    }
    
    #[test]
    fn test_seedhash_generation() {
        let seed0 = get_seedhash(0);
        let seed1 = get_seedhash(1);
        
        // Seeds should be different for different epochs
        assert_ne!(seed0, seed1);
        
        // Seed should be deterministic
        let seed0_again = get_seedhash(0);
        assert_eq!(seed0, seed0_again);
    }
    
    #[test]
    fn test_dag_cache_generation() {
        // Test with small epoch for speed
        let cache = DagCache::new(0);
        
        assert_eq!(cache.epoch, 0);
        assert_eq!(cache.cache.len(), cache.cache_size as usize);
        
        // Cache should not be all zeros
        let non_zero = cache.cache.iter().any(|&b| b != 0);
        assert!(non_zero);
    }
    
    #[test]
    fn test_dataset_item_calculation() {
        let cache = DagCache::new(0);
        let item0 = cache.calc_dataset_item(0);
        let item1 = cache.calc_dataset_item(1);
        
        assert_eq!(item0.len(), HASH_BYTES);
        assert_eq!(item1.len(), HASH_BYTES);
        
        // Different indices should produce different items
        assert_ne!(item0, item1);
    }
}