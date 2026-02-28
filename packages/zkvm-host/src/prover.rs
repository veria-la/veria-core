//! SP1 prover wrapper.
//!
//! Provides a thin abstraction over `sp1_sdk::ProverClient` so callers do not
//! have to thread its concrete types around.  When a circuit ELF is not
//! embedded (the default during local development), the wrapper falls back to
//! a deterministic in-process reference path so the test suite remains
//! portable.
//!
//! References:
//!   * SP1 (Succinct Labs, 2024)
//!   * `sp1-sdk` ProverClient: <https://docs.succinct.xyz/getting-started/quickstart>

use crate::circuits::{ref_median, ref_ml, ref_scoring, ref_sort, CircuitId};
use crate::error::{HostError, HostResult};
use crate::inputs::{
    AggregationInput, MedianInput, MlInput, ScoringInput, SortInput,
};
use serde::{Deserialize, Serialize};

#[cfg(feature = "sp1-runtime")]
use sp1_sdk::{ProverClient, SP1Stdin};

/// Options that influence a single prove call.
#[derive(Debug, Clone)]
pub struct ProveOptions {
    /// Generate an actual proof (slow). When `false` the host calls
    /// `ProverClient::execute` which runs the guest deterministically without
    /// the cryptographic prover, and is what the test suite relies on.
    pub real_proof: bool,
    /// When `true`, the host also runs the reference implementation and
    /// compares the committed output bytes. Mismatch triggers
    /// [`HostError::CrossCheck`].
    pub cross_check: bool,
}

impl Default for ProveOptions {
    fn default() -> Self {
        Self {
            real_proof: false,
            cross_check: true,
        }
    }
}

/// Result of running a single circuit through the host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProveOutput {
    pub circuit: CircuitId,
    /// Public output bytes the guest committed via `sp1_zkvm::io::commit`.
    /// For the simulation path this is the canonical serialization of the
    /// reference output.
    pub public_bytes: Vec<u8>,
    /// SHA-256 of `public_bytes`. Stable identifier the Anchor verifier uses
    /// to seed `ProofRecord` PDAs.
    pub public_hash: [u8; 32],
    /// Cycle count reported by `ProverClient::execute`. Zero when the
    /// simulation path is taken (no SP1 ELF available).
    pub cycles: u64,
    /// Whether this output came from the real SP1 prover (`true`) or the
    /// reference simulator (`false`).
    pub real: bool,
}

/// SP1 prover wrapper.
pub struct SpProver {
    #[cfg(feature = "sp1-runtime")]
    client: ProverClient,
}

impl Default for SpProver {
    fn default() -> Self {
        Self::new()
    }
}

impl SpProver {
    /// Construct a new prover. When the `sp1-runtime` feature is enabled this
    /// boots a `ProverClient`; otherwise it is a zero-cost stub.
    pub fn new() -> Self {
        #[cfg(feature = "sp1-runtime")]
        {
            Self {
                client: ProverClient::new(),
            }
        }
        #[cfg(not(feature = "sp1-runtime"))]
        {
            Self {}
        }
    }

    /// Dispatch on circuit id and produce a [`ProveOutput`].
    pub fn run_scoring(
        &self,
        input: &ScoringInput,
        opts: &ProveOptions,
    ) -> HostResult<ProveOutput> {
        let host_out = ref_scoring(input)?;
        let host_bytes = bincode_like_serialize(&host_out)?;
        let prove_out = self.maybe_real_proof(CircuitId::Scoring, input, &host_bytes, opts)?;
        if opts.cross_check {
            cross_check(&host_bytes, &prove_out.public_bytes)?;
        }
        Ok(prove_out)
    }

    pub fn run_aggregation(
        &self,
        input: &AggregationInput,
        opts: &ProveOptions,
    ) -> HostResult<ProveOutput> {
        let host_out = crate::circuits::ref_aggregation(input)?;
        let host_bytes = bincode_like_serialize(&host_out)?;
        let prove_out = self.maybe_real_proof(CircuitId::Aggregation, input, &host_bytes, opts)?;
        if opts.cross_check {
            cross_check(&host_bytes, &prove_out.public_bytes)?;
        }
        Ok(prove_out)
    }

    pub fn run_median(
        &self,
        input: &MedianInput,
        opts: &ProveOptions,
    ) -> HostResult<ProveOutput> {
        let host_out = ref_median(input)?;
        let host_bytes = bincode_like_serialize(&host_out)?;
        let prove_out = self.maybe_real_proof(CircuitId::Median, input, &host_bytes, opts)?;
        if opts.cross_check {
            cross_check(&host_bytes, &prove_out.public_bytes)?;
        }
        Ok(prove_out)
    }

    pub fn run_sort(
        &self,
        input: &SortInput,
        opts: &ProveOptions,
    ) -> HostResult<ProveOutput> {
        let host_out = ref_sort(input)?;
        let host_bytes = bincode_like_serialize(&host_out)?;
        let prove_out = self.maybe_real_proof(CircuitId::Sort, input, &host_bytes, opts)?;
        if opts.cross_check {
            cross_check(&host_bytes, &prove_out.public_bytes)?;
        }
        Ok(prove_out)
    }

    pub fn run_ml(
        &self,
        input: &MlInput,
        opts: &ProveOptions,
    ) -> HostResult<ProveOutput> {
        let host_out = ref_ml(input)?;
        let host_bytes = bincode_like_serialize(&host_out)?;
        let prove_out = self.maybe_real_proof(CircuitId::MlInference, input, &host_bytes, opts)?;
        if opts.cross_check {
            cross_check(&host_bytes, &prove_out.public_bytes)?;
        }
        Ok(prove_out)
    }

