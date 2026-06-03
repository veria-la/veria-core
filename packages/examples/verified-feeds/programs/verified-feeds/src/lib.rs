//! verified-feeds — a Pyth-style oracle whose every price update is gated by
//! a Nova-folded **median** proof verified on-chain through the VERIA
//! verifier program.
//!
//! Flow of `publish_feed`:
//!
//!   1. The publisher computes a median over N off-chain samples inside the
//!      VERIA `median` circuit (circuit_id = 2) and folds the run into a
//!      single SP1 Groth16 proof.
//!   2. `publish_feed` CPIs into `veria_verifier::cpi::verify_proof`. The
//!      verifier re-derives `sha256(proof_bytes || public_inputs)`, checks the
//!      cluster prefix + vk epoch, discharges the Groth16 pairing check, and
//!      writes a `ProofRecord` PDA. Any failure aborts the whole transaction,
//!      so the price below is *never* written for an unverified proof.
//!   3. Only on success do we copy `price_u64` into the `PriceFeed` PDA and
//!      stamp it with the proof record + clock.
//!
//! The on-chain circuit table (see veria-verifier `state.rs`) is:
//! `0=scoring 1=aggregation 2=median 3=sort 4=ml-inference`.

use anchor_lang::prelude::*;
use veria_verifier::cpi::accounts::VerifyProof;
use veria_verifier::cpi::verify_proof;
use veria_verifier::program::VeriaVerifier;

declare_id!("BgUeCx3tUk1hrKSqfGMzo4WwtUKKphMn3b9Xny2Kdjnb");

/// `median` circuit id in the VERIA v0.1 circuit table.
pub const CIRCUIT_ID_MEDIAN: u8 = 2;

/// Reject observations whose reference slot is older than this many slots
/// (~3 minutes at 400ms/slot) — a basic oracle staleness guard.
pub const MAX_STALENESS_SLOTS: u64 = 450;

#[program]
pub mod verified_feeds {
    use super::*;

    /// Verify a median proof and publish the resulting price.
    ///
    /// `feed_id` seeds the `PriceFeed` PDA so one program instance can host
    /// many independent feeds (e.g. "SOL-USD", "BTC-USD").
    pub fn publish_feed(
        ctx: Context<PublishFeed>,
        feed_id: String,
        proof_hash: [u8; 32],
        proof_bytes: Vec<u8>,
        public_inputs: Vec<u8>,
        circuit_id: u8,
        expected_vk_hash: [u8; 32],
        price_u64: u64,
        prev_slot: u64,
    ) -> Result<()> {
        require!(feed_id.len() <= 32, FeedError::FeedIdTooLong);
        require!(circuit_id == CIRCUIT_ID_MEDIAN, FeedError::WrongCircuit);

        // Staleness guard: the observation slot may not be in the future and
        // may not be older than MAX_STALENESS_SLOTS relative to the cluster.
        let clock = Clock::get()?;
        require!(prev_slot <= clock.slot, FeedError::SlotInFuture);
        require!(
            clock.slot.saturating_sub(prev_slot) <= MAX_STALENESS_SLOTS,
            FeedError::StaleObservation
        );

        // Authority: once a feed exists only its original publisher may
        // update it. `init_if_needed` leaves `publisher` zeroed on the very
        // first call, which we adopt below.
        let feed = &mut ctx.accounts.feed;
        let fresh = feed.publisher == Pubkey::default();
        require!(
            fresh || feed.publisher == ctx.accounts.publisher.key(),
            FeedError::Unauthorized
        );

        // CPI: discharge the median proof through the VERIA verifier. If the
        // proof is invalid this returns Err and the price write never runs.
        let cpi_accounts = VerifyProof {
            submitter: ctx.accounts.publisher.to_account_info(),
            config: ctx.accounts.verifier_config.to_account_info(),
            proof_record: ctx.accounts.proof_record.to_account_info(),
            system_program: ctx.accounts.system_program.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(
            ctx.accounts.verifier_program.to_account_info(),
            cpi_accounts,
        );
        verify_proof(
            cpi_ctx,
            proof_hash,
            proof_bytes,
            public_inputs,
            circuit_id,
            expected_vk_hash,
        )?;

        // Verified — commit the price.
        feed.price = price_u64;
        feed.verified_at = clock.unix_timestamp;
        feed.proof_record = ctx.accounts.proof_record.key();
        feed.publisher = ctx.accounts.publisher.key();
        feed.bump = ctx.bumps.feed;

        msg!(
            "verified-feeds: feed={} price={} slot={} record={}",
            feed_id,
            price_u64,
            clock.slot,
            feed.proof_record
        );
        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(feed_id: String, proof_hash: [u8; 32])]
pub struct PublishFeed<'info> {
    /// Publisher + rent payer. Must sign so the verifier's `init` of the
    /// `ProofRecord` (payer = submitter) is authorized through this CPI.
    #[account(mut)]
    pub publisher: Signer<'info>,

    /// The price feed PDA. `init_if_needed` lets the same feed be updated on
    /// every new verified observation.
    #[account(
        init_if_needed,
        payer = publisher,
        space = 8 + PriceFeed::LEN,
        seeds = [b"feed", feed_id.as_bytes()],
        bump,
    )]
    pub feed: Box<Account<'info, PriceFeed>>,

    /// The VERIA verifier program, bound to its known on-chain id.
    pub verifier_program: Program<'info, VeriaVerifier>,

    /// CHECK: the verifier's global `VerifierConfig` PDA. Validated inside the
    /// verifier via its own `seeds`/`bump` constraints during the CPI.
    #[account(mut)]
    pub verifier_config: UncheckedAccount<'info>,

    /// CHECK: the `ProofRecord` PDA the verifier creates (init) and owns.
    /// Derived client-side as `[b"proof", proof_hash]` under the verifier id.
    #[account(mut)]
    pub proof_record: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

/// On-chain price feed, written only after a verified median proof.
#[account]
pub struct PriceFeed {
    /// Latest verified price (caller-scaled fixed point, e.g. 1e6).
    pub price: u64,
    /// `Clock::unix_timestamp` of the verification.
    pub verified_at: i64,
    /// The `ProofRecord` PDA that backs this price.
    pub proof_record: Pubkey,
    /// Original publisher; only this key may update the feed.
    pub publisher: Pubkey,
    /// PDA bump.
    pub bump: u8,
}

impl PriceFeed {
    /// `8 (price) + 8 (verified_at) + 32 (proof_record) + 32 (publisher) + 1`.
    pub const LEN: usize = 8 + 8 + 32 + 32 + 1;
}

#[error_code]
pub enum FeedError {
    #[msg("feed_id must be <= 32 bytes")]
    FeedIdTooLong,
    #[msg("proof circuit_id is not the median circuit (2)")]
    WrongCircuit,
    #[msg("reference slot is in the future")]
    SlotInFuture,
    #[msg("observation is older than the staleness window")]
    StaleObservation,
    #[msg("only the original publisher may update this feed")]
    Unauthorized,
}
