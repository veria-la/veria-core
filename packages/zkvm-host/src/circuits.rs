//! Circuit registry.
//!
//! The host enumerates the five v0.1.0 circuits and exposes:
//!
//! * a single [`CircuitId`] enum with stable u8 ids — these ids are the same
//!   ones the Anchor verifier uses to dispatch to its on-chain verifier table;
//! * the corresponding embedded ELF bytes (when the SP1 build step has run);
//! * a host-side reference implementation that runs the same computation as
//!   the guest program.  The reference path is what makes the integration
//!   test suite executable without a RISC-V toolchain installed.

use crate::error::{HostError, HostResult};
use crate::inputs::{
    self, AggregationInput, AggregationOutput, MedianInput, MedianOutput, MlInput, MlOutput,
    ScoringInput, ScoringOutput, SortInput, SortOutput,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// All circuits VERIA ships in v0.1.0. The numeric value is the on-chain
/// `circuit_id` consumed by the Anchor verifier program.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum CircuitId {
    Scoring = 1,
    Aggregation = 2,
    Median = 3,
    Sort = 4,
    MlInference = 5,
}

impl CircuitId {
    /// All known circuit ids — useful for the CLI `--list` flag.
    pub const ALL: &'static [CircuitId] = &[
        CircuitId::Scoring,
        CircuitId::Aggregation,
        CircuitId::Median,
        CircuitId::Sort,
        CircuitId::MlInference,
    ];

    /// Stable kebab-case name used in CLI args and JSON payloads.
    pub fn name(self) -> &'static str {
        match self {
            CircuitId::Scoring => "scoring",
            CircuitId::Aggregation => "aggregation",
            CircuitId::Median => "median",
            CircuitId::Sort => "sort",
            CircuitId::MlInference => "ml-inference",
        }
    }

    /// Inverse of [`Self::name`].
    pub fn from_str(s: &str) -> HostResult<Self> {
        match s {
            "scoring" => Ok(CircuitId::Scoring),
            "aggregation" => Ok(CircuitId::Aggregation),
            "median" => Ok(CircuitId::Median),
            "sort" => Ok(CircuitId::Sort),
            "ml-inference" | "ml_inference" => Ok(CircuitId::MlInference),
            other => Err(HostError::UnknownCircuit(other.to_string())),
        }
    }

    /// Whether an SP1 ELF for this circuit has been compiled and embedded
    /// in this host binary.  When `false`, the host falls back to the
    /// deterministic in-process reference implementation.
    pub fn elf_embedded(self) -> bool {
        self.elf().is_some()
    }

    /// Returns the embedded ELF for this circuit if one was produced by the
    /// build script.
    ///
    /// The build script writes the ELFs under each guest crate's `elf/`
    /// directory; we include them with `include_bytes!` only when an
    /// environment flag is set at compile time so the default `cargo check`
    /// path stays toolchain-free.  See `build.rs`.
    pub fn elf(self) -> Option<&'static [u8]> {
        // Hook for future static embedding.  We intentionally return `None`
        // until the toolchain integration lands so that the host falls back
        // to the deterministic reference path — this is what makes the test
        // suite portable.
        None
    }
}

impl core::str::FromStr for CircuitId {
    type Err = HostError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_str(s)
    }
}

/// Run the host-side reference implementation for `scoring`.
///
/// Mirrors `packages/circuits/scoring/program/src/main.rs`.  Computed twice —
/// once here, once inside the guest — and the integration test asserts they
/// agree.
pub fn ref_scoring(input: &ScoringInput) -> HostResult<ScoringOutput> {
    if input.count as usize > inputs::SCORING_MAX {
        return Err(HostError::OutOfBounds {
            got: input.count as usize,
            max: inputs::SCORING_MAX,
        });
    }
    let n = input.count as usize;
    let mut weighted_sum: u128 = 0;
    let mut total_weight: u128 = 0;
    for i in 0..n {
        weighted_sum += (input.scores[i] as u128) * (input.weights[i] as u128);
        total_weight += input.weights[i] as u128;
    }
    let weighted_avg_fp = if total_weight == 0 {
        0
    } else {
        (weighted_sum / total_weight) as u64
    };
    Ok(ScoringOutput {
        weighted_avg_fp,
        total_weight: total_weight as u64,
    })
}

