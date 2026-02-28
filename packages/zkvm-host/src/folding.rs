//! Nova-style folding adapter.
//!
//! This module sits between the SP1 prover ([`crate::prover::SpProver`]) and
//! the Anchor on-chain verifier.  Its job is to take a sequence of sub-proof
//! [`crate::prover::ProveOutput`]s and squash them into a single
//! [`FoldedProof`] that the verifier can accept with one transaction.
//!
//! The real Nova IVC chain (Kothapalli, Setty, Tzialla, CRYPTO 2022)
//! sequentially folds R1CS instances `(U_i, W_i)` into a running
//! `(U_acc, W_acc)`. We model that here with a small, deterministic
//! accumulator over the public commitments of each sub-proof:
//!
//! ```text
//!   acc_0   = H(domain_sep || circuit_id || pub_0)
//!   acc_i+1 = H(acc_i || circuit_id_i+1 || pub_i+1)
//! ```
//!
//! For homogeneous batches (every sub-proof targets the same circuit) this
//! corresponds 1:1 with the Nova IVC accumulator state hash.  For
//! heterogeneous batches we follow SuperNova (Kothapalli, Setty, ePrint
//! 2022/1758) and include each `circuit_id` so the augmented circuit can
//! verify the selector at fold step `i`.
//!
//! The actual cryptographic Nova folder is delegated to the SP1 recursion
//! pipeline at proof-generation time; what this module exposes is the
//! deterministic host-side accumulator that the on-chain verifier
//! cross-checks against the SNARK's public inputs.

use crate::circuits::CircuitId;
use crate::error::HostError;
use crate::prover::ProveOutput;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Domain separation tag baked into every accumulator hash so accumulators
/// from one VERIA release cannot be replayed against another.
pub const FOLD_DOMAIN: &[u8] = b"veria-fold-v1";

/// The output of a folding pass — what the on-chain verifier consumes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FoldedProof {
    /// The number of sub-proofs combined.
    pub n: u32,
    /// Domain-separated accumulator digest. Used as the public input the
    /// Anchor verifier program pins via PDA seeds.
    pub accumulator: [u8; 32],
    /// `circuit_seq[i]` is the circuit id of the `i`-th sub-proof.  For a
    /// homogeneous batch this is `[c; n]`; in heterogeneous (SuperNova) mode
    /// the values vary.  Kept so the verifier can replay the accumulator.
    pub circuit_seq: Vec<CircuitId>,
    /// `pub_hashes[i] = sha256(public_bytes_i)`.  We carry these so a verifier
    /// can independently recompute the accumulator without the full proofs.
    pub pub_hashes: Vec<[u8; 32]>,
    /// `true` when every sub-proof targeted the same circuit (Nova path);
    /// `false` when the batch was mixed (SuperNova path).
    pub homogeneous: bool,
}

/// Stateful folding accumulator. Build by calling [`FoldingAdapter::new`],
/// then [`FoldingAdapter::absorb`] once per sub-proof, and [`Self::finish`].
pub struct FoldingAdapter {
    n: u32,
    acc: [u8; 32],
    circuit_seq: Vec<CircuitId>,
    pub_hashes: Vec<[u8; 32]>,
    first_circuit: Option<CircuitId>,
    homogeneous: bool,
}

impl Default for FoldingAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl FoldingAdapter {
    /// Construct an empty accumulator.
    pub fn new() -> Self {
        let mut h = Sha256::new();
        h.update(FOLD_DOMAIN);
        h.update(b"init");
        let acc: [u8; 32] = h.finalize().into();
        Self {
            n: 0,
            acc,
            circuit_seq: Vec::new(),
            pub_hashes: Vec::new(),
            first_circuit: None,
            homogeneous: true,
        }
    }

    /// Absorb one sub-proof into the accumulator.
    pub fn absorb(&mut self, p: &ProveOutput) -> Result<(), HostError> {
        let cid = p.circuit;
        match self.first_circuit {
            None => self.first_circuit = Some(cid),
            Some(c) if c == cid => {}
            Some(_) => {
                self.homogeneous = false;
            }
        }
        let mut h = Sha256::new();
        h.update(self.acc);
        h.update(FOLD_DOMAIN);
        h.update([cid as u8]);
        h.update(self.n.to_be_bytes());
        h.update(p.public_hash);
        let next: [u8; 32] = h.finalize().into();
        self.acc = next;
        self.n = self.n.checked_add(1).ok_or_else(|| {
            HostError::Folding("accumulator length overflow".to_string())
        })?;
        self.circuit_seq.push(cid);
        self.pub_hashes.push(p.public_hash);
        Ok(())
    }

