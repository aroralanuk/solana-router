//! Dual function evaluation for convex optimization.
//!
//! Evaluates g(ν) and ∇g(ν) by solving per-pool bisection subproblems.
//! The L-BFGS-B outer loop (in router.rs) minimizes g(ν) to find optimal
//! shadow prices, from which optimal trades are extracted.

use magnus_shared::Dex;

use super::{
    Quoter,
    bisection::{BisectionConfig, BisectionResult, find_optimal_amount},
    router::DualPool,
};

/// Result of evaluating the dual function at a given price vector.
#[derive(Debug, Clone)]
pub struct DualEvaluation {
    /// Dual function value g(ν) = Σᵢ arbᵢ(νᵢ)
    pub value: f64,
    /// Gradient ∇g(ν): net token flow imbalance per token index.
    /// grad[j] = Σᵢ (outflow_i[j] - inflow_i[j])
    pub gradient: Vec<f64>,
    /// Per-pool subproblem results (dex, bisection result)
    pub subproblem_results: Vec<(Dex, BisectionResult)>,
    /// Total quotes executed during this evaluation
    pub quote_count: usize,
}

/// Evaluate the dual function and gradient at given shadow prices.
///
/// For each pool, solves the per-pool subproblem via bisection:
///   arbᵢ(ν) = max [ν_out · Λ_out - ν_in · Δ_in]  s.t. (Δ, Λ) ∈ Tᵢ
///
/// Returns the dual value g(ν) = Σᵢ arbᵢ(νᵢ) and the gradient
/// ∇g(ν)[j] = net flow of token j across all pools.
pub fn evaluate_dual(
    quoter: &mut Quoter,
    pools: &[DualPool],
    prices: &[f64],
    bisection_config: &BisectionConfig,
) -> eyre::Result<DualEvaluation> {
    let n_tokens = prices.len();
    let mut value = 0.0;
    let mut gradient = vec![0.0; n_tokens];
    let mut subproblem_results = Vec::with_capacity(pools.len());
    let initial_quote_count = quoter.quote_count();

    for pool in pools {
        let price_in = prices[pool.token_in_idx];
        let price_out = prices[pool.token_out_idx];

        // Target price: at optimum, marginal rate = price_in / price_out
        let target_price = if price_out > 0.0 { price_in / price_out } else { f64::INFINITY };

        // Solve subproblem via bisection
        let result = find_optimal_amount(quoter, &pool.dex, target_price, pool.max_amount, bisection_config)?;

        // Arbitrage profit: price_out * amount_out - price_in * amount_in
        let profit = price_out * (result.optimal_output as f64) - price_in * (result.optimal_amount as f64);
        value += profit;

        // Gradient: net token flow from trader's perspective
        // We send tokens in (negative for trader) and receive tokens out (positive)
        gradient[pool.token_in_idx] -= result.optimal_amount as f64;
        gradient[pool.token_out_idx] += result.optimal_output as f64;

        subproblem_results.push((pool.dex, result));
    }

    Ok(DualEvaluation { value, gradient, subproblem_results, quote_count: quoter.quote_count() - initial_quote_count })
}

#[cfg(test)]
mod tests {
    use super::*;

    // === Unit Tests (no LiteSVM) ===

    #[test]
    fn test_dual_evaluation_fields() {
        let eval = DualEvaluation {
            value: 42.0,
            gradient: vec![-100.0, 100.0],
            subproblem_results: vec![(
                Dex::HumidiFi,
                BisectionResult { optimal_amount: 1000, optimal_output: 950, marginal_rate: 0.95, iterations: 5, converged: true },
            )],
            quote_count: 10,
        };
        assert_eq!(eval.value, 42.0);
        assert_eq!(eval.gradient.len(), 2);
        assert_eq!(eval.subproblem_results.len(), 1);
        assert_eq!(eval.quote_count, 10);
    }

    #[test]
    fn test_dual_pool_construction() {
        let pool = DualPool { dex: Dex::HumidiFi, token_in_idx: 0, token_out_idx: 1, max_amount: 1_000_000_000 };
        assert_eq!(pool.token_in_idx, 0);
        assert_eq!(pool.token_out_idx, 1);
    }

    #[test]
    fn test_dual_evaluation_gradient_signs() {
        // Gradient should have negative sign for input token, positive for output
        let eval = DualEvaluation { value: 10.0, gradient: vec![-500.0, 450.0], subproblem_results: vec![], quote_count: 0 };
        assert!(eval.gradient[0] < 0.0, "input token gradient should be negative (tokens flowing out)");
        assert!(eval.gradient[1] > 0.0, "output token gradient should be positive (tokens flowing in)");
    }

    // === Integration Tests (require LiteSVM + static account cache) ===
    // Run with: cargo test optimizer::dual -- --ignored --nocapture

    use solana_sdk::{pubkey, pubkey::Pubkey};

    use crate::{PMMCfg, consts, optimizer::quoter::QuoterBuilder};

