//! Core types for the convex optimization router.

use magnus_shared::Dex;
use solana_sdk::pubkey::Pubkey;

/// A trade on a single pool.
#[derive(Debug, Clone)]
pub struct Trade {
    pub dex: Dex,
    pub amount_in: u64,
    pub amount_out: u64,
}

impl Trade {
    /// Returns the effective exchange rate (amount_out / amount_in).
    pub fn rate(&self) -> f64 {
        if self.amount_in == 0 { 0.0 } else { self.amount_out as f64 / self.amount_in as f64 }
    }
}

/// Result of routing optimization.
#[derive(Debug, Clone)]
pub struct RouteResult {
    pub trades: Vec<Trade>,
    pub total_in: u64,
    pub total_out: u64,
    pub iterations: usize,
    pub quote_count: usize,
    pub time_ms: f64,
}

impl RouteResult {
    /// Returns the effective exchange rate for the entire route.
    pub fn rate(&self) -> f64 {
        if self.total_in == 0 { 0.0 } else { self.total_out as f64 / self.total_in as f64 }
    }

    /// Returns per-pool percentages of total input.
    pub fn percentages(&self) -> Vec<(Dex, f64)> {
        if self.total_in == 0 {
            return self.trades.iter().map(|t| (t.dex, 0.0)).collect();
        }
        self.trades.iter().map(|t| (t.dex, t.amount_in as f64 / self.total_in as f64 * 100.0)).collect()
    }
}

/// Configuration for a pool in the router.
#[derive(Debug, Clone)]
pub struct PoolConfig {
    pub dex: Dex,
    pub token_in: Pubkey,
    pub token_out: Pubkey,
    pub max_amount: u64,
}

/// Result of a single quote.
#[derive(Debug, Clone, Copy)]
pub struct QuoteResult {
    pub amount_in: u64,
    pub amount_out: u64,
    pub compute_units: u64,
}

impl QuoteResult {
    /// Returns the effective exchange rate.
    pub fn rate(&self) -> f64 {
        if self.amount_in == 0 { 0.0 } else { self.amount_out as f64 / self.amount_in as f64 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quote_result_fields() {
        let qr = QuoteResult { amount_in: 1000, amount_out: 950, compute_units: 30000 };
        assert_eq!(qr.amount_in, 1000);
        assert_eq!(qr.amount_out, 950);
        assert_eq!(qr.compute_units, 30000);
    }

    #[test]
    fn test_quote_result_rate() {
        let qr = QuoteResult { amount_in: 1000, amount_out: 500, compute_units: 0 };
        assert!((qr.rate() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_quote_result_rate_zero_input() {
        let qr = QuoteResult { amount_in: 0, amount_out: 100, compute_units: 0 };
        assert_eq!(qr.rate(), 0.0);
    }

    #[test]
    fn test_trade_fields() {
        let trade = Trade { dex: Dex::HumidiFi, amount_in: 1000, amount_out: 950 };
        assert_eq!(trade.amount_in, 1000);
        assert_eq!(trade.amount_out, 950);
    }

    #[test]
    fn test_trade_rate() {
        let trade = Trade { dex: Dex::HumidiFi, amount_in: 1000, amount_out: 500 };
        assert!((trade.rate() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_route_result_rate() {
        let result = RouteResult {
            trades: vec![Trade { dex: Dex::HumidiFi, amount_in: 500, amount_out: 250 }],
            total_in: 1000,
            total_out: 500,
            iterations: 10,
            quote_count: 50,
            time_ms: 100.0,
        };
        assert!((result.rate() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_route_result_percentages() {
        let result = RouteResult {
            trades: vec![
                Trade { dex: Dex::HumidiFi, amount_in: 600, amount_out: 300 },
                Trade { dex: Dex::Tessera, amount_in: 400, amount_out: 200 },
            ],
            total_in: 1000,
            total_out: 500,
            iterations: 10,
            quote_count: 50,
            time_ms: 100.0,
        };
        let pcts = result.percentages();
        assert_eq!(pcts.len(), 2);
        assert!((pcts[0].1 - 60.0).abs() < f64::EPSILON);
        assert!((pcts[1].1 - 40.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_pool_config_construction() {
        let cfg =
            PoolConfig { dex: Dex::HumidiFi, token_in: Pubkey::new_unique(), token_out: Pubkey::new_unique(), max_amount: 1_000_000_000 };
        assert_eq!(cfg.max_amount, 1_000_000_000);
    }
}
