//! MineOS Hash - Mining algorithm implementations
//!
//! This crate provides optimized implementations of various cryptocurrency
//! mining algorithms, starting with KawPow for Ravencoin.

pub mod common;
pub mod algorithms;
pub mod stratum;

#[cfg(feature = "cuda")]
pub mod cuda;

// Re-export main types
pub use common::hash_types::{Hash256, BlockHeader, MiningResult, Difficulty};
pub use algorithms::kawpow::{KawPowMiner, KawPowGpuMiner};

#[cfg(feature = "cuda")]
pub use cuda::{KawPowCudaMiner, KawPowCudaMinerOptimized};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_library_exports() {
        // Test that main types are accessible
        let _hash = Hash256::default();
        let _header = BlockHeader::test_header(0);
        let _miner = KawPowMiner::new(1);
    }
}
