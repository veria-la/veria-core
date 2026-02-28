//! VERIA `scoring` circuit — SP1 guest program.
//!
//! Computes a weighted average over a bounded vector of scores. Inputs are
//! fixed-point with scale 2^32; intermediate accumulators run in u128 to
//! avoid overflow up to the maximum length of 64.
//!
//! References:
//!   * SP1 zkVM (Succinct Labs, 2024) — host/guest split, RISC-V backend.
//!   * Nova folding (Kothapalli, Setty, Tzialla, CRYPTO 2022) — used by the
//!     host to fold multiple `scoring` instances into a single accumulator.
//!
//! Determinism contract: no allocations after `entrypoint!`, no syscalls
//! beyond `sp1_zkvm::io::read` / `commit`, no floats, branches only on the
//! public `count` field (never on score values themselves).

#![no_main]

sp1_zkvm::entrypoint!(main);

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

/// Mirror of `veria_zkvm_host::inputs::SCORING_MAX`. Keep in sync.
pub const SCORING_MAX: usize = 64;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ScoringInput {
    #[serde(with = "BigArray")]
    pub scores: [u64; SCORING_MAX],
    #[serde(with = "BigArray")]
    pub weights: [u64; SCORING_MAX],
    pub count: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScoringOutput {
    pub weighted_avg_fp: u64,
    pub total_weight: u64,
}

/// SP1 entrypoint. Reads a [`ScoringInput`], computes the weighted average,
/// and commits a [`ScoringOutput`] as the public values of this proof.
pub fn main() {
    let input: ScoringInput = sp1_zkvm::io::read::<ScoringInput>();
    let output = compute(&input);
    sp1_zkvm::io::commit(&output);
}

/// Pure computation. Split from [`main`] so unit tests can drive it directly.
///
/// * `count` larger than `SCORING_MAX` is clamped to `SCORING_MAX` rather
///   than panicking — the host rejects oversized inputs before they reach
///   the guest, so this branch is only a defense-in-depth safeguard.
/// * Multiplication is done in `u128` to handle `u64 * u64` without overflow.
/// * Division is floor division; when `total_weight == 0` the result is `0`.
pub fn compute(input: &ScoringInput) -> ScoringOutput {
    let n = clamped_count(input.count);
    let mut weighted_sum: u128 = 0;
    let mut total_weight: u128 = 0;
    let mut i = 0usize;
    while i < n {
        let s = input.scores[i] as u128;
        let w = input.weights[i] as u128;
        weighted_sum = weighted_sum.wrapping_add(s.wrapping_mul(w));
        total_weight = total_weight.wrapping_add(w);
        i += 1;
    }
    let weighted_avg_fp = if total_weight == 0 {
        0u64
    } else {
        (weighted_sum / total_weight) as u64
    };
    ScoringOutput {
        weighted_avg_fp,
        total_weight: total_weight as u64,
    }
}

/// Clamp `count` to `SCORING_MAX`. Public so the test suite can pin this
/// behaviour explicitly.
pub fn clamped_count(count: u32) -> usize {
    let n = count as usize;
    if n > SCORING_MAX {
        SCORING_MAX
    } else {
        n
    }
}
