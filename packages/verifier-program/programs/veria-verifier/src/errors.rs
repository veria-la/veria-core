//! Custom error codes returned by the VERIA verifier program.
//!
//! Anchor maps `#[error_code]` variants onto the standard
//! `ProgramError::Custom(u32)` envelope so they can be surfaced verbatim in
//! transaction logs and IDL.  The numeric offsets are stable across releases
//! — callers (the TypeScript SDK and the CLI) match on the discriminator
//! rather than the human message.

use anchor_lang::prelude::*;

/// All custom errors the verifier can return.
///
/// Numeric codes start at the standard Anchor base offset (`6000`) and grow
/// monotonically.  Do **not** reorder existing variants when adding new ones
/// — append at the end so SDK clients keep working.
#[error_code]
pub enum VerifierError {
    /// `verify_proof` was called with `proof_bytes` that failed Groth16
    /// verification under the registered verification key.  This is the
    /// soundness path — a malicious prover cannot reach this branch except
    /// by breaking SP1 / Groth16 / the underlying curve.
    #[msg("Invalid SP1 proof: groth16 verification failed")]
    InvalidProof,

    /// The serialized public inputs blob did not match the shape expected
    /// by the registered verification key (e.g. wrong length, wrong field
    /// element encoding).
    #[msg("Invalid public inputs: shape or length mismatch")]
    InvalidPublicInputs,

    /// A `ProofRecord` PDA already exists for the supplied
    /// `(proof_bytes, public_inputs)` hash.  The verifier uses plain `init`
    /// (not `init_if_needed`) precisely so this case triggers a hard error
    /// instead of silently overwriting a previously verified record.
    #[msg("Proof already verified: ProofRecord PDA exists for this hash")]
    ProofAlreadyVerified,

    /// `circuit_id` was outside the inclusive range `0..=MAX_CIRCUIT_ID`.
    /// VERIA v0.1 ships five circuits (scoring / aggregation / median /
    /// sort / ml-inference) so the legal values are `0..=4`.
    #[msg("Circuit id out of range: must be 0..=4 for v0.1")]
    CircuitIdOutOfRange,

    /// `update_vk` was called by a signer that is not the registered admin
    /// inside `VerifierConfig`.
    #[msg("Unauthorized admin: signer does not match VerifierConfig.admin")]
    UnauthorizedAdmin,

    /// The verifying key hash supplied by the caller did not match the
    /// value already stored inside `VerifierConfig`.  This guards against
    /// callers shipping the wrong proof artefact for the wrong vk epoch.
    #[msg("Verification key mismatch: vk_hash does not match VerifierConfig")]
    VkMismatch,

    /// Replay across clusters.  Public inputs must start with the 8-byte
    /// `cluster_prefix` from `VerifierConfig` so the same proof cannot be
    /// recycled from devnet onto mainnet (or vice versa).
    #[msg("Cluster prefix mismatch: proof was generated for a different cluster")]
    ClusterMismatch,

    /// The caller supplied an empty proof or public inputs blob.  Most
    /// likely an integration bug.
    #[msg("Empty payload: proof_bytes or public_inputs is zero-length")]
    EmptyPayload,

    /// The `proof_bytes` blob exceeds the configured maximum size.  The
    /// verifier hard-caps the size to avoid pathological compute-unit usage.
    #[msg("Proof too large: exceeds MAX_PROOF_BYTES")]
    ProofTooLarge,
}

impl VerifierError {
    /// Returns a stable string code suitable for SDK error mapping.
    ///
    /// The TypeScript SDK reads this via the IDL's `errors[]` array; the
    /// strings are also surfaced from CLI error output for human readers.
    pub fn code(&self) -> &'static str {
        match self {
            VerifierError::InvalidProof => "invalid_proof",
            VerifierError::InvalidPublicInputs => "invalid_public_inputs",
            VerifierError::ProofAlreadyVerified => "proof_already_verified",
            VerifierError::CircuitIdOutOfRange => "circuit_id_out_of_range",
            VerifierError::UnauthorizedAdmin => "unauthorized_admin",
            VerifierError::VkMismatch => "vk_mismatch",
            VerifierError::ClusterMismatch => "cluster_mismatch",
            VerifierError::EmptyPayload => "empty_payload",
            VerifierError::ProofTooLarge => "proof_too_large",
        }
    }

    /// Returns `true` for errors callers can retry without changing the
    /// payload (e.g. transient compute-budget rejection — currently
    /// always `false` here because every variant is a hard reject).
    pub fn is_retryable(&self) -> bool {
        false
    }

    /// Returns `true` for errors that indicate a vk-rotation race — the
    /// caller probably needs to refresh the vk hash before retrying.
    pub fn is_vk_related(&self) -> bool {
        matches!(self, VerifierError::VkMismatch)
    }
}
