# `packages/verifier-program/` — Service Context

This directory is the Solana mainnet root of trust for VERIA.  Everything
else in the monorepo treats the on-chain `verify_proof` instruction as the
final arbiter of correctness.

## Role in the project

* **1st hook (Folding Visualizer).**  The visualizer displays the
  before/after cost of folding (0.5 SOL → 0.0001 SOL).  The "0.0001 SOL"
  number is the empirical cost of calling `verify_proof` on mainnet — so
  this program is the source of that number.
* **2nd hook (Compute API + Anchor mainnet verifier).**  The Compute API
  produces SP1 proofs off-chain; the only way those proofs become
  *verified* in the user-facing sense is by landing here.  The
  `_DIRECTION.md` `NEXT_PUBLIC_2_PROGRAM_ID` env var points at this
  program.
* **3rd hook (CLI).**  `veria verify proof.bin --program $VERIA_PROGRAM`
  inside `packages/cli/` translates to a `verify_proof` instruction call
  against this program.

## Why a separate Cargo workspace

The top-level `Cargo.toml` excludes this directory because Anchor needs
its own workspace for the BPF / SBF target.  The host crates use a wide
dependency tree (`tokio`, `axum`, `sp1-sdk`) that does not link on the
on-chain target; isolating the workspaces lets each side compile with
exactly the dependencies it needs.

## Production vs. iteration build

The verifier has two cargo features:

* **default** (no features) — uses the deterministic stub verifier inside
  `instructions/verify_proof.rs`.  Production deploys MUST NOT use this
  mode.  The stub exists so `cargo test` runs fast on machines without
  the BN254 pairing crate.
* **`sp1-verify`** — links `sp1-solana` and dispatches the real Groth16
  verification on-chain.  Build with `anchor build -- --features sp1-verify`.

The migration script (`migrations/deploy.ts`) is feature-agnostic — it
calls `initialize` either way and the on-chain registry stores the
vk_hash, not the raw key.

## Mainnet deployment checklist (excerpt)

See `new_project_guide/14_SOLANA_DEPLOYMENT.md` for the full checklist.

1. `anchor keys list` — record the program ID.
2. Replace `declare_id!("Ver1aVeRiFier11111111111111111111111111111")`
   in `programs/veria-verifier/src/lib.rs` with the printed ID.
3. Replace the three matching entries in `Anchor.toml`.
4. `anchor build -- --features sp1-verify`.
5. `solana program deploy target/deploy/veria_verifier.so --url $HELIUS_RPC_URL`.
6. `anchor idl init --provider.cluster mainnet -f target/idl/veria_verifier.json $PROGRAM_ID`.
7. `anchor migrate --provider.cluster mainnet`.
8. Confirm Solana Explorer renders the program at 200 OK.
9. Update `_DIRECTION.md`:
   * `NEXT_PUBLIC_2_PROGRAM_ID` = the deployed program ID.
   * `NEXT_PUBLIC_2_API=true` flips on the 2nd hook.

## Files touched by future work

* `programs/veria-verifier/src/state.rs` — adding a circuit means bumping
  `MAX_CIRCUIT_ID` and documenting the new id in `docs/circuits.md`.
* `programs/veria-verifier/src/instructions/verify_proof.rs` — vk-hash
  rotation logic lives in `verify_sp1_groth16`.
* `tests/veria-verifier.ts` — add a test case per new failure mode.
