//! Bisection algorithm for finding optimal trade amounts.
//!
//! Given shadow prices, finds the trade amount where the marginal rate
//! equals the target price ratio.

use magnus_shared::Dex;

use super::Quoter;

/// Configuration for bisection search.
#[derive(Debug, Clone)]
pub struct BisectionConfig {
    /// Relative tolerance for convergence (e.g., 0.001 = 0.1%)
    pub rel_tol: f64,
    /// Absolute tolerance in base units
    pub abs_tol: u64,
    /// Maximum iterations before giving up
    pub max_iter: usize,
}

impl Default for BisectionConfig {
    fn default() -> Self {
        Self {
            rel_tol: 0.001,
            abs_tol: 1000, // 1000 base units
            max_iter: 30,
        }
    }
}

/// Result of bisection search.
#[derive(Debug, Clone)]
pub struct BisectionResult {
    pub optimal_amount: u64,
    pub optimal_output: u64,
    pub marginal_rate: f64,
    pub iterations: usize,
    pub converged: bool,
}

/// Find optimal trade amount where marginal rate equals target price.
///
/// Given shadow prices v = [v_in, v_out], we want to find amount where:
///   d(output)/d(input) = v_in / v_out = target_price
///
/// Uses bisection: if marginal rate > target, need more input (move right)
///                 if marginal rate < target, need less input (move left)
pub fn find_optimal_amount(
    _quoter: &mut Quoter,
    _dex: &Dex,
    _target_price: f64, // v_in / v_out
    _max_amount: u64,
    _config: &BisectionConfig,
) -> eyre::Result<BisectionResult> {
    // Stub implementation - will be filled in Phase 2
    todo!("Bisection implementation in Phase 2")
}

#[cfg(test)]
mod tests {
    use super::*;

    // === Unit Tests (no LiteSVM) ===

    #[test]
    fn test_bisection_config_default() {
        let cfg = BisectionConfig::default();
        assert!(cfg.rel_tol > 0.0);
        assert!(cfg.max_iter > 0);
        assert_eq!(cfg.abs_tol, 1000);
    }

    #[test]
    fn test_bisection_result_fields() {
        let result = BisectionResult { optimal_amount: 1000, optimal_output: 950, marginal_rate: 0.95, iterations: 15, converged: true };
        assert!(result.converged);
        assert_eq!(result.iterations, 15);
    }

    // === Integration Tests ===

    #[test]
    #[ignore]
    fn test_bisection_converges_humidifi() {
        // Test bisection on HumidiFi with various target prices
    }

    #[test]
    #[ignore]
    fn test_bisection_higher_price_more_trading() {
        // Higher target_price (input more valuable) => trade more
    }
}
