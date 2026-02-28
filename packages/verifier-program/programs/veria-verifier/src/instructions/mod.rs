//! VERIA verifier instructions.
//!
//! Each public on-chain entry point lives in its own module so the file is
//! single-responsibility and the IDL section for that instruction is easy
//! to audit.  The three instructions in v0.1 are:
//!
//! | name           | who calls it    | purpose                                |
//! |----------------|-----------------|----------------------------------------|
//! | `initialize`   | admin (once)    | create `VerifierConfig` PDA + vk hash  |
//! | `verify_proof` | anyone          | Groth16 verify + write `ProofRecord`   |
//! | `update_vk`    | admin           | rotate the registered vk hash + epoch  |
//!
//! The handler signatures here mirror the Anchor `#[program]` shims defined
//! in `crate::veria_verifier` (i.e. `lib.rs`).  Splitting them out keeps
//! every individual handler under ~200 lines while still letting Anchor
//! generate one IDL section per `#[derive(Accounts)]`.

pub mod initialize;
pub mod update_vk;
pub mod verify_proof;

// Glob re-exports.  Each per-instruction module renames its handler to
// `handler_<ix>` so the glob does not produce three colliding `handler`
// names — the `Accounts` struct, the public handler function, and any
// Anchor-generated `__client_accounts_*` helper all reach the crate root.
pub use initialize::*;
pub use update_vk::*;
pub use verify_proof::*;
