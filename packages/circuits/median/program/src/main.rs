//! VERIA `median` circuit — SP1 guest program.
//!
//! The guest receives both an unsorted input vector and a host-provided
//! sorted permutation, then proves three things succinctly:
//!
//!   1. The sorted vector is monotonic (non-decreasing).
//!   2. The permutation is a valid bijection — each `perm[i]` lies in
//!      `0..count` and is unique.
//!   3. `sorted[i] == raw[perm[i]]` for every `i in 0..count`.
//!
//! Then it commits the middle element (median) and a SHA-256 commitment of
//! the sorted vector so downstream code can refer to the sorted result
//! without re-reading the full proof.
//!
//! References:
//!   * SP1 zkVM (Succinct Labs, 2024).
//!   * Pyth-style median oracle (`docs/circuits.md` §3).
//!
//! Determinism: branches only on the public `count` field; the witness
//! checks return cleanly (panic) on failure — soundness comes from SP1.

#![no_main]

sp1_zkvm::entrypoint!(main);

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

pub const MEDIAN_MAX: usize = 256;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MedianInput {
    #[serde(with = "BigArray")]
    pub raw: [u64; MEDIAN_MAX],
    #[serde(with = "BigArray")]
    pub sorted: [u64; MEDIAN_MAX],
    #[serde(with = "BigArray")]
    pub perm: [u16; MEDIAN_MAX],
    pub count: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct MedianOutput {
    pub median: u64,
    pub sorted_commit: [u8; 32],
    pub count: u32,
}

pub fn main() {
    let input: MedianInput = sp1_zkvm::io::read::<MedianInput>();
    let output = compute(&input);
    sp1_zkvm::io::commit(&output);
}

/// Off-zkvm reference. Panics on witness violations to mirror the guest's
/// "no recovery" stance.
pub fn compute(input: &MedianInput) -> MedianOutput {
    let n = clamped_count(input.count);
    if n == 0 {
        return MedianOutput {
            median: 0,
            sorted_commit: [0u8; 32],
            count: 0,
        };
    }
    // 1) monotonicity witness.
    let mut i = 1usize;
    while i < n {
        assert!(input.sorted[i] >= input.sorted[i - 1], "sorted not monotonic");
        i += 1;
    }
    // 2) permutation validity — every perm[i] in range, mutually distinct.
    let mut seen = [false; MEDIAN_MAX];
    let mut i = 0usize;
    while i < n {
        let p = input.perm[i] as usize;
        assert!(p < n, "perm index out of bounds");
        assert!(!seen[p], "perm index reused");
        seen[p] = true;
        // 3) sorted[i] == raw[perm[i]]
        assert!(input.sorted[i] == input.raw[p], "perm/sorted mismatch");
        i += 1;
    }
    // Compute the lower median for even counts to keep the guest fully
    // deterministic without floating-point. Host pins the same convention.
    let median = if n % 2 == 1 {
        input.sorted[n / 2]
    } else {
        input.sorted[n / 2 - 1]
    };
    let sorted_commit = sha256_u64_be(&input.sorted, n);
    MedianOutput {
        median,
        sorted_commit,
        count: input.count,
    }
}

/// Clamp the public `count` to `MEDIAN_MAX`.
pub fn clamped_count(count: u32) -> usize {
    let n = count as usize;
    if n > MEDIAN_MAX {
        MEDIAN_MAX
    } else {
        n
    }
}

/// SHA-256 over the first `n` u64 entries of `buf`, in big-endian byte order.
/// Implemented locally with the `sha2`-compatible Poseidon-free path so the
/// guest does not need to pull in heavyweight dependencies (we re-implement
/// SHA-256 directly here would be over-engineering; we rely on SP1 to provide
/// `sha2`-grade hashing via a syscall in production, but for the off-zkvm
/// host path we use the standalone `sha2` crate transparently via the host
/// reference function).
///
/// In the guest binary this function is intentionally simple: it XORs the
/// length and the bytes into a 32-byte buffer.  The host re-implements the
/// _same_ collapsing rule in `veria_zkvm_host::circuits::ref_median`
/// (modulo the host using `sha2::Sha256`).  The two pipelines are kept in
/// sync by the integration cross-check, which is the source of truth.
fn sha256_u64_be(buf: &[u64; MEDIAN_MAX], n: usize) -> [u8; 32] {
    // Implementation note: SP1's `precompiles` SHA-256 is faster but requires
    // syscall opt-in. The reference path here uses a constant-time fold so
    // the guest behaviour is deterministic regardless of precompile config.
    // The on-chain verifier hashes the same way (`commit_u64_be` in the host).
    sha256_be_u64(&buf[..n])
}

/// Constant-time pure-Rust SHA-256 over the big-endian byte representation
/// of a u64 slice.  We inline a minimal implementation to keep the guest
/// dependency footprint small.
fn sha256_be_u64(data: &[u64]) -> [u8; 32] {
    let mut hasher = MiniSha256::new();
    for &v in data {
        hasher.update(&v.to_be_bytes());
    }
    hasher.finalize()
}

// =====================================================================
// Minimal SHA-256 implementation. RFC 6234.
// We embed it here so the guest does not need to pull `sha2`'s asm/optimised
// features — those are unhelpful inside zkVM execution where every cycle
// counts and we want a deterministic round count.
// =====================================================================

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

struct MiniSha256 {
    h: [u32; 8],
    buf: [u8; 64],
    buf_len: usize,
    total_len: u64,
}

impl MiniSha256 {
    fn new() -> Self {
        Self {
            h: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buf: [0u8; 64],
            buf_len: 0,
            total_len: 0,
        }
    }

    fn update(&mut self, data: &[u8]) {
        self.total_len = self.total_len.wrapping_add(data.len() as u64);
        let mut idx = 0;
        while idx < data.len() {
            let take = core::cmp::min(64 - self.buf_len, data.len() - idx);
            self.buf[self.buf_len..self.buf_len + take]
                .copy_from_slice(&data[idx..idx + take]);
            self.buf_len += take;
            idx += take;
            if self.buf_len == 64 {
                let block = self.buf;
                self.compress(&block);
                self.buf_len = 0;
            }
        }
    }

    fn finalize(mut self) -> [u8; 32] {
        let total_bits = self.total_len.wrapping_mul(8);
        self.buf[self.buf_len] = 0x80;
        self.buf_len += 1;
        if self.buf_len > 56 {
            for b in &mut self.buf[self.buf_len..] {
                *b = 0;
            }
            let block = self.buf;
            self.compress(&block);
            self.buf_len = 0;
        }
        for b in &mut self.buf[self.buf_len..56] {
            *b = 0;
        }
        self.buf[56..64].copy_from_slice(&total_bits.to_be_bytes());
        let block = self.buf;
        self.compress(&block);
        let mut out = [0u8; 32];
        for (i, w) in self.h.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&w.to_be_bytes());
        }
        out
    }

    fn compress(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut a = self.h[0];
        let mut b = self.h[1];
        let mut c = self.h[2];
        let mut d = self.h[3];
        let mut e = self.h[4];
        let mut f = self.h[5];
        let mut g = self.h[6];
        let mut hh = self.h[7];
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let mj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(mj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        self.h[0] = self.h[0].wrapping_add(a);
        self.h[1] = self.h[1].wrapping_add(b);
        self.h[2] = self.h[2].wrapping_add(c);
        self.h[3] = self.h[3].wrapping_add(d);
        self.h[4] = self.h[4].wrapping_add(e);
        self.h[5] = self.h[5].wrapping_add(f);
        self.h[6] = self.h[6].wrapping_add(g);
        self.h[7] = self.h[7].wrapping_add(hh);
    }
}
