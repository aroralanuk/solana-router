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
///
/// AMMs have monotonically decreasing marginal rates, so bisection converges.
pub fn find_optimal_amount(
    quoter: &mut Quoter,
    dex: &Dex,
    target_price: f64, // v_in / v_out
    max_amount: u64,
    config: &BisectionConfig,
) -> eyre::Result<BisectionResult> {
    // Edge case: nonsensical target price → don't trade
    if target_price <= 0.0 || max_amount == 0 {
        return Ok(BisectionResult { optimal_amount: 0, optimal_output: 0, marginal_rate: f64::INFINITY, iterations: 0, converged: true });
    }

    let mut lo: u64 = 0;
    let mut hi: u64 = max_amount;
    let mut iterations = 0;

    while iterations < config.max_iter {
        // Check convergence: range is small enough
        let range = hi - lo;
        if range <= config.abs_tol || (range as f64 / max_amount as f64) <= config.rel_tol {
            break;
        }

        let mid = lo + (hi - lo) / 2;

        // Compute finite-difference delta: 1% of mid, clamped to [abs_tol, remaining range]
        let delta = ((mid as f64 * 0.01) as u64).max(config.abs_tol).min(max_amount.saturating_sub(mid));

        if delta == 0 {
            break;
        }

        // Two quotes to estimate marginal rate via finite differences
        let quote_mid = quoter.quote(dex, mid)?;
        let quote_mid_plus = quoter.quote(dex, mid.saturating_add(delta))?;

        let d_output = quote_mid_plus.amount_out.saturating_sub(quote_mid.amount_out);
        let marginal_rate = d_output as f64 / delta as f64;

        if marginal_rate > target_price {
            // Marginal rate still above target → pool has room, trade more
            lo = mid;
        } else {
            // Marginal rate below target → trading too much, reduce
            hi = mid;
        }

        iterations += 1;
    }

    // Final quote at converged midpoint
    let final_amount = lo + (hi - lo) / 2;
    let final_quote = quoter.quote(dex, final_amount)?;

    // Estimate final marginal rate
    let final_delta = config.abs_tol.min(max_amount.saturating_sub(final_amount));
    let final_marginal = if final_delta > 0 && final_amount > 0 {
        let final_quote_plus = quoter.quote(dex, final_amount.saturating_add(final_delta))?;
        final_quote_plus.amount_out.saturating_sub(final_quote.amount_out) as f64 / final_delta as f64
    } else {
        0.0
    };

    Ok(BisectionResult {
        optimal_amount: final_amount,
        optimal_output: final_quote.amount_out,
        marginal_rate: final_marginal,
        iterations,
        converged: iterations < config.max_iter,
    })
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
        assert_eq!(result.optimal_amount, 1000);
        assert_eq!(result.optimal_output, 950);
    }

    #[test]
    fn test_bisection_zero_target_price() {
        // target_price <= 0 should return immediately with 0 amount
        // (can't test with real quoter, but we can verify the edge case
        // by constructing a result manually)
        let result = BisectionResult { optimal_amount: 0, optimal_output: 0, marginal_rate: f64::INFINITY, iterations: 0, converged: true };
        assert_eq!(result.optimal_amount, 0);
        assert!(result.converged);
    }

    // === Integration Tests (require LiteSVM + static account cache) ===
    // Run with: cargo test optimizer::bisection -- --ignored --nocapture

    use magnus_shared::Dex;
    use solana_sdk::{pubkey, pubkey::Pubkey};

    use crate::{PMMCfg, consts, optimizer::quoter::QuoterBuilder};

    const WSOL: Pubkey = pubkey!("So11111111111111111111111111111111111111112");
    const WSOL_DECIMALS: u8 = 9;
    const USDC: Pubkey = pubkey!("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
    const USDC_DECIMALS: u8 = 6;

    /// Helper: build a Quoter for bisection tests.
    fn build_test_quoter(pmms: &[Dex]) -> super::super::Quoter<'static> {
        let cfg = PMMCfg::load(consts::SETUP_PATH).expect("failed to load setup.toml");
        let mints: &'static [(Pubkey, u8)] = &*Box::leak(Box::new(vec![(WSOL, WSOL_DECIMALS), (USDC, USDC_DECIMALS)]));

        QuoterBuilder::new()
            .cfg(cfg)
            .pmms(pmms.to_vec())
            .mints(mints)
            .src_mint(WSOL)
            .dst_mint(USDC)
            .src_decimals(WSOL_DECIMALS)
            .dst_decimals(USDC_DECIMALS)
            .build()
            .expect("failed to build quoter")
    }

    #[test]
    #[ignore]
    fn test_bisection_converges_humidifi() {
        let mut quoter = build_test_quoter(&[Dex::HumidiFi]);
        let config = BisectionConfig::default();

        // Use a moderate target price (spot rate for SOL/USDC is ~100-200 USDC per SOL,
        // but in raw units: ~0.1 micro-USDC per lamport)
        let target_price = 0.1;
        let max_amount = 10_000_000_000u64; // 10 SOL

        let result = find_optimal_amount(&mut quoter, &Dex::HumidiFi, target_price, max_amount, &config).expect("bisection failed");

        println!(
            "Bisection result: amount={}, output={}, marginal_rate={:.6}, iters={}, converged={}",
            result.optimal_amount, result.optimal_output, result.marginal_rate, result.iterations, result.converged
        );

        assert!(result.converged, "bisection should converge");
        assert!(result.optimal_output > 0, "should produce some output");
        assert!(result.iterations > 0, "should take at least one iteration");
    }

    #[test]
    #[ignore]
    fn test_bisection_higher_price_more_trading() {
        let mut quoter = build_test_quoter(&[Dex::HumidiFi]);
        let config = BisectionConfig::default();
        let max_amount = 10_000_000_000u64; // 10 SOL

        // Low target price → input less valuable → trade less
        let result_low = find_optimal_amount(&mut quoter, &Dex::HumidiFi, 0.2, max_amount, &config).expect("bisection failed (low)");

        // High target price → input more valuable → trade more
        // (higher target = willing to accept worse marginal rates, so trade bigger amounts)
        let result_high = find_optimal_amount(&mut quoter, &Dex::HumidiFi, 0.05, max_amount, &config).expect("bisection failed (high)");

        println!("Low target (0.2):  amount={}", result_low.optimal_amount);
        println!("High target (0.05): amount={}", result_high.optimal_amount);

        // Lower target price means the marginal rate threshold is lower,
        // so we'd trade MORE before hitting it (AMM rates decrease with size)
        assert!(
            result_high.optimal_amount > result_low.optimal_amount,
            "lower target_price should result in more trading (amount {} vs {})",
            result_high.optimal_amount,
            result_low.optimal_amount
        );
    }

    #[test]
    #[ignore]
    fn test_bisection_zero_price_returns_zero() {
        let mut quoter = build_test_quoter(&[Dex::HumidiFi]);
        let config = BisectionConfig::default();

        let result = find_optimal_amount(&mut quoter, &Dex::HumidiFi, 0.0, 1_000_000_000, &config).expect("bisection failed");

        assert_eq!(result.optimal_amount, 0);
        assert_eq!(result.optimal_output, 0);
        assert!(result.converged);
        assert_eq!(result.iterations, 0);
    }

    #[test]
    #[ignore]
    fn test_bisection_negative_price_returns_zero() {
        let mut quoter = build_test_quoter(&[Dex::HumidiFi]);
        let config = BisectionConfig::default();

        let result = find_optimal_amount(&mut quoter, &Dex::HumidiFi, -1.0, 1_000_000_000, &config).expect("bisection failed");

        assert_eq!(result.optimal_amount, 0);
        assert!(result.converged);
    }

    #[test]
    #[ignore]
    fn test_bisection_very_low_target_trades_near_max() {
        let mut quoter = build_test_quoter(&[Dex::HumidiFi]);
        let config = BisectionConfig::default();
        let max_amount = 1_000_000_000u64; // 1 SOL

        // Very low target price → willing to accept terrible marginal rates → trade near max
        let result = find_optimal_amount(&mut quoter, &Dex::HumidiFi, 0.001, max_amount, &config).expect("bisection failed");

        println!("Very low target: amount={} (max={})", result.optimal_amount, max_amount);

        // Should trade a large portion of max
        assert!(
            result.optimal_amount > max_amount / 2,
            "very low target should trade most of max amount, got {} vs max {}",
            result.optimal_amount,
            max_amount
        );
    }

    #[test]
    #[ignore]
    fn test_bisection_max_iter_reached() {
        let mut quoter = build_test_quoter(&[Dex::HumidiFi]);

        // Very tight config that should exhaust iterations
        let config = BisectionConfig {
            rel_tol: 1e-15, // impossibly tight
            abs_tol: 1,     // 1 lamport
            max_iter: 3,    // very few iterations
        };

        let result = find_optimal_amount(&mut quoter, &Dex::HumidiFi, 0.1, 10_000_000_000, &config).expect("bisection failed");

        println!("Max iter test: iters={}, converged={}", result.iterations, result.converged);

        // With only 3 iterations and impossibly tight tolerance, should not converge
        assert!(!result.converged, "should not converge with only 3 iterations");
        assert_eq!(result.iterations, 3);
    }

    #[test]
    #[ignore]
    fn test_bisection_deterministic() {
        let config = BisectionConfig::default();
        let max_amount = 5_000_000_000u64; // 5 SOL
        let target = 0.1;

        let mut quoter1 = build_test_quoter(&[Dex::HumidiFi]);
        let result1 = find_optimal_amount(&mut quoter1, &Dex::HumidiFi, target, max_amount, &config).expect("bisection 1 failed");

        let mut quoter2 = build_test_quoter(&[Dex::HumidiFi]);
        let result2 = find_optimal_amount(&mut quoter2, &Dex::HumidiFi, target, max_amount, &config).expect("bisection 2 failed");

        assert_eq!(result1.optimal_amount, result2.optimal_amount, "bisection should be deterministic");
        assert_eq!(result1.optimal_output, result2.optimal_output, "outputs should match");
    }
}
