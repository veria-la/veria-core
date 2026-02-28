//! VERIA — Solana mainnet SP1 Groth16 verifier program.
//!
//! VERIA is a Solana-native ZK coprocessor: off-chain SP1 zkVM hosts
//! (Succinct Labs, 2024) generate proofs of long computations, fold them
//! with Nova / SuperNova IVC (Kothapalli, Setty, Tzialla, CRYPTO 2022;
//! Kothapalli, Setty, ePrint 2022/1758) into a single recursive proof, and
//! submit the result here for on-chain verification.  Successful proofs
//! land in a deterministic `ProofRecord` PDA keyed by
//! `sha256(proof_bytes || public_inputs)` — verified facts subscribers can
//! read in a Pyth-style marketplace pattern.
//!
//! ## Layout
//!
//! ```text
//! src/
//!  ├── lib.rs                       <- this file: program ID + #[program]
//!  ├── state.rs                     <- VerifierConfig, ProofRecord, events
//!  ├── errors.rs                    <- VerifierError enum
//!  ├── utils/hash.rs                <- proof_hash, vk_hash, cluster_prefix
//!  └── instructions/
//!       ├── initialize.rs
//!       ├── verify_proof.rs         <- the main on-chain entry point
//!       └── update_vk.rs
//! ```
//!
//! ## Trust boundary
//!
//! The verifier is the single on-chain root of trust for VERIA.  Soundness
//! is inherited from:
//!   * SP1 zkVM (Succinct Labs, 2024) — passing proof implies correct
//!     guest execution.
//!   * Nova / SuperNova folding — sound under the SXDH assumption used by
//!     the Pedersen commitments in Nova.
//!   * sp1-solana Groth16 verifier — the BN254 pairing routines linked at
//!     build time when `sp1-verify` feature is enabled.
//!
//! See `docs/security.md` for the full threat model.
//!
//! ## Cluster
//!
//! `Anchor.toml` pins the default provider cluster to mainnet.  Deployment:
//!
//! ```bash
//! cd packages/verifier-program
//! anchor build --features sp1-verify
//! anchor deploy --provider.cluster mainnet \
//!               --provider.wallet ~/.config/solana/id.json
//! anchor idl init --provider.cluster mainnet \
//!     -f target/idl/veria_verifier.json $PROGRAM_ID
//! ```

use anchor_lang::prelude::*;

pub mod errors;
pub mod instructions;
pub mod state;
pub mod utils;

// Anchor's `#[program]` macro emits code that resolves each instruction's
// helper modules (`__client_accounts_<ix>`, `__cpi_client_accounts_<ix>`,
// ...) via `crate::<ix_name>::*`.  Re-exporting the instruction modules
// with `pub use` makes the macro-generated paths resolve cleanly; the
// `Accounts` struct and the renamed `handler_<ix>` functions ride along
// the same glob.
pub use crate::instructions::*;

// Program ID placeholder.
//
// The string below is a syntactically valid base58 pubkey that Anchor
// accepts as a `declare_id!` argument.  Before the first mainnet deploy
// the operator runs `anchor keys list` to print the real program ID
// generated from `target/deploy/veria_verifier-keypair.json` and replaces
// this value (and the matching entries inside `Anchor.toml`).
//
// The string below is derived from `sha256("veria-verifier-program")` so
// it is obviously not a real keypair-produced ID and is grep-friendly when
// rotating to the production value.  Anchor accepts any valid 32-byte
// base58 pubkey here, so this string compiles cleanly; the deploy script
// later overwrites it with the printed `anchor keys list` output.
declare_id!("J5UE1ddXZS48tYbTTMRe8nMjy4x2VgEozi2VN7scYsAY");

/// VERIA verifier program shims.
///
/// Anchor expands `#[program]` into a per-instruction dispatch function
/// that decodes the instruction data and forwards to the module-level
/// `handler` we defined in `crate::instructions::*`.  Keeping the bodies
/// here a single line each makes the IDL output trivial to audit.
#[program]
pub mod veria_verifier {
    use super::*;

    /// Bootstrap the verifier — must be called exactly once per program.
    /// See [`instructions::initialize`] for full semantics.
    pub fn initialize(
        ctx: Context<Initialize>,
        vk_hash: [u8; 32],
        cluster_label: Vec<u8>,
    ) -> Result<()> {
        handler_initialize(ctx, vk_hash, cluster_label)
    }

    /// Verify an SP1 Groth16 proof and write its `ProofRecord` PDA.
    /// See [`instructions::verify_proof`] for the full pipeline.
    pub fn verify_proof(
        ctx: Context<VerifyProof>,
        proof_hash: [u8; 32],
        proof_bytes: Vec<u8>,
        public_inputs: Vec<u8>,
        circuit_id: u8,
        expected_vk_hash: [u8; 32],
    ) -> Result<()> {
        handler_verify_proof(
            ctx,
            proof_hash,
            proof_bytes,
            public_inputs,
            circuit_id,
            expected_vk_hash,
        )
    }

    /// Admin-only: rotate the registered verification key hash.
    /// See [`instructions::update_vk`] for the full semantics.
    pub fn update_vk(ctx: Context<UpdateVk>, new_vk_hash: [u8; 32]) -> Result<()> {
        handler_update_vk(ctx, new_vk_hash)
    }
}

// Re-exports so downstream IDL consumers can resolve every public type from
// the crate root.  Anchor's IDL generator follows public paths to discover
// account / event / error types.
pub use errors::VerifierError;
pub use state::{
    ProofRecord, ProofVerified, VerifierConfig, VerifierInitialized, VkRotated, MAX_CIRCUIT_ID,
    MAX_PROOF_BYTES, MAX_PUBLIC_INPUTS,
};
