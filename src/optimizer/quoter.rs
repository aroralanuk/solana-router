//! Quoter abstraction for getting quotes from black-box Prop AMM pools.
//!
//! The Quoter wraps an Environment and provides quote functionality with proper
//! state reset between quotes to ensure consistent results.

use magnus_router_client::instructions::SwapBuilder;
use magnus_shared::{Dex, Route};
use solana_sdk::{account::Account, pubkey::Pubkey, transaction::Transaction};

use super::types::QuoteResult;
use crate::{Aggregator, ConstructSwap, Environment, Misc, PMMCfg, consts};

/// Quoter wraps Environment to provide quote functionality for black-box pools.
///
/// Key responsibility: reset state between quotes to ensure consistent results.
pub struct Quoter<'a> {
    env: Environment<'a, String>,
    cfg: PMMCfg,

    /// Cached initial state for reset
    pmm_accounts: Vec<(Pubkey, Account)>,

    /// Token configuration
    src_mint: Pubkey,
    dst_mint: Pubkey,
    src_decimals: u8,
    dst_decimals: u8,

    /// Optional aggregator spoofing
    spoof: Option<Aggregator>,

    /// Counters for debugging
    quote_count: usize,
}

impl<'a> Quoter<'a> {
    /// Create a new Quoter with the given environment and pool configuration.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        env: Environment<'a, String>,
        cfg: PMMCfg,
        pmm_accounts: Vec<(Pubkey, Account)>,
        src_mint: Pubkey,
        dst_mint: Pubkey,
        src_decimals: u8,
        dst_decimals: u8,
        spoof: Option<Aggregator>,
    ) -> Self {
        Self { env, cfg, pmm_accounts, src_mint, dst_mint, src_decimals, dst_decimals, spoof, quote_count: 0 }
    }

    /// Get a quote for swapping amount_in through the specified DEX.
    ///
    /// IMPORTANT: This resets pool state before each quote to ensure consistency.
    pub fn quote(&mut self, dex: &Dex, amount_in: u64) -> eyre::Result<QuoteResult> {
        // Edge case: zero input returns zero output
        if amount_in == 0 {
            return Ok(QuoteResult { amount_in: 0, amount_out: 0, compute_units: 0 });
        }

        // 1. Reset state
        self.env.reset_wallet(&self.src_mint, amount_in)?;
        self.env.set_accounts(&self.pmm_accounts)?;

        // 2. Build swap instruction
        let swap_ix = self.build_swap_instruction(dex, amount_in)?;

        // 3. Execute
        let tx = Transaction::new_signed_with_payer(
            &[swap_ix],
            Some(&self.env.wallet_pubkey()),
            &[&self.env.wallet],
            self.env.latest_blockhash(),
        );

        let res = self.env.send_transaction(tx).map_err(|e| eyre::eyre!("transaction failed: {:?}", e))?;

        // 4. Parse result
        let amount_out = self.env.get_event_amount_out(&res);

        self.quote_count += 1;

        Ok(QuoteResult { amount_in, amount_out, compute_units: res.compute_units_consumed })
    }

    /// Get total number of quotes executed (for debugging/benchmarking)
    pub fn quote_count(&self) -> usize {
        self.quote_count
    }

    /// Reset the quote counter
    pub fn reset_quote_count(&mut self) {
        self.quote_count = 0;
    }

    /// Get source decimals
    pub fn src_decimals(&self) -> u8 {
        self.src_decimals
    }

    /// Get destination decimals
    pub fn dst_decimals(&self) -> u8 {
        self.dst_decimals
    }

    /// Get source mint
    pub fn src_mint(&self) -> &Pubkey {
        &self.src_mint
    }

    /// Get destination mint
    pub fn dst_mint(&self) -> &Pubkey {
        &self.dst_mint
    }

    fn build_swap_instruction(&mut self, dex: &Dex, amount_in: u64) -> eyre::Result<solana_sdk::instruction::Instruction> {
        let src_ata = self.env.wallet_ata(&self.src_mint);
        let dst_ata = self.env.wallet_ata(&self.dst_mint);

        // Routes: single route through single DEX with 100% weight
        let routes: Vec<Vec<magnus_router_client::types::Route>> = vec![vec![Route { dexes: vec![*dex], weights: vec![100] }.into()]];

        let mut swap_builder = SwapBuilder::new()
            .payer(self.env.wallet_pubkey())
            .source_token_account(src_ata)
            .destination_token_account(dst_ata)
            .source_mint(self.src_mint)
            .destination_mint(self.dst_mint)
            .amount_in(amount_in)
            .expect_amount_out(1)
            .min_return(1)
            .amounts(vec![amount_in])
            .routes(routes)
            .order_id(self.quote_count as u64)
            .clone();

        let mut swap_ix = ConstructSwap {
            cfg: self.cfg.clone(),
            builder: &mut swap_builder,
            payer: self.env.wallet_pubkey(),
            src_ta: src_ata,
            dst_ta: dst_ata,
            src_mint: self.src_mint,
            dst_mint: self.dst_mint,
        }
        .attach_pmm_accs(dex)
        .instruction();

        // Apply aggregator spoofing if configured
        if let Some(aggr) = self.spoof {
            swap_ix.program_id = aggr.program_id();
        }

        Ok(swap_ix)
    }
}

