//! Shared helpers used across the verifier instructions.
//!
//! Everything here is `pub` so the IDL-build pass and the off-chain test
//! harness can reach the same code path that runs in BPF.

pub mod hash;

pub use hash::{
    cluster_prefix_for, compute_proof_hash, compute_public_inputs_hash, compute_vk_hash,
};
