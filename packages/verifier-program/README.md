# VERIA Verifier — Solana Mainnet SP1 Groth16 Verifier

`packages/verifier-program/` is the on-chain root of trust for VERIA.  The
program ingests an SP1 Groth16 proof, dispatches verification to the
[`sp1-solana`](https://crates.io/crates/sp1-solana) on-chain verifier
published by Succinct Labs, and writes a deterministic `ProofRecord`
account that downstream consumers can read in a Pyth-style marketplace
pattern.

> "Few dots. Whole truth."

| field                       | value                                            |
|-----------------------------|--------------------------------------------------|
| Framework                   | Anchor 0.31.1                                    |
| Cluster                     | Solana mainnet (Sealevel)                        |
| Verifier                    | SP1 Groth16 via `sp1-solana = "0.1.0"`           |
| zkVM source                 | SP1 (Succinct Labs, 2024)                        |
| Folding scheme              | Nova / SuperNova (Kothapalli et al., 2022)       |
| Trust root                  | Anchor program ID published in `_DIRECTION.md`   |
| Re-init protection          | `init` not `init_if_needed`                      |
| Replay protection           | 8-byte cluster prefix in `public_inputs`         |

## Why a custom verifier?

The Solana BPF / SBF runtime can verify a single Groth16 proof in
~200K compute units, well within the 1.4M CU budget.  But there is no
default "SP1 verifier" program shipped with the runtime, so every project
that wants to consume SP1 proofs on-chain needs to embed the verifier in
its own program.

VERIA's verifier is intentionally minimal: one config PDA, one record
PDA, three instructions.  Every higher-level concept (job queues, the
marketplace, the visualizer) lives off-chain and reads from this program.

## Architecture

```mermaid
flowchart LR
    subgraph offchain["Off-chain (Compute API)"]
        A[Compute API<br/>Next.js + FastAPI] --> B[SP1 Host<br/>Rust + sp1-sdk v3]
        B --> C[Nova / SuperNova<br/>folding adapter]
        C --> D[(Folded SP1 proof<br/>~260B + public inputs)]
    end

    subgraph onchain["On-chain (Solana mainnet)"]
        E[verify_proof ix]
        F[(ProofRecord PDA<br/>seeds: b\"proof\" || hash)]
        G[(VerifierConfig PDA<br/>seeds: b\"config\")]
    end

    D -->|Anchor tx| E
    E -->|init| F
    E -->|mut total_verified| G
    F -->|read| H[Marketplace<br/>subscribers]
```

## Instructions

| name           | who      | purpose                                                              |
|----------------|----------|----------------------------------------------------------------------|
| `initialize`   | admin    | Create the global `VerifierConfig` PDA and register the active vk.   |
| `verify_proof` | anyone   | Verify a Groth16 proof and write a `ProofRecord` PDA.                |
| `update_vk`    | admin    | Rotate the registered verification key hash and bump the vk epoch.   |

### `verify_proof` ABI

```rust
pub fn verify_proof(
    ctx: Context<VerifyProof>,
    proof_hash: [u8; 32],          // = sha256(proof_bytes || public_inputs)
    proof_bytes: Vec<u8>,          // SP1 Groth16 proof (<= 1024B)
    public_inputs: Vec<u8>,        // first 8 bytes = cluster prefix
    circuit_id: u8,                // 0..=4 (scoring / aggregation / median / sort / ml-inference)
    expected_vk_hash: [u8; 32],    // must equal VerifierConfig.vk_hash
) -> Result<()>
```

### `ProofRecord` PDA layout

```rust
#[account]
pub struct ProofRecord {
    pub circuit_id: u8,            // 0..=4
    pub public_inputs_hash: [u8; 32],
    pub verified_at: i64,
    pub submitter: Pubkey,
    pub vk_epoch_at_verify: u32,
    pub bump: u8,
    pub _padding: [u8; 2],
}
```

Seeds: `[b"proof", &sha256(proof_bytes || public_inputs)]`.

## Build

The default build skips the SP1 verifier linkage for faster iteration on
the data model.  The production artefact must be built with
`--features sp1-verify`:

```bash
cd packages/verifier-program

# Fast iteration (stub verifier — for unit tests only):
anchor build

# Production .so (real SP1 Groth16 verification):
anchor build -- --features sp1-verify
```

After `anchor build`, the IDL lands at `target/idl/veria_verifier.json`
and the program keypair at `target/deploy/veria_verifier-keypair.json`.

## Deploy

```bash
# 1. Print the program ID (deterministic from the keypair):
anchor keys list

# 2. Replace the placeholder in src/lib.rs declare_id!() and Anchor.toml
#    with the printed ID, then rebuild so the .so embeds the right ID.

# 3. Deploy to mainnet (uses ~/.config/solana/id.json by default):
anchor deploy \
    --provider.cluster mainnet \
    --provider.wallet ~/.config/solana/id.json

# 4. Publish the IDL so wallets and explorers can decode tx data:
anchor idl init \
    --provider.cluster mainnet \
    -f target/idl/veria_verifier.json \
    $(anchor keys list | awk '/veria_verifier/ {print $2}')

# 5. Run the migration to initialize VerifierConfig:
anchor migrate \
    --provider.cluster mainnet \
    --provider.wallet ~/.config/solana/id.json
```

`solana program deploy` is the recommended path for retrying failed
deploys — it tolerates RPC flakes better than `anchor deploy`:

```bash
solana program deploy target/deploy/veria_verifier.so \
    --program-id $(anchor keys list | awk '/veria_verifier/ {print $2}') \
    --keypair ~/.config/solana/id.json \
    --url "$MAINNET_RPC_URL"
```

## Test

```bash
# Local validator + TypeScript integration tests:
anchor test --provider.cluster localnet

# Rust unit tests (handlers, hashing, errors):
cargo test --features ""

# Skip the deploy step if the program is already on the local validator:
anchor test --skip-deploy
```

The Rust-side `cargo test` runs the deterministic stub verifier; the
TypeScript tests under `tests/veria-verifier.ts` cover end-to-end Anchor
behaviour including:

1. `initialize` writes a config PDA with the expected fields.
2. `verify_proof` writes a `ProofRecord` and bumps `total_verified`.
3. The sentinel `[0xFF; 4]` proof prefix triggers `InvalidProof`.
4. A duplicate proof hash is rejected (`init` failure).
5. `update_vk` rotates the hash and increments the epoch.
6. `update_vk` from a non-admin signer fails via `has_one = admin`.

## Stack discipline

Solana BPF programs are bound by a 4 KiB per-frame stack budget.  The
verifier follows the standard practice from the Anchor 0.30+ guide:

* Every `Account<..>` field is wrapped in `Box<..>`.
* No `init_if_needed` — all `init`s in the same instruction context are
  single-use, and admin-only flows do not share a context with end-user
  flows.
* The two `Vec<u8>` instruction args (`proof_bytes`, `public_inputs`)
  live on the heap; only the 32-byte `proof_hash` traveler is on the
  stack.

## Threat model (excerpt)

| adversary                | mitigation                                              |
|--------------------------|---------------------------------------------------------|
| Malicious prover         | SP1 + Nova soundness                                    |
| Forged proof submission  | Groth16 vk check + on-chain verifier                    |
| Cross-cluster replay     | `cluster_prefix` enforced in `public_inputs`            |
| Duplicate verify rewrite | `init` (not `init_if_needed`) on `ProofRecord`          |
| Unauthorized vk rotation | `has_one = admin` on `update_vk`                        |
| Compute DoS              | `MAX_PROOF_BYTES` cap + Solana fee market               |

See `docs/security.md` for the full threat model.

## References

The verifier is engineered against the following primary sources.  We
list page numbers rather than re-summarising the cryptography to keep
the README compact.

* **SP1 zkVM**.  Succinct Labs.  *SP1: A Performant, 100% Open-Source,
  Contributor-Friendly zkVM.*  2024.  Used for the proof system whose
  proofs this program verifies.
* **Nova folding**.  Kothapalli, Setty, Tzialla.  *Nova: Recursive
  Zero-Knowledge Arguments from Folding Schemes.*  CRYPTO 2022,
  §4 "Folding Scheme".  Used by the off-chain host to compress N
  sub-proofs into a single recursive instance before submission.
* **SuperNova**.  Kothapalli, Setty.  *SuperNova: Proving Universal
  Machine Executions without Universal Circuits.*  ePrint 2022/1758.
  Used when a batch mixes circuit shapes.
* **Jolt**.  Arasu, Setty et al.  *Jolt: SNARKs for Virtual Machines
  via Lookups.*  2024.  Cited as the lookup-centric path for future vk
  rotations.
* **Recursive SNARKs**.  Bitansky, Chiesa, Tromer.  *Recursive
  composition and bootstrapping for SNARKs and proof-carrying data.*
  STOC 2013.  Theoretical underpinning of the folding approach.
* **Solana Sealevel**.  Solana Labs.  *Sealevel: Parallel transaction
  processing for blockchains.*  2019.  Sealevel write-lock semantics
  motivate the unique-PDA-per-proof design here.

## Layout

```text
packages/verifier-program/
├── Anchor.toml
├── Cargo.toml                      <- workspace (excluded from veria/Cargo.toml)
├── package.json
├── tsconfig.json
├── README.md                       <- this file
├── CONTEXT.md
├── .gitignore
├── migrations/
│   └── deploy.ts                   <- idempotent init + log
├── programs/veria-verifier/
│   ├── Cargo.toml
│   ├── Xargo.toml
│   └── src/
│       ├── lib.rs                  <- declare_id! + #[program]
│       ├── state.rs                <- VerifierConfig + ProofRecord + events
│       ├── errors.rs               <- VerifierError enum
│       ├── utils/
│       │   ├── mod.rs
│       │   └── hash.rs             <- proof_hash / vk_hash / cluster_prefix
│       └── instructions/
│           ├── mod.rs
│           ├── initialize.rs
│           ├── verify_proof.rs     <- the main entry point
│           └── update_vk.rs
├── tests/
│   └── veria-verifier.ts           <- 6 mocha tests against local validator
└── target/
    ├── idl/.gitkeep                <- veria_verifier.json lands here
    └── deploy/.gitkeep             <- veria_verifier.so + keypair land here
```

## License

Apache-2.0.  See the top-level `LICENSE` file for the full text.