/// Reference implementation of the aggregation circuit.
pub fn ref_aggregation(input: &AggregationInput) -> HostResult<AggregationOutput> {
    if input.data.len() > inputs::AGG_MAX {
        return Err(HostError::OutOfBounds {
            got: input.data.len(),
            max: inputs::AGG_MAX,
        });
    }
    if input.data.is_empty() {
        return Ok(AggregationOutput {
            sum_u128: 0,
            avg_u64: 0,
            min_u64: 0,
            max_u64: 0,
            count: 0,
        });
    }
    let mut sum: u128 = 0;
    let mut lo = u64::MAX;
    let mut hi = u64::MIN;
    for &v in &input.data {
        sum += v as u128;
        if v < lo {
            lo = v;
        }
        if v > hi {
            hi = v;
        }
    }
    let n = input.data.len() as u128;
    let avg = (sum / n) as u64;
    Ok(AggregationOutput {
        sum_u128: sum,
        avg_u64: avg,
        min_u64: lo,
        max_u64: hi,
        count: input.data.len() as u32,
    })
}

/// Reference implementation of the median circuit (with witness check).
pub fn ref_median(input: &MedianInput) -> HostResult<MedianOutput> {
    let n = input.count as usize;
    if n > inputs::MEDIAN_MAX {
        return Err(HostError::OutOfBounds {
            got: n,
            max: inputs::MEDIAN_MAX,
        });
    }
    if n == 0 {
        return Ok(MedianOutput {
            median: 0,
            sorted_commit: [0u8; 32],
            count: 0,
        });
    }
    // Witness check: sorted is monotonic.
    for i in 1..n {
        if input.sorted[i] < input.sorted[i - 1] {
            return Err(HostError::Folding(
                "sorted witness not monotonic".to_string(),
            ));
        }
    }
    // Permutation check: each perm index unique and within bounds, and
    // sorted[i] == raw[perm[i]].
    let mut seen = vec![false; n];
    for i in 0..n {
        let p = input.perm[i] as usize;
        if p >= n || seen[p] {
            return Err(HostError::Folding(format!("invalid perm at {i}: {p}")));
        }
        seen[p] = true;
        if input.sorted[i] != input.raw[p] {
            return Err(HostError::Folding(format!(
                "perm/sorted mismatch at {i}: raw[{p}]={} sorted[{i}]={}",
                input.raw[p], input.sorted[i]
            )));
        }
    }
    let median = if n % 2 == 1 {
        input.sorted[n / 2]
    } else {
        // Even count -> lower median to match guest behaviour (deterministic
        // tie-break; guests cannot do floating-point average).
        input.sorted[n / 2 - 1]
    };
    Ok(MedianOutput {
        median,
        sorted_commit: inputs::commit_u64_be(&input.sorted[..n]),
        count: input.count,
    })
}

/// Reference implementation of the sort circuit.
pub fn ref_sort(input: &SortInput) -> HostResult<SortOutput> {
    let n = input.count as usize;
    if n > inputs::SORT_MAX {
        return Err(HostError::OutOfBounds {
            got: n,
            max: inputs::SORT_MAX,
        });
    }
    // Monotonicity witness.
    for i in 1..n {
        if input.sorted[i] < input.sorted[i - 1] {
            return Err(HostError::Folding(
                "sorted witness not monotonic".to_string(),
            ));
        }
    }
    // Multiset / permutation check using running product hash (a
    // Schwartz-Zippel style PIOP shortcut). We pick a deterministic
    // pseudo-random challenge `r` derived from the input commitment so the
    // check is non-interactive and reproducible.
    let input_commit = inputs::commit_u64_be(&input.input[..n]);
    let sorted_commit = inputs::commit_u64_be(&input.sorted[..n]);
    let r = derive_challenge(&input_commit, &sorted_commit);
    let mut prod_in: u128 = 1;
    let mut prod_out: u128 = 1;
    let r128 = r as u128;
    for i in 0..n {
        // (r + x_i) mod 2^64, then accumulate mod 2^64 too.
        let a = (r128.wrapping_add(input.input[i] as u128)) & u64::MAX as u128;
        let b = (r128.wrapping_add(input.sorted[i] as u128)) & u64::MAX as u128;
        prod_in = (prod_in.wrapping_mul(a)) & u64::MAX as u128;
        prod_out = (prod_out.wrapping_mul(b)) & u64::MAX as u128;
    }
    if prod_in != prod_out {
        return Err(HostError::Folding(
            "multiset mismatch (running product check failed)".to_string(),
        ));
    }
    Ok(SortOutput {
        sorted_commit,
        input_commit,
        count: input.count,
    })
}

