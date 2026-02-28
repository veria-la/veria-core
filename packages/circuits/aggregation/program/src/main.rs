//! VERIA `aggregation` circuit — SP1 guest program.
//!
//! Computes SUM / AVG / MIN / MAX over a variable-length u64 vector (up to
//! 4096 entries). Single pass, u128 accumulator for the sum to avoid
//! overflow.
//!
//! References:
//!   * SP1 zkVM (Succinct Labs, 2024).
//!   * Used as a building block for oracle price aggregation; see
//!     `docs/circuits.md` §2.
//!
//! Determinism contract: no allocations after `entrypoint!`, no syscalls
//! beyond `sp1_zkvm::io::read` / `commit`, no floats, branches only on the
//! public `count` field.

#![no_main]

sp1_zkvm::entrypoint!(main);

use serde::{Deserialize, Serialize};

/// Hard upper bound on the aggregation vector length. The host enforces this
/// before submission; the guest re-checks as defense-in-depth.
pub const AGG_MAX: usize = 4096;

/// Wire format on the SP1 stdin channel.
///
/// We send `count` first, then `count` u64 words. We do not use `Vec<u64>`
/// directly because the SP1 reader works best with `read` of typed values;
/// keeping the protocol explicit means the host and guest agree on the byte
/// layout.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AggregationHeader {
    pub count: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct AggregationOutput {
    pub sum_u128: u128,
    pub avg_u64: u64,
    pub min_u64: u64,
    pub max_u64: u64,
    pub count: u32,
}

pub fn main() {
    let header: AggregationHeader = sp1_zkvm::io::read::<AggregationHeader>();
    let n = clamped_count(header.count);
    let mut sum: u128 = 0;
    let mut lo: u64 = u64::MAX;
    let mut hi: u64 = u64::MIN;
    let mut i = 0usize;
    while i < n {
        let v: u64 = sp1_zkvm::io::read::<u64>();
        sum = sum.wrapping_add(v as u128);
        if v < lo {
            lo = v;
        }
        if v > hi {
            hi = v;
        }
        i += 1;
    }
    let (avg, min_o, max_o) = if n == 0 {
        (0u64, 0u64, 0u64)
    } else {
        let avg = (sum / (n as u128)) as u64;
        (avg, lo, hi)
    };
    let output = AggregationOutput {
        sum_u128: sum,
        avg_u64: avg,
        min_u64: min_o,
        max_u64: max_o,
        count: n as u32,
    };
    sp1_zkvm::io::commit(&output);
}

/// Off-zkvm reference computation. Mirrors the loop above so the host can
/// cross-check without running a real proof.
pub fn compute(data: &[u64]) -> AggregationOutput {
    if data.is_empty() {
        return AggregationOutput {
            sum_u128: 0,
            avg_u64: 0,
            min_u64: 0,
            max_u64: 0,
            count: 0,
        };
    }
    let n = if data.len() > AGG_MAX {
        AGG_MAX
    } else {
        data.len()
    };
    let mut sum: u128 = 0;
    let mut lo = u64::MAX;
    let mut hi = u64::MIN;
    for &v in &data[..n] {
        sum = sum.wrapping_add(v as u128);
        if v < lo {
            lo = v;
        }
        if v > hi {
            hi = v;
        }
    }
    AggregationOutput {
        sum_u128: sum,
        avg_u64: (sum / n as u128) as u64,
        min_u64: lo,
        max_u64: hi,
        count: n as u32,
    }
}

/// Clamp `count` to `AGG_MAX`.
pub fn clamped_count(count: u32) -> usize {
    let n = count as usize;
    if n > AGG_MAX {
        AGG_MAX
    } else {
        n
    }
}
