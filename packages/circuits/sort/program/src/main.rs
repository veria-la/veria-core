//! VERIA `sort` circuit — SP1 guest program.
//!
//! Proves that `sorted` is a non-decreasing permutation of `input`. The
//! multiset equality is enforced with a running product hash in the spirit of
//! Schwartz-Zippel:
//!
//!   prod_{i in 0..count} (r + input[i])   ==   prod_{i in 0..count} (r + sorted[i])
//!
//! where `r` is a non-interactive Fiat-Shamir challenge derived from the
//! SHA-256 commitments of both vectors. Monotonicity is a separate scan.
//!
//! For the public output we commit two SHA-256 digests so consumers can
//! reference either vector without re-streaming the proof.
//!
//! References:
//!   * Sort-PIOP / multiset-equality argument: Plookup (Gabizon, Williamson
//!     2020) and Halo2's logUp — same shape, simpler form.
//!   * SP1 zkVM (Succinct Labs, 2024).
//!
//! Determinism: branches only on the public `count`; the witness checks
//! abort cleanly on failure.

#![no_main]

sp1_zkvm::entrypoint!(main);

use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

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
    pub sorted_commit: [u8; 32],
    pub input_commit: [u8; 32],
    pub count: u32,
}

pub fn main() {
    let input: SortInput = sp1_zkvm::io::read::<SortInput>();
    let output = compute(&input);
    sp1_zkvm::io::commit(&output);
}

pub fn compute(input: &SortInput) -> SortOutput {
    let n = clamped_count(input.count);
    if n == 0 {
        return SortOutput {
            sorted_commit: [0u8; 32],
            input_commit: [0u8; 32],
            count: 0,
        };
    }

    // 1) Monotonicity.
    let mut i = 1usize;
    while i < n {
        assert!(input.sorted[i] >= input.sorted[i - 1], "sorted not monotonic");
        i += 1;
    }

    // 2) Compute commitments and derive challenge r.
    let input_commit = sha256_be_u64(&input.input, n);
    let sorted_commit = sha256_be_u64(&input.sorted, n);
    let r = derive_challenge(&input_commit, &sorted_commit);
    let r128 = r as u128;

    // 3) Running-product multiset equality.
    let mut prod_in: u128 = 1;
    let mut prod_out: u128 = 1;
    let mask: u128 = u64::MAX as u128;
    let mut j = 0usize;
    while j < n {
        let a = r128.wrapping_add(input.input[j] as u128) & mask;
        let b = r128.wrapping_add(input.sorted[j] as u128) & mask;
        prod_in = prod_in.wrapping_mul(a) & mask;
        prod_out = prod_out.wrapping_mul(b) & mask;
        j += 1;
    }
    assert!(prod_in == prod_out, "multiset mismatch");

    SortOutput {
        sorted_commit,
        input_commit,
        count: input.count,
    }
}

/// Clamp the public `count` to `SORT_MAX`.
pub fn clamped_count(count: u32) -> usize {
    let n = count as usize;
    if n > SORT_MAX {
        SORT_MAX
    } else {
        n
    }
}

/// Fiat-Shamir challenge derivation. Same shape as the host's
/// `derive_challenge` in `circuits.rs` so the two pipelines agree.
pub fn derive_challenge(a: &[u8; 32], b: &[u8; 32]) -> u64 {
    let mut h = MiniSha256::new();
    h.update(b"veria-multiset-challenge-v1");
    h.update(a);
    h.update(b);
    let d = h.finalize();
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&d[0..8]);
    u64::from_be_bytes(bytes)
}

fn sha256_be_u64(buf: &[u64; SORT_MAX], n: usize) -> [u8; 32] {
    let mut h = MiniSha256::new();
    let mut i = 0usize;
    while i < n {
        h.update(&buf[i].to_be_bytes());
        i += 1;
    }
    h.finalize()
}

// =====================================================================
// Minimal SHA-256.  Same code as the median guest; duplicated to keep each
// guest crate self-contained (they cannot share a workspace lib because
// the SP1 toolchain treats each guest as its own RISC-V build).
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
