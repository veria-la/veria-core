//! On-chain state for the VERIA verifier program.
//!
//! Two account types live here:
//!
//! * [`VerifierConfig`] — singleton, PDA-derived from `[b"config"]`.  Holds
//!   the admin key, the registered Groth16 verification key hash, the
//!   cluster prefix (used to prevent cross-cluster proof replay), and a
//!   running counter for telemetry.
//!
//! * [`ProofRecord`] — one per verified proof.  PDA-derived from
//!   `[b"proof", &proof_hash]` where `proof_hash` is the SHA-256 of
//!   `proof_bytes || public_inputs` (see `crate::utils::hash`).  Because the
//!   seed is the hash of the full payload, every distinct proof writes to a
//!   distinct PDA — Sealevel parallelism is preserved.
//!
//! Account discriminators are produced by Anchor automatically using
//! `sha256("account:<TypeName>")[..8]`.  Account sizes are encoded as
//! `pub const LEN: usize` so each `init` macro can compute
//! `space = 8 + Self::LEN` without magic numbers.

use anchor_lang::prelude::*;

/// Highest legal `circuit_id` for VERIA v0.1.  See `docs/circuits.md`:
///
/// | id | circuit       |
/// |----|---------------|
/// |  0 | scoring       |
/// |  1 | aggregation   |
/// |  2 | median        |
/// |  3 | sort          |
/// |  4 | ml-inference  |
pub const MAX_CIRCUIT_ID: u8 = 4;

/// Hard cap on the on-chain proof bytes the verifier will accept.
///
/// SP1 Groth16 proofs are ~260 bytes after compression but the public
/// inputs blob can grow with the witness; we leave plenty of headroom while
/// still guaranteeing the transaction fits within Solana's 1232-byte packet
/// limit (the actual payload is sent via a separate compute-unit budget
/// instruction in practice).
pub const MAX_PROOF_BYTES: usize = 1024;

/// Hard cap on the public inputs blob.
pub const MAX_PUBLIC_INPUTS: usize = 4096;

/// Singleton configuration PDA for the verifier.
///
/// PDA seeds: `[b"config"]`.
#[account]
pub struct VerifierConfig {
    /// Authority that may rotate the verification key via
    /// [`crate::instructions::update_vk`].  Set at `initialize` and
    /// transferable only via a future `transfer_admin` instruction (not in
    /// v0.1 — admin is treated as a hardware key held by the protocol).
    pub admin: Pubkey,

    /// SHA-256 hash of the active Groth16 verification key.  The on-chain
    /// verifier compares this against the hash recomputed from the vk
    /// bundled inside `sp1-solana` so a vk rotation requires an explicit
    /// `update_vk` transaction.
    pub vk_hash: [u8; 32],

    /// 8-byte cluster prefix that callers must prepend to their public
    /// inputs.  Prevents proofs generated for one cluster from being
    /// replayed onto another.  Set at `initialize`:
    ///
    /// * mainnet: first 8 bytes of `sha256("solana-mainnet-beta")`
    /// * devnet:  first 8 bytes of `sha256("solana-devnet")`
    /// * test:    first 8 bytes of `sha256("solana-test-validator")`
    pub cluster_prefix: [u8; 8],

    /// Total successful verifications since `initialize`.  Saturating add
    /// (cannot wrap on u64 within human lifetimes but written defensively).
    pub total_verified: u64,

    /// Monotonic vk epoch.  Bumped each time `update_vk` succeeds.  SDK
    /// clients carry the epoch out-of-band so they can drop proofs
    /// generated against an older vk before paying the on-chain compute.
    pub vk_epoch: u32,

    /// PDA bump.  Stored so future instructions can sign without
    /// recomputing via `Pubkey::find_program_address`.
    pub bump: u8,

    /// 3 bytes of explicit padding so the struct is 8-byte aligned (Solana
    /// account memory loads are faster when aligned).
    pub _padding: [u8; 3],
}

impl VerifierConfig {
    /// Account size excluding the 8-byte Anchor discriminator.
    ///
    /// `32 (admin) + 32 (vk_hash) + 8 (cluster_prefix) + 8 (total_verified)
    /// + 4 (vk_epoch) + 1 (bump) + 3 (padding) = 88` bytes.
    pub const LEN: usize = 32 + 32 + 8 + 8 + 4 + 1 + 3;

    /// PDA seed prefix.
    pub const SEED: &'static [u8] = b"config";

    /// Returns `true` if the supplied vk hash matches the stored one.
    pub fn has_vk(&self, hash: &[u8; 32]) -> bool {
        anchor_lang::solana_program::program_memory::sol_memcmp(
            self.vk_hash.as_ref(),
            hash.as_ref(),
            32,
        ) == 0
    }

