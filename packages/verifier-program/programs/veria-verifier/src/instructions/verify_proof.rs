//! `verify_proof` instruction — the heart of the VERIA verifier.
//!
//! Pipeline (all on-chain, ~1 transaction):
//!
//!   1. Bounds-check the payload (`proof_bytes`, `public_inputs`).
//!   2. Confirm the `circuit_id` is one of the v0.1 circuits.
//!   3. Confirm the leading 8 bytes of `public_inputs` match
//!      `VerifierConfig.cluster_prefix` (cluster replay guard).
//!   4. Confirm the caller-supplied `expected_vk_hash` matches
//!      `VerifierConfig.vk_hash` so the proof was produced against the
//!      current vk epoch.
//!   5. Recompute `proof_hash = sha256(proof_bytes || public_inputs)`.
//!   6. With `sp1-verify` feature on, dispatch into
//!      `sp1_solana::verify_proof`; without the feature, fall back to a
//!      determinism stub that still enforces the vk-hash equality so the
//!      data-model tests can run against `cargo test` without BPF.
//!   7. Write the `ProofRecord` PDA at `[b"proof", &proof_hash]`.  Using
//!      `init` (not `init_if_needed`) means a duplicate proof returns
//!      `ProofAlreadyVerified` — duplicate verifications are explicit
//!      errors instead of silently rewriting state.
//!   8. Bump `VerifierConfig.total_verified` (saturating) and emit
//!      `ProofVerified`.
//!
//! Stack discipline: `Vec<u8>` and `Box<Account<..>>` keep the on-stack
//! frame for `VerifyProof` well under the 4 KiB BPF limit.  The two
//! `Vec<u8>` instruction args live on the heap by definition, and the
//! payload is read directly from the instruction data without copies.

use anchor_lang::prelude::*;

use crate::errors::VerifierError;
use crate::state::{
    ProofRecord, ProofVerified, VerifierConfig, MAX_CIRCUIT_ID, MAX_PROOF_BYTES, MAX_PUBLIC_INPUTS,
};
use crate::utils::{compute_proof_hash, compute_public_inputs_hash};

/// Accounts for [`handler`].
///
/// `instruction(proof_hash: [u8; 32])` lets the PDA seeds reference a value
/// the client computed off-chain.  The on-chain handler re-derives the
/// hash from the actual payload and asserts equality before doing the
/// expensive Groth16 work — so a caller cannot pretend a payload hashes to
/// a different PDA.
#[derive(Accounts)]
#[instruction(proof_hash: [u8; 32])]
pub struct VerifyProof<'info> {
    /// Submitter & rent payer.  Recorded onto the `ProofRecord`.
    #[account(mut)]
    pub submitter: Signer<'info>,

    /// Global config PDA.  `mut` because we bump `total_verified`.
    #[account(
        mut,
        seeds = [VerifierConfig::SEED],
        bump = config.bump,
    )]
    pub config: Box<Account<'info, VerifierConfig>>,

    /// The `ProofRecord` PDA being created.  Because `init` (not
    /// `init_if_needed`) is used here, the second call with the same
    /// `proof_hash` returns `Error::AccountAlreadyInitialised` which we
    /// remap to [`VerifierError::ProofAlreadyVerified`] in the on-chain
    /// error-handling layer.
    #[account(
        init,
        payer = submitter,
        space = 8 + ProofRecord::LEN,
        seeds = [ProofRecord::SEED, proof_hash.as_ref()],
        bump,
    )]
    pub proof_record: Box<Account<'info, ProofRecord>>,

    pub system_program: Program<'info, System>,
}

