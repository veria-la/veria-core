//! Hashing helpers for the verifier program.
//!
//! The verifier derives every `ProofRecord` PDA from
//! `sha256(proof_bytes || public_inputs)`.  Computing that hash on-chain
//! lets us:
//!
//!   * use the proof hash as the second seed of the PDA — distinct proofs
//!     always land on distinct accounts, so `init` (not `init_if_needed`)
//!     fires automatically on a re-verify attempt;
//!   * store the public-inputs hash separately on the record so downstream
//!     readers do not need to refetch the full inputs blob;
//!   * generate a stable, off-chain reproducible identifier for the
//!     marketplace layer.
//!
//! All hashing routes through `anchor_lang::solana_program::hash`, which
//! delegates to the Solana sha256 syscall on-chain (constant-time, ~3K CUs
//! per hash) and to a regular `sha2` implementation off-chain so the same
//! code path runs in unit tests.

use anchor_lang::solana_program::hash::{hashv, Hash};

/// Returns `sha256(len(proof_bytes) || proof_bytes || public_inputs)` as a
/// `[u8; 32]`.
///
/// This is the canonical "proof hash" that:
///   * forms the second PDA seed (`[b"proof", &proof_hash]`);
///   * is emitted in the `ProofVerified` event;
///   * is reproduced off-chain by the TypeScript SDK before submission so
///     the client can short-circuit a redundant call against an existing
///     PDA.
///
/// We prefix `proof_bytes` with its 4-byte little-endian length so the hash
/// is domain-separating in the `(proof_bytes, public_inputs)` split.
/// Without the length prefix, `hashv` would only see the concatenation and
/// two different split points (`("ab", "cd")` vs. `("abc", "d")`) could
/// collide to the same `proof_hash` — the on-chain handler defends against
/// that collision separately, but baking the separation into the hash makes
/// the SDK-side equality check airtight.
pub fn compute_proof_hash(proof_bytes: &[u8], public_inputs: &[u8]) -> [u8; 32] {
    let len_prefix = (proof_bytes.len() as u32).to_le_bytes();
    let h: Hash = hashv(&[&len_prefix, proof_bytes, public_inputs]);
    h.to_bytes()
}

/// Returns `sha256(public_inputs)`.  Stored verbatim on `ProofRecord`.
pub fn compute_public_inputs_hash(public_inputs: &[u8]) -> [u8; 32] {
    let h: Hash = hashv(&[public_inputs]);
    h.to_bytes()
}

/// Returns `sha256(vk_bytes)`.  Used by `initialize` and `update_vk` to
/// record the active vk without storing the multi-KB key on-chain.
pub fn compute_vk_hash(vk_bytes: &[u8]) -> [u8; 32] {
    let h: Hash = hashv(&[vk_bytes]);
    h.to_bytes()
}

/// Returns the first 8 bytes of `sha256(cluster_label)`.
///
/// The cluster prefix is stored on `VerifierConfig` and must be prepended
/// to every public_inputs blob.  Soundness here is purely against accidental
/// or malicious cross-cluster replay; the prefix is not a secret.
pub fn cluster_prefix_for(label: &[u8]) -> [u8; 8] {
    let full = hashv(&[label]).to_bytes();
    let mut prefix = [0u8; 8];
    prefix.copy_from_slice(&full[..8]);
    prefix
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proof_hash_is_deterministic() {
        let a = compute_proof_hash(b"proof-bytes", b"public-inputs");
        let b = compute_proof_hash(b"proof-bytes", b"public-inputs");
        assert_eq!(a, b);
    }

    #[test]
    fn proof_hash_distinguishes_payload() {
        let a = compute_proof_hash(b"proof-bytes", b"public-inputs-a");
        let b = compute_proof_hash(b"proof-bytes", b"public-inputs-b");
        assert_ne!(a, b);
    }

    #[test]
    fn proof_hash_distinguishes_proof_vs_inputs_split() {
        // Domain separation: the 4-byte little-endian length prefix on
        // `proof_bytes` means `("ab", "cd")` and `("abc", "d")` hash to
        // different values even though their concatenation matches.
        let a = compute_proof_hash(b"ab", b"cd");
        let b = compute_proof_hash(b"abc", b"d");
        assert_ne!(a, b);
    }

    #[test]
    fn cluster_prefix_is_stable_across_calls() {
        let p1 = cluster_prefix_for(b"solana-mainnet-beta");
        let p2 = cluster_prefix_for(b"solana-mainnet-beta");
        assert_eq!(p1, p2);
    }

    #[test]
    fn cluster_prefix_distinguishes_labels() {
        let mainnet = cluster_prefix_for(b"solana-mainnet-beta");
        let devnet = cluster_prefix_for(b"solana-devnet");
        assert_ne!(mainnet, devnet);
    }

    #[test]
    fn vk_hash_is_full_32_bytes() {
        let h = compute_vk_hash(b"placeholder-verification-key");
        assert_eq!(h.len(), 32);
    }
}