    /// Returns `true` if `signer` matches the registered admin pubkey.
    pub fn is_admin(&self, signer: &Pubkey) -> bool {
        &self.admin == signer
    }

    /// Returns the on-chain cluster prefix used to defeat cross-cluster
    /// proof replay.  Callers can copy these 8 bytes into the head of any
    /// public_inputs blob they intend to submit.
    pub fn cluster_prefix_bytes(&self) -> &[u8; 8] {
        &self.cluster_prefix
    }
}

/// Per-proof record written on successful verification.
///
/// PDA seeds: `[b"proof", &proof_hash]` where
/// `proof_hash = sha256(proof_bytes || public_inputs)`.
///
/// The seed is the hash of the full payload (proof + public inputs), not
/// just the proof bytes, so two proofs differing only in their public
/// inputs land on distinct PDAs.  Because the PDA derivation is the only
/// uniqueness key, the `init` (not `init_if_needed`) macro on the
/// `verify_proof` context guarantees no record can be silently overwritten.
#[account]
pub struct ProofRecord {
    /// Which circuit this proof discharges.  Must be `<= MAX_CIRCUIT_ID`.
    pub circuit_id: u8,

    /// Stored for downstream readers who only have the proof hash; lets
    /// them re-derive the seed without re-fetching the full public inputs.
    pub public_inputs_hash: [u8; 32],

    /// `Clock::unix_timestamp` at the moment the proof was verified.
    pub verified_at: i64,

    /// The signer that submitted the proof.  Stored for telemetry &
    /// reputation systems — verification soundness does not depend on the
    /// submitter identity.
    pub submitter: Pubkey,

    /// `vk_epoch` from `VerifierConfig` at verification time.  Lets
    /// downstream consumers tell whether the proof was verified against
    /// the current or a previous verification key.
    pub vk_epoch_at_verify: u32,

    /// PDA bump.
    pub bump: u8,

    /// 2 bytes of explicit padding for 8-byte alignment.
    pub _padding: [u8; 2],
}

impl ProofRecord {
    /// Account size excluding the 8-byte Anchor discriminator.
    ///
    /// `1 (circuit_id) + 32 (public_inputs_hash) + 8 (verified_at) + 32
    /// (submitter) + 4 (vk_epoch_at_verify) + 1 (bump) + 2 (padding)
    /// = 80` bytes.
    pub const LEN: usize = 1 + 32 + 8 + 32 + 4 + 1 + 2;

    /// PDA seed prefix.
    pub const SEED: &'static [u8] = b"proof";

    /// Returns `true` if the record is for the supplied circuit id.
    pub fn is_for_circuit(&self, id: u8) -> bool {
        self.circuit_id == id
    }

    /// Returns the human-readable label for the embedded `circuit_id`.
    ///
    /// Useful for log lines and SDK error messages.  Returns `"unknown"`
    /// for ids outside the v0.1 range — the on-chain handler rejects
    /// those before they can reach a `ProofRecord`, so this branch is
    /// only ever taken if a future verifier reads a record written by an
    /// older program version.
    pub fn circuit_label(&self) -> &'static str {
        match self.circuit_id {
            0 => "scoring",
            1 => "aggregation",
            2 => "median",
            3 => "sort",
            4 => "ml-inference",
            _ => "unknown",
        }
    }

    /// Convenience: was this record verified against a stale vk epoch?
    pub fn is_stale_vs(&self, current_epoch: u32) -> bool {
        self.vk_epoch_at_verify < current_epoch
    }
}

/// Emitted by `verify_proof` after a successful Groth16 check.
///
/// Subscribers (Compute API marketplace, indexers) listen for this event
/// and surface verified results to their consumers.
#[event]
pub struct ProofVerified {
    /// Submitter of the proof (the tx signer).
    pub submitter: Pubkey,
    /// PDA of the freshly written `ProofRecord`.
    pub proof_record: Pubkey,
    /// SHA-256 of the proof payload (also the latter half of the PDA seed).
    pub proof_hash: [u8; 32],
    /// SHA-256 of the public inputs blob (mirror of the field on the PDA).
    pub public_inputs_hash: [u8; 32],
    /// Which circuit was verified.
    pub circuit_id: u8,
    /// Clock at verification time.
    pub verified_at: i64,
    /// vk_epoch the proof was checked against.
    pub vk_epoch: u32,
}

/// Emitted by `initialize` when the global config PDA is created.
#[event]
pub struct VerifierInitialized {
    pub admin: Pubkey,
    pub vk_hash: [u8; 32],
    pub cluster_prefix: [u8; 8],
}

/// Emitted by `update_vk` when the verification key is rotated.
#[event]
pub struct VkRotated {
    pub admin: Pubkey,
    pub old_vk_hash: [u8; 32],
    pub new_vk_hash: [u8; 32],
    pub new_epoch: u32,
}
