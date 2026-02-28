//! Shared input/output schemas for the five VERIA circuits.
//!
//! These structs are the source of truth for the wire format consumed by both
//! the host (off-chain prover) and each SP1 guest program.  Guests cannot link
//! against this crate (they are no_std and target RISC-V), so each guest
//! re-declares an identical layout — keep them in sync.
//!
//! All fields are fixed-length arrays so the serialization is constant-size,
//! which is what the SP1 reader expects.
//!
//! ## Fixed-point convention
//!
//! Real numbers are encoded with scale `2^32` for u64 fields (`scoring`,
//! `aggregation`) and scale `2^16` for i32 fields (`ml-inference`).  See
//! `docs/circuits.md`.

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

// =====================================================================
// scoring
// =====================================================================

/// Max scoring vector length. Padded with zero scores / weights.
pub const SCORING_MAX: usize = 64;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ScoringInput {
    #[serde(with = "BigArray")]
    pub scores: [u64; SCORING_MAX],
    /// Weights in fixed-point, scale `2^32`.
    #[serde(with = "BigArray")]
    pub weights: [u64; SCORING_MAX],
    pub count: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScoringOutput {
    /// Weighted average in fixed-point scale `2^32`.
    pub weighted_avg_fp: u64,
    pub total_weight: u64,
}

impl ScoringInput {
    /// Construct from variable-length slices, padding to `SCORING_MAX`.
    /// Returns `None` if the inputs exceed the bound or have mismatched
    /// lengths.
    pub fn new(scores: &[u64], weights: &[u64]) -> Option<Self> {
        if scores.len() != weights.len() || scores.len() > SCORING_MAX {
            return None;
        }
        let mut s = [0u64; SCORING_MAX];
        let mut w = [0u64; SCORING_MAX];
        s[..scores.len()].copy_from_slice(scores);
        w[..weights.len()].copy_from_slice(weights);
        Some(Self {
            scores: s,
            weights: w,
            count: scores.len() as u32,
        })
    }
}

// =====================================================================
// aggregation
// =====================================================================

/// Max aggregation vector length. Mirrors `docs/circuits.md`.
pub const AGG_MAX: usize = 4096;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregationInput {
    /// Variable-length data; the guest receives the count then reads
    /// exactly `count` u64 words.  We model it as `Vec<u64>` in the host
    /// for ergonomics and convert to fixed buffers when handing to SP1.
    pub data: Vec<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct AggregationOutput {
    pub sum_u128: u128,
    pub avg_u64: u64,
    pub min_u64: u64,
    pub max_u64: u64,
    pub count: u32,
}

// =====================================================================
// median
// =====================================================================

pub const MEDIAN_MAX: usize = 256;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MedianInput {
    #[serde(with = "BigArray")]
    pub raw: [u64; MEDIAN_MAX],
    #[serde(with = "BigArray")]
    pub sorted: [u64; MEDIAN_MAX],
    /// `perm[i]` is the index into `raw` such that `sorted[i] == raw[perm[i]]`.
    #[serde(with = "BigArray")]
    pub perm: [u16; MEDIAN_MAX],
    pub count: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct MedianOutput {
    pub median: u64,
    /// 32-byte commitment to the sorted vector so downstream verifiers can
    /// reference it without re-reading the proof.
    pub sorted_commit: [u8; 32],
    pub count: u32,
}

// =====================================================================
// sort
// =====================================================================

pub const SORT_MAX: usize = 256;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SortInput {
    #[serde(with = "BigArray")]
    pub input: [u64; SORT_MAX],
    #[serde(with = "BigArray")]
    pub sorted: [u64; SORT_MAX],
    #[serde(with = "BigArray")]
    pub perm: [u16; SORT_MAX],
    pub count: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct SortOutput {
    /// 32-byte commitment to the sorted permutation.
    pub sorted_commit: [u8; 32],
    /// 32-byte commitment to the original input.
    pub input_commit: [u8; 32],
    pub count: u32,
}

// =====================================================================
// ml-inference
// =====================================================================

pub const ML_IN: usize = 32;
pub const ML_H1: usize = 16;
pub const ML_H2: usize = 8;
pub const ML_OUT: usize = 4;

/// Fixed-point scale used throughout the ml-inference circuit.  All weights,
/// biases, and intermediate accumulators are `i32` with scale `2^16`.
pub const ML_FP_SHIFT: u32 = 16;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MlInput {
    pub features: [i32; ML_IN],
    pub w1: [[i32; ML_IN]; ML_H1],
    pub b1: [i32; ML_H1],
    pub w2: [[i32; ML_H1]; ML_H2],
    pub b2: [i32; ML_H2],
    pub w3: [[i32; ML_H2]; ML_OUT],
    pub b3: [i32; ML_OUT],
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct MlOutput {
    pub logits: [i32; ML_OUT],
    /// Commitment to the model weights so verifiers can pin a model id.
    pub model_commit: [u8; 32],
}

impl Default for MlInput {
    fn default() -> Self {
        Self {
            features: [0; ML_IN],
            w1: [[0; ML_IN]; ML_H1],
            b1: [0; ML_H1],
            w2: [[0; ML_H1]; ML_H2],
            b2: [0; ML_H2],
            w3: [[0; ML_H2]; ML_OUT],
            b3: [0; ML_OUT],
        }
    }
}

// =====================================================================
// shared helpers
// =====================================================================

/// Compute a 32-byte SHA-256 digest over a slice of u64 values, big-endian.
/// Used both by the host (to recompute expected commitments) and by the
/// guest programs (re-implemented there because guests are no_std).
pub fn commit_u64_be(data: &[u64]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    for &v in data {
        h.update(v.to_be_bytes());
    }
    h.finalize().into()
}

/// Commit an i32 slice (used for the ml-inference model commitment).
pub fn commit_i32_be(data: &[i32]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    for &v in data {
        h.update(v.to_be_bytes());
    }
    h.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoring_pad_works() {
        let s = ScoringInput::new(&[1, 2, 3], &[10, 20, 30]).unwrap();
        assert_eq!(s.count, 3);
        assert_eq!(s.scores[0], 1);
        assert_eq!(s.scores[3], 0);
        assert_eq!(s.weights[2], 30);
    }

    #[test]
    fn scoring_rejects_overflow() {
        let big = vec![0u64; SCORING_MAX + 1];
        assert!(ScoringInput::new(&big, &big).is_none());
    }

    #[test]
    fn scoring_rejects_length_mismatch() {
        assert!(ScoringInput::new(&[1, 2], &[10, 20, 30]).is_none());
    }

    #[test]
    fn commit_deterministic() {
        let a = commit_u64_be(&[1, 2, 3]);
        let b = commit_u64_be(&[1, 2, 3]);
        assert_eq!(a, b);
        let c = commit_u64_be(&[1, 2, 4]);
        assert_ne!(a, c);
    }
}