/// Builder for creating a Quoter with a convenient API.
#[allow(dead_code)] // Fields for future JIT support
pub struct QuoterBuilder<'a> {
    programs_path: String,
    accounts_path: String,
    cfg: Option<PMMCfg>,
    pmms: Vec<Dex>,
    mints: Option<&'a [(Pubkey, u8)]>,
    src_mint: Option<Pubkey>,
    dst_mint: Option<Pubkey>,
    src_decimals: Option<u8>,
    dst_decimals: Option<u8>,
    spoof: Option<Aggregator>,
    slot: Option<u64>,
    jit_accounts: bool,
    jit_programs: bool,
}

impl<'a> QuoterBuilder<'a> {
    pub fn new() -> Self {
        Self {
            programs_path: consts::PROGRAMS_PATH.to_string(),
            accounts_path: consts::ACCOUNTS_PATH.to_string(),
            cfg: None,
            pmms: vec![],
            mints: None,
            src_mint: None,
            dst_mint: None,
            src_decimals: None,
            dst_decimals: None,
            spoof: None,
            slot: None,
            jit_accounts: false,
            jit_programs: false,
        }
    }

    pub fn programs_path(mut self, path: &str) -> Self {
        self.programs_path = path.to_string();
        self
    }

    pub fn accounts_path(mut self, path: &str) -> Self {
        self.accounts_path = path.to_string();
        self
    }

    pub fn cfg(mut self, cfg: PMMCfg) -> Self {
        self.cfg = Some(cfg);
        self
    }

    pub fn pmms(mut self, pmms: Vec<Dex>) -> Self {
        self.pmms = pmms;
        self
    }

    pub fn mints(mut self, mints: &'a [(Pubkey, u8)]) -> Self {
        self.mints = Some(mints);
        self
    }

    pub fn src_mint(mut self, mint: Pubkey) -> Self {
        self.src_mint = Some(mint);
        self
    }

    pub fn dst_mint(mut self, mint: Pubkey) -> Self {
        self.dst_mint = Some(mint);
        self
    }

    pub fn src_decimals(mut self, dec: u8) -> Self {
        self.src_decimals = Some(dec);
        self
    }

    pub fn dst_decimals(mut self, dec: u8) -> Self {
        self.dst_decimals = Some(dec);
        self
    }

    pub fn spoof(mut self, spoof: Option<Aggregator>) -> Self {
        self.spoof = spoof;
        self
    }

    pub fn slot(mut self, slot: Option<u64>) -> Self {
        self.slot = slot;
        self
    }