    const WSOL: Pubkey = pubkey!("So11111111111111111111111111111111111111112");
    const WSOL_DECIMALS: u8 = 9;
    const USDC: Pubkey = pubkey!("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
    const USDC_DECIMALS: u8 = 6;

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
    fn test_dual_evaluation_single_pool() {
        let mut quoter = build_test_quoter(&[Dex::HumidiFi]);
        let config = BisectionConfig::default();

        let pools = vec![DualPool { dex: Dex::HumidiFi, token_in_idx: 0, token_out_idx: 1, max_amount: 10_000_000_000 }];

        // prices: input=0 (spending it), output=1 (want it)
        let prices = vec![0.0, 1.0];

        let eval = evaluate_dual(&mut quoter, &pools, &prices, &config).expect("dual eval failed");

        println!("Dual value: {:.6}", eval.value);
        println!("Gradient: {:?}", eval.gradient);
        println!("Quotes used: {}", eval.quote_count);

        // With input price=0, all trading is "free" so should trade near max
        assert!(eval.value >= 0.0, "dual value should be non-negative");
        assert!(eval.gradient[0] < 0.0, "input gradient should be negative (tokens sent to pool)");
        assert!(eval.gradient[1] > 0.0, "output gradient should be positive (tokens received)");

        let (dex, ref result) = eval.subproblem_results[0];
        assert_eq!(dex, Dex::HumidiFi);
        assert!(result.optimal_amount > 0, "should trade something");
        assert!(result.optimal_output > 0, "should receive something");
    }

    #[test]
    #[ignore]
    fn test_dual_evaluation_two_pools() {
        let mut quoter = build_test_quoter(&[Dex::HumidiFi, Dex::Tessera]);
        let config = BisectionConfig::default();

        let pools = vec![
            DualPool { dex: Dex::HumidiFi, token_in_idx: 0, token_out_idx: 1, max_amount: 10_000_000_000 },
            DualPool { dex: Dex::Tessera, token_in_idx: 0, token_out_idx: 1, max_amount: 10_000_000_000 },
        ];

        // Moderate prices to get meaningful splits
        let prices = vec![0.5, 1.0];

        let eval = evaluate_dual(&mut quoter, &pools, &prices, &config).expect("dual eval failed");

        println!("Dual value: {:.6}", eval.value);
        println!("Gradient: {:?}", eval.gradient);
        for (dex, result) in &eval.subproblem_results {
            println!(
                "  {:?}: amount={}, output={}, marginal={:.6}",
                dex, result.optimal_amount, result.optimal_output, result.marginal_rate
            );
        }

        assert_eq!(eval.subproblem_results.len(), 2, "should have results for both pools");
        // Combined gradient reflects total flow across both pools
        assert!(eval.gradient[0] < 0.0, "combined input gradient should be negative");
        assert!(eval.gradient[1] > 0.0, "combined output gradient should be positive");
    }

    #[test]
    #[ignore]
    fn test_dual_gradient_at_high_input_price() {
        let mut quoter = build_test_quoter(&[Dex::HumidiFi]);
        let config = BisectionConfig::default();

        let pools = vec![DualPool { dex: Dex::HumidiFi, token_in_idx: 0, token_out_idx: 1, max_amount: 10_000_000_000 }];

        // Very high input price → target_price = price_in/price_out is very high
        // → marginal rate threshold is very high → should trade very little
        let prices = vec![1000.0, 1.0];

        let eval = evaluate_dual(&mut quoter, &pools, &prices, &config).expect("dual eval failed");

        println!("High input price - Dual value: {:.6}", eval.value);
        println!("Gradient: {:?}", eval.gradient);

        let (_, ref result) = eval.subproblem_results[0];
        println!("Optimal amount: {}", result.optimal_amount);

        // With very high input price, not worth trading much
        // (marginal rate from pool unlikely to exceed 1000)
        assert!(result.optimal_amount < 1_000_000_000, "should trade very little with high input price, got {}", result.optimal_amount);
    }

    #[test]
    #[ignore]
    fn test_dual_zero_input_price_trades_max() {
        let mut quoter = build_test_quoter(&[Dex::HumidiFi]);
        let config = BisectionConfig::default();

        let pools = vec![DualPool { dex: Dex::HumidiFi, token_in_idx: 0, token_out_idx: 1, max_amount: 5_000_000_000 }];

        // Input price = 0 → target_price = 0 → trade as much as possible
        let prices = vec![0.0, 1.0];

        let eval = evaluate_dual(&mut quoter, &pools, &prices, &config).expect("dual eval failed");

        let (_, ref result) = eval.subproblem_results[0];
        println!("Zero input price: amount={}, output={}", result.optimal_amount, result.optimal_output);

        // With zero input price, should trade near the max
        assert!(result.optimal_amount > 2_500_000_000, "should trade most of max with zero input price, got {}", result.optimal_amount);
    }
}