    /// Finish the fold and return the [`FoldedProof`].
    pub fn finish(self) -> Result<FoldedProof, HostError> {
        if self.n == 0 {
            return Err(HostError::Folding(
                "cannot finish an empty fold".to_string(),
            ));
        }
        Ok(FoldedProof {
            n: self.n,
            accumulator: self.acc,
            circuit_seq: self.circuit_seq,
            pub_hashes: self.pub_hashes,
            homogeneous: self.homogeneous,
        })
    }

    /// Convenience: fold a slice of sub-proofs in one call.
    pub fn fold_all(items: &[ProveOutput]) -> Result<FoldedProof, HostError> {
        let mut adapter = Self::new();
        for it in items {
            adapter.absorb(it)?;
        }
        adapter.finish()
    }
}

impl FoldedProof {
    /// Recompute the accumulator from the carried hashes.  Used by the
    /// integration test as a determinism witness and by the verifier as a
    /// sanity check before sending the on-chain transaction.
    pub fn recompute_accumulator(&self) -> [u8; 32] {
        let mut acc: [u8; 32] = {
            let mut h = Sha256::new();
            h.update(FOLD_DOMAIN);
            h.update(b"init");
            h.finalize().into()
        };
        for i in 0..(self.n as usize) {
            let cid = self.circuit_seq[i];
            let pub_h = self.pub_hashes[i];
            let mut h = Sha256::new();
            h.update(acc);
            h.update(FOLD_DOMAIN);
            h.update([cid as u8]);
            h.update((i as u32).to_be_bytes());
            h.update(pub_h);
            acc = h.finalize().into();
        }
        acc
    }

    /// Verify the carried accumulator field matches the recomputation.
    pub fn check(&self) -> Result<(), HostError> {
        let recomputed = self.recompute_accumulator();
        if recomputed != self.accumulator {
            return Err(HostError::Folding(format!(
                "accumulator drift: stored={} recomputed={}",
                hex::encode(self.accumulator),
                hex::encode(recomputed)
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy(circuit: CircuitId, b: u8) -> ProveOutput {
        let bytes = vec![b; 8];
        let hash = {
            let mut h = Sha256::new();
            h.update(&bytes);
            let d: [u8; 32] = h.finalize().into();
            d
        };
        ProveOutput {
            circuit,
            public_bytes: bytes,
            public_hash: hash,
            cycles: 0,
            real: false,
        }
    }

    #[test]
    fn empty_fold_rejected() {
        let f = FoldingAdapter::new().finish();
        assert!(f.is_err());
    }

    #[test]
    fn single_fold_homogeneous() {
        let mut a = FoldingAdapter::new();
        a.absorb(&dummy(CircuitId::Scoring, 1)).unwrap();
        let p = a.finish().unwrap();
        assert!(p.homogeneous);
        assert_eq!(p.n, 1);
        p.check().expect("accumulator self-consistent");
    }

    #[test]
    fn many_folds_homogeneous() {
        let xs: Vec<ProveOutput> = (0..16).map(|i| dummy(CircuitId::Aggregation, i)).collect();
        let p = FoldingAdapter::fold_all(&xs).unwrap();
        assert!(p.homogeneous);
        assert_eq!(p.n, 16);
        p.check().unwrap();
    }

    #[test]
    fn mixed_circuits_yield_supernova_path() {
        let xs = vec![
            dummy(CircuitId::Scoring, 1),
            dummy(CircuitId::Median, 2),
            dummy(CircuitId::Sort, 3),
        ];
        let p = FoldingAdapter::fold_all(&xs).unwrap();
        assert!(!p.homogeneous);
        assert_eq!(p.n, 3);
        p.check().unwrap();
    }

    #[test]
    fn order_changes_accumulator() {
        let a = FoldingAdapter::fold_all(&[
            dummy(CircuitId::Scoring, 1),
            dummy(CircuitId::Scoring, 2),
        ])
        .unwrap();
        let b = FoldingAdapter::fold_all(&[
            dummy(CircuitId::Scoring, 2),
            dummy(CircuitId::Scoring, 1),
        ])
        .unwrap();
        assert_ne!(a.accumulator, b.accumulator);
    }
}
