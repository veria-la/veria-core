//! VERIA `ml-inference` circuit — SP1 guest program.
//!
//! Verifies a forward pass of a 3-layer MLP:
//!
//!   features (32)  -> w1 (16x32) + b1  -> ReLU
//!                  -> w2 (8x16)  + b2  -> ReLU
//!                  -> w3 (4x8)   + b3  -> logits (no activation)
//!
//! All arithmetic is fixed-point with scale `2^16` and uses `i32` storage.
//! Multiplications run in `i64` and are shifted right by 16 before
//! accumulation. The accumulator is saturated to the `i32` range so the
//! guest never panics on a deep-negative or deep-positive product.
//!
//! Public output: the four output logits plus a SHA-256 commitment over the
//! flattened weights. The weight commitment lets a verifier pin "this
//! prediction came from this exact model" without revealing the weights
//! themselves on-chain.
//!
//! References:
//!   * SP1 zkVM (Succinct Labs, 2024).
//!   * Lee, Kim, Lee, et al., "vCNN" (2020) — verifiable inference framing.
//!   * Modulus Labs, "RemainderNet" (2024) — verifiable MLP benchmark.
//!
//! Determinism: branches only on the layer shapes, never on activations.

#![no_main]

sp1_zkvm::entrypoint!(main);

use serde::{Deserialize, Serialize};

pub const ML_IN: usize = 32;
pub const ML_H1: usize = 16;
pub const ML_H2: usize = 8;
pub const ML_OUT: usize = 4;
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

pub fn main() {
    let input: MlInput = sp1_zkvm::io::read::<MlInput>();
    let output = compute(&input);
    sp1_zkvm::io::commit(&output);
}

pub fn compute(input: &MlInput) -> MlOutput {
    // Layer 1: 32 -> 16, ReLU.
    let mut h1 = [0i32; ML_H1];
    let mut j = 0usize;
    while j < ML_H1 {
        let mut acc: i64 = input.b1[j] as i64;
        let mut i = 0usize;
        while i < ML_IN {
            let p = (input.features[i] as i64) * (input.w1[j][i] as i64);
            acc = acc.saturating_add(p >> ML_FP_SHIFT);
            i += 1;
        }
        h1[j] = relu_saturating(acc);
        j += 1;
    }

    // Layer 2: 16 -> 8, ReLU.
    let mut h2 = [0i32; ML_H2];
    let mut j = 0usize;
    while j < ML_H2 {
        let mut acc: i64 = input.b2[j] as i64;
        let mut i = 0usize;
        while i < ML_H1 {
            let p = (h1[i] as i64) * (input.w2[j][i] as i64);
            acc = acc.saturating_add(p >> ML_FP_SHIFT);
            i += 1;
        }
        h2[j] = relu_saturating(acc);
        j += 1;
    }

    // Output layer: 8 -> 4, no activation (logits).
    let mut logits = [0i32; ML_OUT];
    let mut j = 0usize;
    while j < ML_OUT {
        let mut acc: i64 = input.b3[j] as i64;
        let mut i = 0usize;
        while i < ML_H2 {
            let p = (h2[i] as i64) * (input.w3[j][i] as i64);
            acc = acc.saturating_add(p >> ML_FP_SHIFT);
            i += 1;
        }
        logits[j] = saturating_cast(acc);
        j += 1;
    }

    let model_commit = model_commitment(input);
    MlOutput {
        logits,
        model_commit,
    }
}

pub fn relu_saturating(acc: i64) -> i32 {
    if acc <= 0 {
        0
    } else if acc > i32::MAX as i64 {
        i32::MAX
    } else {
        acc as i32
    }
}

pub fn saturating_cast(acc: i64) -> i32 {
    if acc > i32::MAX as i64 {
        i32::MAX
    } else if acc < i32::MIN as i64 {
        i32::MIN
    } else {
        acc as i32
    }
}

pub fn model_commitment(input: &MlInput) -> [u8; 32] {
    let mut h = MiniSha256::new();
    for row in &input.w1 {
        for &x in row {
            h.update(&x.to_be_bytes());
        }
    }
    for &x in &input.b1 {
        h.update(&x.to_be_bytes());
    }
    for row in &input.w2 {
        for &x in row {
            h.update(&x.to_be_bytes());
        }
    }
    for &x in &input.b2 {
        h.update(&x.to_be_bytes());
    }
    for row in &input.w3 {
        for &x in row {
            h.update(&x.to_be_bytes());
        }
    }
    for &x in &input.b3 {
        h.update(&x.to_be_bytes());
    }
    h.finalize()
}

// =====================================================================
// Minimal SHA-256 (same as median/sort guests; duplicated intentionally).
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
