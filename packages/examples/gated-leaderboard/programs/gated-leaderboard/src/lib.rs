//! gated-leaderboard — an anti-cheat ranking board whose every update is
//! gated by a Nova-folded **sort** proof verified on-chain through the VERIA
//! verifier program.
//!
//! Why a sort circuit? The `sort` circuit (circuit_id = 3) proves that the
//! published `top_scores` are the descending sort of the season's submitted
//! scores **and** carries a permutation witness establishing that the output
//! multiset equals the input multiset. That permutation argument is what makes
//! the board tamper-proof: a cheater cannot inject an extra entry or drop a
//! rival without breaking the multiset equality, which makes the folded proof
//! unsatisfiable — so `verify_proof` fails and `submit_ranking` aborts.
//!
//! On-chain circuit table (veria-verifier `state.rs`):
//! `0=scoring 1=aggregation 2=median 3=sort 4=ml-inference`.

use anchor_lang::prelude::*;
use veria_verifier::cpi::accounts::VerifyProof;
use veria_verifier::cpi::verify_proof;
use veria_verifier::program::VeriaVerifier;

declare_id!("8gvp9JHFrspxppEB3usTARr5NVsKdE76b9mzt7eyqkav");

/// `sort` circuit id in the VERIA v0.1 circuit table.
pub const CIRCUIT_ID_SORT: u8 = 3;

/// Number of ranks the board tracks.
pub const TOP_N: usize = 10;

#[program]
pub mod gated_leaderboard {
    use super::*;

    /// Verify a sort proof and commit the season's top-N ranking.
    ///
    /// `top_ranks`/`top_scores` are the descending ranking the prover claims;
    /// the sort proof (with its permutation witness) is what binds them to the
    /// real submitted set. They are written only after the CPI verify returns.
    pub fn submit_ranking(
        ctx: Context<SubmitRanking>,
        season_id: u32,
        proof_hash: [u8; 32],
        proof_bytes: Vec<u8>,
        public_inputs: Vec<u8>,
        circuit_id: u8,
        expected_vk_hash: [u8; 32],
        top_ranks: [Pubkey; TOP_N],
        top_scores: [u64; TOP_N],
    ) -> Result<()> {
        require!(circuit_id == CIRCUIT_ID_SORT, BoardError::WrongCircuit);

        // Defensive: a valid sort output is non-increasing. The proof already
        // enforces this, but checking here gives a precise local error instead
        // of an opaque verifier failure for an obviously malformed claim.
        for i in 1..TOP_N {
            require!(
                top_scores[i] <= top_scores[i - 1],
                BoardError::NotDescending
            );
        }

        // CPI: discharge the sort/permutation proof. Invalid → Err → abort.
        let cpi_accounts = VerifyProof {
            submitter: ctx.accounts.submitter.to_account_info(),
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

        // Verified — commit the ranking.
        let clock = Clock::get()?;
        let board = &mut ctx.accounts.leaderboard;
        board.season_id = season_id;
        board.top_ranks = top_ranks;
        board.top_scores = top_scores;
        board.proof_record = ctx.accounts.proof_record.key();
        board.last_update = clock.unix_timestamp;
        board.bump = ctx.bumps.leaderboard;

        msg!(
            "gated-leaderboard: season={} leader={} top={} record={}",
            season_id,
            top_ranks[0],
            top_scores[0],
            board.proof_record
        );
        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(season_id: u32, proof_hash: [u8; 32])]
pub struct SubmitRanking<'info> {
    /// Submitter + rent payer; must sign so the verifier's `ProofRecord` init
    /// (payer = submitter) is authorized through this CPI.
    #[account(mut)]
    pub submitter: Signer<'info>,

    /// One leaderboard per season. `init_if_needed` lets the board be replaced
    /// whenever a newer verified ranking is submitted.
    #[account(
        init_if_needed,
        payer = submitter,
        space = 8 + Leaderboard::LEN,
        seeds = [b"board", season_id.to_le_bytes().as_ref()],
        bump,
    )]
    pub leaderboard: Box<Account<'info, Leaderboard>>,

    /// The VERIA verifier program, bound to its known on-chain id.
    pub verifier_program: Program<'info, VeriaVerifier>,

    /// CHECK: verifier global `VerifierConfig` PDA; validated inside the CPI.
    #[account(mut)]
    pub verifier_config: UncheckedAccount<'info>,

    /// CHECK: `ProofRecord` PDA created (init) and owned by the verifier.
    #[account(mut)]
    pub proof_record: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

/// On-chain leaderboard, written only after a verified sort proof.
#[account]
pub struct Leaderboard {
    /// Season this board covers.
    pub season_id: u32,
    /// Top-N player keys, descending by score.
    pub top_ranks: [Pubkey; TOP_N],
    /// Top-N scores, descending; multiset-bound to submissions by the proof.
    pub top_scores: [u64; TOP_N],
    /// `ProofRecord` PDA backing this ranking.
    pub proof_record: Pubkey,
    /// `Clock::unix_timestamp` of the last verified update.
    pub last_update: i64,
    /// PDA bump.
    pub bump: u8,
}

impl Leaderboard {
    /// `4 + 32*10 + 8*10 + 32 + 8 + 1`.
    pub const LEN: usize = 4 + (32 * TOP_N) + (8 * TOP_N) + 32 + 8 + 1;
}

#[error_code]
pub enum BoardError {
    #[msg("proof circuit_id is not the sort circuit (3)")]
    WrongCircuit,
    #[msg("top_scores must be non-increasing")]
    NotDescending,
}
