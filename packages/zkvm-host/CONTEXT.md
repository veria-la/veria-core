# packages/zkvm-host — context

## Role

Off-chain proving service for VERIA. Wraps SP1 SDK v3 (Succinct Labs, 2024),
applies a Nova-style folding accumulator (Kothapalli, Setty, Tzialla, CRYPTO
2022), and exposes both a CLI (`veria-host`) and an Axum HTTP bridge that the
FastAPI Compute API in the private Compute API service calls into.

For heterogeneous batches the folding adapter falls back to a SuperNova-style
augmented accumulator (Kothapalli, Setty, ePrint 2022/1758) so circuit
selectors are carried through the IVC chain.

## Layout

```
zkvm-host/
  Cargo.toml             workspace member
  build.rs               SP1 guest ELF build trigger (gated by VERIA_BUILD_ELFS=1)
  src/
    lib.rs               crate root + re-exports
    error.rs             HostError + stable error codes
    inputs.rs            shared input/output schemas for the five circuits
    circuits.rs          CircuitId enum + host reference implementations
    prover.rs            SpProver: real SP1 + simulator fallback
    folding.rs           FoldingAdapter + FoldedProof
    api.rs               Axum HTTP bridge (`/healthz`, `/circuits`, `/fold`)
    main.rs              CLI binary
  tests/
    integration_test.rs  all five circuits + fold round-trip
    scoring_test.rs
    aggregation_test.rs
    median_test.rs
    sort_test.rs
    ml_inference_test.rs
  CONTEXT.md             this file
```

## Build

* Plain dev path (no RISC-V toolchain required):

  ```bash
  cargo check  -p veria-zkvm-host
  cargo build  -p veria-zkvm-host --release
  cargo test   -p veria-zkvm-host
  ```

  In this mode `build.rs` does not invoke `sp1-build`, so `SpProver` runs
  every circuit through the in-process reference path defined in
  `circuits.rs`.  All cross-checks (`opts.cross_check = true`) still apply.

* Real SP1 path (requires `cargo prove` to be installed):

  ```bash
  VERIA_BUILD_ELFS=1 cargo build -p veria-zkvm-host --release
  veria-host prove --circuit scoring --input fixtures/scoring.json --real
  ```

## HTTP bridge

`veria-host serve --addr 127.0.0.1:8088` brings up the Axum router. The
FastAPI proxy lives at `the Compute API source` and forwards
`POST /fold` payloads to this service.

## Determinism

Every public path in this crate is deterministic given the same input. The
simulator output of `SpProver::run_*` is byte-identical to what the on-chain
verifier consumes as public values, which is the property the integration
test asserts.

## Next steps (out of scope here)

1. Install `cargo-prove` and run `VERIA_BUILD_ELFS=1 cargo build` on the CI
   runner to embed real SP1 ELFs.
2. Wire the private Compute API service to call this bridge instead of stubbing.
3. Land `packages/verifier-program/` (Anchor verifier) — `FoldedProof` is the
   shape it consumes via `verify_proof(proof_bytes, public_inputs, circuit_id)`.
