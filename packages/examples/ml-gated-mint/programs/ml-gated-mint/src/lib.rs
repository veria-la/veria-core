//! ml-gated-mint — an NFT mint that succeeds **only** when a Nova-folded
//! `ml-inference` proof, verified on-chain through the VERIA verifier,
//! demonstrates that a fixed MLP classifier assigns the minter's private
//! feature vector to the gate's target class.
//!
//! The proof's public inputs are laid out (after the verifier's 8-byte
//! cluster prefix) as 32 input features followed by 4 classifier logits, each
//! a 32-byte field element. We:
//!
//!   1. CPI into `veria_verifier::cpi::verify_proof` (circuit_id = 4). The
//!      verifier checks the folded MLP execution proof; a prover who lies
//!      about their features cannot produce a satisfying proof.
//!   2. Parse the 4 logits out of the verified public inputs and assert
//!      `argmax(logits) == target_class`. Because `target_class` is an
//!      instruction arg the program pins (not something the prover chooses
//!      post-hoc), a prover cannot mint into an arbitrary class — the verifier
//!      attests the classification itself.
//!
//! On-chain circuit table (veria-verifier `state.rs`):
//! `0=scoring 1=aggregation 2=median 3=sort 4=ml-inference`.

use anchor_lang::prelude::*;
use veria_verifier::cpi::accounts::VerifyProof;
use veria_verifier::cpi::verify_proof;
use veria_verifier::program::VeriaVerifier;

declare_id!("2g9TWn1PHd5uT8k5NfzUqJFdLkfhj3vDoNfRMFZg4VTA");

/// `ml-inference` circuit id in the VERIA v0.1 circuit table.
pub const CIRCUIT_ID_ML: u8 = 4;
/// MLP input dimensionality (features), each a 32-byte public-input element.
pub const NUM_FEATURES: usize = 32;
/// Classifier output dimensionality (logits / classes).
pub const NUM_CLASSES: usize = 4;
/// Bytes the verifier prepends to public inputs (cluster replay guard).
const CLUSTER_PREFIX_LEN: usize = 8;
/// Field element width in the public-inputs blob.
const FE_LEN: usize = 32;

#[program]
pub mod ml_gated_mint {
    use super::*;

    /// Verify an ML-inference proof and mint iff the proven argmax class
    /// equals `target_class`.
    pub fn mint_if_verified(
        ctx: Context<MintIfVerified>,
        collection: Pubkey,
        proof_hash: [u8; 32],
        proof_bytes: Vec<u8>,
        public_inputs: Vec<u8>,
        circuit_id: u8,
        expected_vk_hash: [u8; 32],
        target_class: u8,
        max_supply: u64,
    ) -> Result<()> {
        require!(circuit_id == CIRCUIT_ID_ML, MintError::WrongCircuit);
        require!(
            (target_class as usize) < NUM_CLASSES,
            MintError::BadTargetClass
        );

        // Compute argmax over the 4 logits BEFORE the CPI moves the bytes.
        let winner = argmax_logits(&public_inputs)?;
        require!(winner == target_class, MintError::ClassMismatch);

        // CPI: discharge the ML-inference proof. Invalid → Err → abort.
        let cpi_accounts = VerifyProof {
            submitter: ctx.accounts.minter.to_account_info(),
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

        // Verified + classified — record the mint against the gate's supply.
        let gate = &mut ctx.accounts.gate;
        let fresh = gate.collection == Pubkey::default();
        if fresh {
            gate.collection = collection;
            gate.target_class = target_class;
            gate.max_supply = max_supply;
            gate.bump = ctx.bumps.gate;
        } else {
            require!(gate.collection == collection, MintError::WrongCollection);
            require!(gate.target_class == target_class, MintError::ClassMismatch);
        }
        require!(gate.minted_count < gate.max_supply, MintError::SupplyExhausted);
        gate.minted_count = gate.minted_count.saturating_add(1);

        msg!(
            "ml-gated-mint: collection={} class={} minted={}/{}",
            gate.collection,
            target_class,
            gate.minted_count,
            gate.max_supply
        );
        Ok(())
    }
}

/// Read the 4 trailing logit field elements from `public_inputs` and return
/// the argmax index. Each logit is the low 8 bytes (little-endian, signed) of
/// its 32-byte element — enough dynamic range for fixed-point MLP outputs.
fn argmax_logits(public_inputs: &[u8]) -> Result<u8> {
    let need = CLUSTER_PREFIX_LEN + (NUM_FEATURES + NUM_CLASSES) * FE_LEN;
    require!(public_inputs.len() >= need, MintError::BadPublicInputs);

    let logits_start = CLUSTER_PREFIX_LEN + NUM_FEATURES * FE_LEN;
    let mut best_idx: u8 = 0;
    let mut best_val: i64 = i64::MIN;
    for i in 0..NUM_CLASSES {
        let off = logits_start + i * FE_LEN;
        let mut le = [0u8; 8];
        le.copy_from_slice(&public_inputs[off..off + 8]);
        let val = i64::from_le_bytes(le);
        if val > best_val {
            best_val = val;
            best_idx = i as u8;
        }
    }
    Ok(best_idx)
}

#[derive(Accounts)]
#[instruction(collection: Pubkey, proof_hash: [u8; 32])]
pub struct MintIfVerified<'info> {
    /// Minter + rent payer; must sign so the verifier's `ProofRecord` init
    /// (payer = submitter) is authorized through this CPI.
    #[account(mut)]
    pub minter: Signer<'info>,

    /// One gate per collection. `init_if_needed` bootstraps it on first mint.
    #[account(
        init_if_needed,
        payer = minter,
        space = 8 + MintGate::LEN,
        seeds = [b"gate", collection.as_ref()],
        bump,
    )]
    pub gate: Box<Account<'info, MintGate>>,

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

/// Per-collection mint gate, advanced only on a verified classification.
#[account]
pub struct MintGate {
    /// Collection this gate guards.
    pub collection: Pubkey,
    /// The class a proof must classify into to mint (0..NUM_CLASSES).
    pub target_class: u8,
    /// Number of verified mints so far.
    pub minted_count: u64,
    /// Hard supply cap.
    pub max_supply: u64,
    /// PDA bump.
    pub bump: u8,
}

impl MintGate {
    /// `32 (collection) + 1 (target_class) + 8 (minted_count) + 8 (max_supply) + 1`.
    pub const LEN: usize = 32 + 1 + 8 + 8 + 1;
}

#[error_code]
pub enum MintError {
    #[msg("proof circuit_id is not the ml-inference circuit (4)")]
    WrongCircuit,
    #[msg("target_class is outside the classifier range")]
    BadTargetClass,
    #[msg("public_inputs too short to contain features + logits")]
    BadPublicInputs,
    #[msg("argmax(logits) does not equal target_class")]
    ClassMismatch,
    #[msg("gate is bound to a different collection")]
    WrongCollection,
    #[msg("mint supply exhausted")]
    SupplyExhausted,
}