    /// Build the Quoter, loading accounts and programs from disk (static).
    pub fn build(self) -> eyre::Result<Quoter<'a>> {
        let cfg = self.cfg.ok_or_else(|| eyre::eyre!("PMMCfg is required"))?;
        let mints = self.mints.ok_or_else(|| eyre::eyre!("mints are required"))?;
        let src_mint = self.src_mint.ok_or_else(|| eyre::eyre!("src_mint is required"))?;
        let dst_mint = self.dst_mint.ok_or_else(|| eyre::eyre!("dst_mint is required"))?;
        let src_decimals = self.src_decimals.ok_or_else(|| eyre::eyre!("src_decimals is required"))?;
        let dst_decimals = self.dst_decimals.ok_or_else(|| eyre::eyre!("dst_decimals is required"))?;

        // Load accounts from disk
        let (slot, accs_map) = Misc::read_accounts_from_disk(&self.pmms, &self.accounts_path)?;

        // Flatten accounts into a single vec
        let pmm_accounts: Vec<(Pubkey, Account)> = accs_map.into_values().flatten().collect();

        // Create environment (clone paths to transfer ownership)
        let mut env = Environment::new(self.programs_path.clone(), self.accounts_path.clone(), Some(mints), cfg.clone(), slot)?;

        // Load programs
        env.get_and_load_programs(&self.pmms, false, self.spoof, None)?;

        // Load accounts into environment
        env.set_accounts(&pmm_accounts)?;

        // Setup wallet with max amount (will be reset per quote)
        env.setup_wallet(&src_mint, u64::MAX / 2, consts::AIRDROP_AMOUNT)?;

        Ok(Quoter::new(env, cfg, pmm_accounts, src_mint, dst_mint, src_decimals, dst_decimals, self.spoof))
    }
}

