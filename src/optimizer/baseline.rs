//! Bellman-Ford baseline router with greedy splitting.
//!
//! Provides a simple baseline for comparison with the convex optimizer.

use magnus_shared::Dex;

use super::{Quoter, RouteResult};

/// Bellman-Ford baseline router with greedy splitting.
#[allow(dead_code)] // Fields used in Phase 5 implementation
pub struct BellmanFordRouter<'a> {
    quoter: Quoter<'a>,
    pools: Vec<Dex>,
    split_count: usize,
}

impl<'a> BellmanFordRouter<'a> {
    pub fn new(quoter: Quoter<'a>, pools: Vec<Dex>, split_count: usize) -> Self {
        Self { quoter, pools, split_count }
    }

    /// Route using greedy split strategy.
    pub fn route(&mut self, _input_amount: u64) -> eyre::Result<RouteResult> {
        // Stub implementation - will be filled in Phase 5
        todo!("BellmanFordRouter implementation in Phase 5")
    }

    /// Get reference to internal quoter for inspection.
    pub fn quoter(&self) -> &Quoter<'a> {
        &self.quoter
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_baseline_single_pool() {
        // Should route everything through the one pool
    }

    #[test]
    #[ignore]
    fn test_baseline_greedy_selection() {
        // With 2 pools, should pick better one for each chunk
    }

    #[test]
    #[ignore]
    fn test_baseline_split_count() {
        // Verify correct number of chunks processed
    }
}