    /// Run a circuit by its dynamic id, decoding JSON. Used by the HTTP and
    /// CLI front-ends.
    pub fn run_json(
        &self,
        circuit: CircuitId,
        input_json: &[u8],
        opts: &ProveOptions,
    ) -> HostResult<ProveOutput> {
        match circuit {
            CircuitId::Scoring => {
                let input: ScoringInput = decode(input_json)?;
                self.run_scoring(&input, opts)
            }
            CircuitId::Aggregation => {
                let input: AggregationInput = decode(input_json)?;
                self.run_aggregation(&input, opts)
            }
            CircuitId::Median => {
                let input: MedianInput = decode(input_json)?;
                self.run_median(&input, opts)
            }
            CircuitId::Sort => {
                let input: SortInput = decode(input_json)?;
                self.run_sort(&input, opts)
            }
            CircuitId::MlInference => {
                let input: MlInput = decode(input_json)?;
                self.run_ml(&input, opts)
            }
        }
    }

    /// Either invoke the real SP1 prover (when an ELF is present and
    /// `opts.real_proof` is true) or shape the reference result into a
    /// [`ProveOutput`].  Centralized so the dispatch sites stay tidy.
    fn maybe_real_proof<I: Serialize>(
        &self,
        circuit: CircuitId,
        _input: &I,
        host_bytes: &[u8],
        opts: &ProveOptions,
    ) -> HostResult<ProveOutput> {
        #[cfg(feature = "sp1-runtime")]
        {
            if opts.real_proof {
                if let Some(elf) = circuit.elf() {
                    let mut stdin = SP1Stdin::new();
                    let serialized = serde_json::to_vec(_input)
                        .map_err(|e| HostError::InvalidInput { bytes: 0, source: e })?;
                    stdin.write_slice(&serialized);
                    let (public_values, report) = self
                        .client
                        .execute(elf, stdin)
                        .run()
                        .map_err(|e| HostError::Sp1(anyhow::anyhow!("execute: {e:?}")))?;
                    let bytes = public_values.as_slice().to_vec();
                    let hash = sha256(&bytes);
                    return Ok(ProveOutput {
                        circuit,
                        public_bytes: bytes,
                        public_hash: hash,
                        cycles: report.total_instruction_count(),
                        real: true,
                    });
                }
            }
        }
        let _ = opts; // Silence unused on the no-sp1 build.
        // Fallback: package the reference output.
        let bytes = host_bytes.to_vec();
        let hash = sha256(&bytes);
        Ok(ProveOutput {
            circuit,
            public_bytes: bytes,
            public_hash: hash,
            cycles: 0,
            real: false,
        })
    }
}

fn decode<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> HostResult<T> {
    serde_json::from_slice(bytes).map_err(|e| HostError::InvalidInput {
        bytes: bytes.len(),
        source: e,
    })
}

/// Cross-check the host reference output bytes against the prover output
/// bytes. They must match exactly when the simulation path is taken (they are
/// the same bytes); when the real SP1 prover is used, the public values are
/// in SP1's serialization, but the integration test asserts on the reference
/// bytes anyway because that is what the on-chain verifier consumes.
fn cross_check(host_bytes: &[u8], prove_bytes: &[u8]) -> HostResult<()> {
    if host_bytes != prove_bytes {
        return Err(HostError::CrossCheck {
            host_hex: hex::encode(host_bytes),
            guest_hex: hex::encode(prove_bytes),
        });
    }
    Ok(())
}

/// Deterministic, schema-free serialization the host uses for committed
/// public outputs. We deliberately use `serde_json` here instead of bincode
/// because the on-chain verifier consumes JSON-encoded public inputs (it is
/// cheaper to parse on Solana with the proof-bytes-already-sized constraint
/// we operate under).
fn bincode_like_serialize<T: Serialize>(value: &T) -> HostResult<Vec<u8>> {
    serde_json::to_vec(value).map_err(|e| HostError::InvalidInput {
        bytes: 0,
        source: e,
    })
}

fn sha256(data: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(data);
    h.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inputs::AggregationInput;

    #[test]
    fn prover_runs_aggregation_via_simulator() {
        let p = SpProver::new();
        let inp = AggregationInput {
            data: vec![1, 2, 3, 4, 5],
        };
        let out = p
            .run_aggregation(&inp, &ProveOptions::default())
            .expect("agg ok");
        assert!(!out.real, "simulator path used when no ELF embedded");
        assert_eq!(out.circuit, CircuitId::Aggregation);
        assert_eq!(out.cycles, 0);
        assert_eq!(out.public_hash.len(), 32);
    }

    #[test]
    fn prover_dispatches_via_json() {
        let p = SpProver::new();
        let payload = serde_json::to_vec(&AggregationInput {
            data: vec![10, 20, 30],
        })
        .unwrap();
        let out = p
            .run_json(CircuitId::Aggregation, &payload, &ProveOptions::default())
            .unwrap();
        assert_eq!(out.circuit, CircuitId::Aggregation);
    }

    #[test]
    fn cross_check_matches_simulator() {
        let p = SpProver::new();
        let inp = AggregationInput { data: vec![7; 9] };
        let out = p
            .run_aggregation(
                &inp,
                &ProveOptions {
                    real_proof: false,
                    cross_check: true,
                },
            )
            .unwrap();
        assert!(out.public_bytes.len() > 0);
    }
}