impl Default for QuoterBuilder<'_> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // === Unit Tests ===

    #[test]
    fn test_quote_result_fields() {
        let qr = QuoteResult { amount_in: 1000, amount_out: 950, compute_units: 30000 };
        assert_eq!(qr.amount_in, 1000);
        assert_eq!(qr.amount_out, 950);
    }

    #[test]
    fn test_quoter_builder_new() {
        let builder = QuoterBuilder::new();
        assert_eq!(builder.programs_path, consts::PROGRAMS_PATH);
        assert_eq!(builder.accounts_path, consts::ACCOUNTS_PATH);
    }

    #[test]
    fn test_quoter_builder_fluent_api() {
        let builder = QuoterBuilder::new()
            .programs_path("/custom/programs")
            .accounts_path("/custom/accounts")
            .pmms(vec![Dex::HumidiFi, Dex::Tessera])
            .src_decimals(9)
            .dst_decimals(6);

        assert_eq!(builder.programs_path, "/custom/programs");
        assert_eq!(builder.accounts_path, "/custom/accounts");
        assert_eq!(builder.pmms.len(), 2);
        assert_eq!(builder.src_decimals, Some(9));
        assert_eq!(builder.dst_decimals, Some(6));
    }

    // === Integration Tests (require LiteSVM + static account cache) ===
    // Run with: cargo test optimizer::quoter -- --ignored --nocapture

    use solana_sdk::pubkey;

    use crate::{PMMCfg, consts};

    const WSOL: Pubkey = pubkey!("So11111111111111111111111111111111111111112");
    const WSOL_DECIMALS: u8 = 9;
    const USDC: Pubkey = pubkey!("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
    const USDC_DECIMALS: u8 = 6;

    /// Helper: build a Quoter for the given PMMs using static disk accounts.
    fn build_test_quoter(pmms: &[Dex]) -> Quoter<'static> {
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
    #[ignore] // Run with: cargo test optimizer::quoter::tests::test_quoter_single_quote -- --ignored
    fn test_quoter_single_quote() {
        let mut quoter = build_test_quoter(&[Dex::HumidiFi]);

        // 1 SOL = 1_000_000_000 lamports
        let amount_in = 1_000_000_000u64;
        let result = quoter.quote(&Dex::HumidiFi, amount_in).expect("quote failed");

        println!("Quote: {} lamports SOL -> {} micro-USDC", result.amount_in, result.amount_out);
        println!("Rate: {:.6}", result.rate());
        println!("Compute units: {}", result.compute_units);

        assert_eq!(result.amount_in, amount_in);
        assert!(result.amount_out > 0, "expected non-zero output");
        assert!(result.compute_units > 0, "expected non-zero compute units");
        assert_eq!(quoter.quote_count(), 1);
    }

    #[test]
    #[ignore]
    fn test_quoter_state_reset() {
        let mut quoter = build_test_quoter(&[Dex::HumidiFi]);

        let amount_in = 1_000_000_000u64; // 1 SOL

        let result1 = quoter.quote(&Dex::HumidiFi, amount_in).expect("first quote failed");
        let result2 = quoter.quote(&Dex::HumidiFi, amount_in).expect("second quote failed");

        println!("Quote 1: {} -> {}", result1.amount_in, result1.amount_out);
        println!("Quote 2: {} -> {}", result2.amount_in, result2.amount_out);

        // Outputs must be identical since state is reset between quotes
        assert_eq!(result1.amount_out, result2.amount_out, "state reset failed: outputs differ");
        assert_eq!(quoter.quote_count(), 2);
    }

    #[test]
    #[ignore]
    fn test_quoter_different_amounts() {
        let mut quoter = build_test_quoter(&[Dex::HumidiFi]);

        let small = quoter.quote(&Dex::HumidiFi, 100_000_000).expect("small quote failed"); // 0.1 SOL
        let large = quoter.quote(&Dex::HumidiFi, 200_000_000).expect("large quote failed"); // 0.2 SOL

        println!("Small: {} -> {} (rate {:.6})", small.amount_in, small.amount_out, small.rate());
        println!("Large: {} -> {} (rate {:.6})", large.amount_in, large.amount_out, large.rate());

        // 2x input should yield < 2x output due to price impact
        assert!(large.amount_out > small.amount_out, "larger input should produce more output");
        assert!(
            large.amount_out < small.amount_out * 2,
            "2x input should produce < 2x output (price impact), but got {} vs {}",
            large.amount_out,
            small.amount_out * 2
        );
    }

    #[test]
    #[ignore]
    fn test_quoter_zero_amount() {
        let mut quoter = build_test_quoter(&[Dex::HumidiFi]);

        let result = quoter.quote(&Dex::HumidiFi, 0).expect("zero quote failed");

        assert_eq!(result.amount_in, 0);
        assert_eq!(result.amount_out, 0);
        // Zero-amount quotes are short-circuited and don't increment the counter
        assert_eq!(quoter.quote_count(), 0);
    }

    #[test]
    #[ignore]
    fn test_quoter_count_increments() {
        let mut quoter = build_test_quoter(&[Dex::HumidiFi]);

        for i in 1..=5 {
            let amount = (i as u64) * 100_000_000; // 0.1, 0.2, ... 0.5 SOL
            quoter.quote(&Dex::HumidiFi, amount).expect("quote failed");
            assert_eq!(quoter.quote_count(), i);
        }

        quoter.reset_quote_count();
        assert_eq!(quoter.quote_count(), 0);
    }

    #[test]
    #[ignore]
    fn test_quoter_multiple_pmms() {
        let mut quoter = build_test_quoter(&[Dex::HumidiFi, Dex::Tessera]);

        let amount_in = 1_000_000_000u64; // 1 SOL

        let humidifi = quoter.quote(&Dex::HumidiFi, amount_in).expect("humidifi quote failed");
        let tessera = quoter.quote(&Dex::Tessera, amount_in).expect("tessera quote failed");

        println!("HumidiFi: {} -> {} (rate {:.6})", humidifi.amount_in, humidifi.amount_out, humidifi.rate());
        println!("Tessera:  {} -> {} (rate {:.6})", tessera.amount_in, tessera.amount_out, tessera.rate());

        assert!(humidifi.amount_out > 0, "humidifi should produce output");
        assert!(tessera.amount_out > 0, "tessera should produce output");
    }
}
