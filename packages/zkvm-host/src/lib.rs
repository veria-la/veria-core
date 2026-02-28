//! # veria-zkvm-host
//!
//! Off-chain proving service for VERIA.
//!
//! This crate wraps three layers:
//!
//! 1. **SP1 zkVM** (`sp1_sdk`) — Succinct Labs, 2024. RISC-V based zkVM. Each
//!    guest program in `packages/circuits/<name>/program/` compiles to an ELF
//!    that the host loads with [`crate::prover::SpProver`].
//! 2. **Nova folding** (Kothapalli, Setty, Tzialla, CRYPTO 2022). Sequential
//!    folding of sub-proofs into a single recursive accumulator. The adapter
//!    lives in [`crate::folding`]. For non-uniform batches we follow
//!    SuperNova (Kothapalli, Setty, ePrint 2022/1758).
//! 3. **HTTP bridge** ([`crate::api`]) — an Axum service that lets the FastAPI
//!    Compute API enqueue jobs against the host.
//!
//! The CLI front-end is in `src/main.rs`.
//!
//! ## Determinism
//!
//! Every public entry point in this crate is deterministic given the same
//! guest input. The fall-back simulation path in `prover.rs` (used when no
//! SP1 ELF is available) mirrors the guest computation exactly so test
//! fixtures stay valid across both the real prover and the simulator.

pub mod api;
pub mod circuits;
pub mod error;
pub mod folding;
pub mod inputs;
pub mod prover;

pub use circuits::CircuitId;
pub use error::HostError;
pub use folding::{FoldedProof, FoldingAdapter};
pub use prover::{ProveOptions, ProveOutput, SpProver};

/// Crate version surfaced into headers, telemetry, and CLI banners.
pub const VERIA_HOST_VERSION: &str = env!("CARGO_PKG_VERSION");

/// SP1 SDK major version VERIA targets.  Bumped only after we have run the
/// full regression suite against the new SDK.
pub const SP1_TARGET_MAJOR: u32 = 3;