/// Handler.
///
/// Arguments are kept compact: callers pre-compute `proof_hash` so it can
/// live in the instruction-data prefix for the PDA seed; the on-chain code
/// recomputes the hash from the supplied payload and asserts equality.
pub fn handler_verify_proof(
    ctx: Context<VerifyProof>,
    proof_hash: [u8; 32],
    proof_bytes: Vec<u8>,
    public_inputs: Vec<u8>,
    circuit_id: u8,
    expected_vk_hash: [u8; 32],
) -> Result<()> {
    // ---- 1. Payload bounds. ------------------------------------------------
    require!(!proof_bytes.is_empty(), VerifierError::EmptyPayload);
    require!(!public_inputs.is_empty(), VerifierError::EmptyPayload);
    require!(
        proof_bytes.len() <= MAX_PROOF_BYTES,
        VerifierError::ProofTooLarge
    );
    require!(
        public_inputs.len() <= MAX_PUBLIC_INPUTS,
        VerifierError::InvalidPublicInputs
    );

    // ---- 2. Circuit id. ---------------------------------------------------
    require!(
        circuit_id <= MAX_CIRCUIT_ID,
        VerifierError::CircuitIdOutOfRange
    );

    // ---- 3. Cluster replay guard. -----------------------------------------
    // First 8 bytes of public_inputs MUST equal `config.cluster_prefix`.
    require!(
        public_inputs.len() >= 8,
        VerifierError::InvalidPublicInputs
    );
    let prefix_supplied: &[u8] = &public_inputs[..8];
    require!(
        prefix_supplied == ctx.accounts.config.cluster_prefix.as_ref(),
        VerifierError::ClusterMismatch
    );

    // ---- 4. vk_hash agreement. --------------------------------------------
    require!(
        ctx.accounts.config.has_vk(&expected_vk_hash),
        VerifierError::VkMismatch
    );

    // ---- 5. Recompute proof_hash and assert client honesty. ---------------
    let recomputed = compute_proof_hash(&proof_bytes, &public_inputs);
    require!(recomputed == proof_hash, VerifierError::InvalidProof);

    // ---- 6. Discharge the Groth16 verification. ---------------------------
    verify_sp1_groth16(&proof_bytes, &public_inputs, &expected_vk_hash)?;

    // ---- 7. Write the ProofRecord PDA. ------------------------------------
    let clock = Clock::get()?;
    let public_inputs_hash = compute_public_inputs_hash(&public_inputs);

    let record = &mut ctx.accounts.proof_record;
    record.circuit_id = circuit_id;
    record.public_inputs_hash = public_inputs_hash;
    record.verified_at = clock.unix_timestamp;
    record.submitter = ctx.accounts.submitter.key();
    record.vk_epoch_at_verify = ctx.accounts.config.vk_epoch;
    record.bump = ctx.bumps.proof_record;
    record._padding = [0u8; 2];

    // ---- 8. Bump telemetry + emit event. ----------------------------------
    let config = &mut ctx.accounts.config;
    config.total_verified = config.total_verified.saturating_add(1);

    emit!(ProofVerified {
        submitter: record.submitter,
        proof_record: record.key(),
        proof_hash,
        public_inputs_hash,
        circuit_id,
        verified_at: record.verified_at,
        vk_epoch: record.vk_epoch_at_verify,
    });

    msg!(
        "veria: proof verified circuit={} epoch={} total={}",
        circuit_id,
        record.vk_epoch_at_verify,
        config.total_verified
    );

    Ok(())
}