/// Reference implementation of the ml-inference circuit. ReLU activation,
/// fixed-point scale 2^16, lower-saturated at zero. Saturates at `i32::MAX`
/// in the positive direction to avoid the panic that the guest would also
/// avoid by using saturating arithmetic.
pub fn ref_ml(input: &MlInput) -> HostResult<MlOutput> {
    // Layer 1.
    let mut h1 = [0i32; inputs::ML_H1];
    for j in 0..inputs::ML_H1 {
        let mut acc: i64 = input.b1[j] as i64;
        for i in 0..inputs::ML_IN {
            let p = (input.features[i] as i64) * (input.w1[j][i] as i64);
            acc = acc.saturating_add(p >> inputs::ML_FP_SHIFT);
        }
        h1[j] = relu_saturating_i64(acc);
    }
    // Layer 2.
    let mut h2 = [0i32; inputs::ML_H2];
    for j in 0..inputs::ML_H2 {
        let mut acc: i64 = input.b2[j] as i64;
        for i in 0..inputs::ML_H1 {
            let p = (h1[i] as i64) * (input.w2[j][i] as i64);
            acc = acc.saturating_add(p >> inputs::ML_FP_SHIFT);
        }
        h2[j] = relu_saturating_i64(acc);
    }
    // Output layer (no ReLU on logits).
    let mut logits = [0i32; inputs::ML_OUT];
    for j in 0..inputs::ML_OUT {
        let mut acc: i64 = input.b3[j] as i64;
        for i in 0..inputs::ML_H2 {
            let p = (h2[i] as i64) * (input.w3[j][i] as i64);
            acc = acc.saturating_add(p >> inputs::ML_FP_SHIFT);
        }
        logits[j] = saturating_cast_i64_to_i32(acc);
    }
    // Commit to the model weights (w1 || b1 || w2 || b2 || w3 || b3).
    let mut h = Sha256::new();
    for row in &input.w1 {
        for &x in row {
            h.update(x.to_be_bytes());
        }
    }
    for &x in &input.b1 {
        h.update(x.to_be_bytes());
    }
    for row in &input.w2 {
        for &x in row {
            h.update(x.to_be_bytes());
        }
    }
    for &x in &input.b2 {
        h.update(x.to_be_bytes());
    }
    for row in &input.w3 {
        for &x in row {
            h.update(x.to_be_bytes());
        }
    }
    for &x in &input.b3 {
        h.update(x.to_be_bytes());
    }
    let model_commit: [u8; 32] = h.finalize().into();
    Ok(MlOutput {
        logits,
        model_commit,
    })
}

fn relu_saturating_i64(acc: i64) -> i32 {
    if acc <= 0 {
        0
    } else if acc > i32::MAX as i64 {
        i32::MAX
    } else {
        acc as i32
    }
}

fn saturating_cast_i64_to_i32(acc: i64) -> i32 {
    if acc > i32::MAX as i64 {
        i32::MAX
    } else if acc < i32::MIN as i64 {
        i32::MIN
    } else {
        acc as i32
    }
}

/// Derive a 64-bit non-interactive challenge from two commitments.
/// This is the Fiat-Shamir transform applied to the multiset-equality PIOP.
pub fn derive_challenge(a: &[u8; 32], b: &[u8; 32]) -> u64 {
    let mut h = Sha256::new();
    h.update(b"veria-multiset-challenge-v1");
    h.update(a);
    h.update(b);
    let d = h.finalize();
    u64::from_be_bytes(d[0..8].try_into().expect("8 bytes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_roundtrip() {
        for c in CircuitId::ALL {
            assert_eq!(CircuitId::from_str(c.name()).unwrap(), *c);
        }
    }

    #[test]
    fn unknown_circuit_returns_err() {
        assert!(CircuitId::from_str("nonsense").is_err());
    }

    #[test]
    fn elf_default_none() {
        for c in CircuitId::ALL {
            assert!(!c.elf_embedded());
        }
    }

    #[test]
    fn ml_underscore_alias() {
        assert_eq!(
            CircuitId::from_str("ml_inference").unwrap(),
            CircuitId::MlInference
        );
    }

    #[test]
    fn challenge_deterministic() {
        let a = [1u8; 32];
        let b = [2u8; 32];
        assert_eq!(derive_challenge(&a, &b), derive_challenge(&a, &b));
        assert_ne!(derive_challenge(&a, &b), derive_challenge(&b, &a));
    }
}