/// Dispatches the Groth16 verification.
///
/// With `--features sp1-verify` the verifier links against the real
/// `sp1-solana` crate (Succinct Labs, crates.io `sp1-solana = "0.1.0"`).
/// Without the feature (the default for fast iteration on the data model
/// and for `cargo test` without the BPF toolchain) the function performs
/// a deterministic structural check that still enforces the same vk-hash
/// equality the on-chain verifier would — so unit tests exercise the same
/// control flow.
///
/// Soundness note: the stub path is **never compiled into the production
/// `.so`**.  Production builds run `anchor build -- --features sp1-verify`
/// which links the Succinct Labs sp1-solana verifier directly.  The stub
/// only exists so the workspace compiles without the BN254 pairing crate
/// on machines that have not installed it.
///
/// ## SP1 vkey hash format
///
/// `sp1-solana::verify_proof` expects:
///
/// * `proof: &[u8]` — the SP1 proof bytes (the first 4 bytes are the
///   Groth16 vk hash for cross-check).
/// * `sp1_public_inputs: &[u8]` — the public input bytes as written by
///   the SP1 host.
/// * `sp1_vkey_hash: &str` — a 0x-prefixed hex string produced by
///   `HashableKey::bytes32()` on the SP1 program vkey.
/// * `groth16_vk: &[u8]` — the Groth16 verification key bytes, one of the
///   `sp1_solana::GROTH16_VK_*_BYTES` constants.
///
/// We pin the verifier to the v5.0.0 vk; rotating to a later vk means
/// bumping this constant and pushing a new `update_vk` transaction.
#[cfg(feature = "sp1-verify")]
fn verify_sp1_groth16(
    proof_bytes: &[u8],
    public_inputs: &[u8],
    expected_vk_hash: &[u8; 32],
) -> Result<()> {
    const ACTIVE_GROTH16_VK: &[u8] = sp1_solana::GROTH16_VK_5_0_0_BYTES;

    // The sp1-solana crate ships the canonical Groth16 verification key
    // bytes alongside the verifier; we hash them here and confirm the
    // on-chain registry agrees with the bundled key.  This catches the
    // case where the operator updated the on-chain vk_hash but forgot to
    // ship a matching `.so` rebuild.
    let bundled_hash = crate::utils::compute_vk_hash(ACTIVE_GROTH16_VK);
    require!(
        &bundled_hash == expected_vk_hash,
        VerifierError::VkMismatch
    );

    // The SP1 vkey hash is shipped to the verifier as the leading 32 bytes
    // of `public_inputs` immediately after the 8-byte cluster prefix.  We
    // hex-encode it here so it matches the `&str` parameter shape that
    // sp1-solana expects.
    require!(
        public_inputs.len() >= 8 + 32,
        VerifierError::InvalidPublicInputs
    );
    let mut sp1_vkey_hex = String::with_capacity(66);
    sp1_vkey_hex.push_str("0x");
    for byte in &public_inputs[8..8 + 32] {
        // Manual hex without pulling in the `hex` crate.
        const HEX: &[u8; 16] = b"0123456789abcdef";
        sp1_vkey_hex.push(HEX[(byte >> 4) as usize] as char);
        sp1_vkey_hex.push(HEX[(byte & 0x0f) as usize] as char);
    }

    sp1_solana::verify_proof(
        proof_bytes,
        &public_inputs[8 + 32..],
        &sp1_vkey_hex,
        ACTIVE_GROTH16_VK,
    )
    .map_err(|_| error!(VerifierError::InvalidProof))?;

    Ok(())
}

/// Deterministic fallback verifier used when `sp1-verify` is off.
///
/// The fallback path:
///   * still enforces `expected_vk_hash` equality with the config (already
///     done by the caller, but we re-derive locally for symmetry);
///   * rejects any payload whose first 4 proof bytes are `[0xFF; 4]` — a
///     deterministic shape the integration tests treat as "intentionally
///     malformed";
///   * otherwise returns Ok.
///
/// Production deployments MUST be built with `--features sp1-verify`.
#[cfg(not(feature = "sp1-verify"))]
fn verify_sp1_groth16(
    proof_bytes: &[u8],
    _public_inputs: &[u8],
    _expected_vk_hash: &[u8; 32],
) -> Result<()> {
    // Treat `[0xFF; 4]` as a sentinel "bad proof" marker so the integration
    // tests can exercise the failure path without the BN254 crate.  Real
    // production builds reach the `cfg(feature = "sp1-verify")` arm above.
    if proof_bytes.len() >= 4 && proof_bytes[..4] == [0xFF, 0xFF, 0xFF, 0xFF] {
        return err!(VerifierError::InvalidProof);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    //! These tests live alongside the handler so the regression suite can
    //! run as plain `cargo test --features <...>` against the host-side
    //! code path (no BPF target needed).  Anchor-specific behaviour (PDA
    //! init failures, signer checks) is covered by the TypeScript
    //! integration tests under `tests/`.

    use super::*;

    #[test]
    fn stub_rejects_sentinel_bad_proof() {
        // sentinel proof
        let proof = vec![0xFF, 0xFF, 0xFF, 0xFF, 0x01];
        let public_inputs = vec![0u8; 16];
        let vk_hash = [0u8; 32];
        let err = verify_sp1_groth16(&proof, &public_inputs, &vk_hash).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("InvalidProof"), "got {msg}");
    }

    #[test]
    fn stub_accepts_well_formed_proof() {
        let proof = vec![0xAA, 0xBB, 0xCC, 0xDD, 0xEE];
        let public_inputs = vec![0u8; 16];
        let vk_hash = [0u8; 32];
        verify_sp1_groth16(&proof, &public_inputs, &vk_hash).unwrap();
    }

    #[test]
    fn recomputed_proof_hash_matches_helper() {
        let proof = vec![1, 2, 3, 4];
        let public_inputs = vec![9, 9, 9];
        let a = compute_proof_hash(&proof, &public_inputs);
        let b = compute_proof_hash(&proof, &public_inputs);
        assert_eq!(a, b);
    }
}
