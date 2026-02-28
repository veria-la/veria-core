# packages/circuits — context

## Role

Five SP1 guest programs that ship in VERIA v0.1.0.  Each guest is compiled by
`cargo prove build` (Succinct Labs toolchain) to a RISC-V ELF; the host
embeds those ELFs at build time and dispatches to the right one based on the
`circuit_id` request field.

When the toolchain is not installed the host falls back to a deterministic
in-process reference implementation that lives in
`packages/zkvm-host/src/circuits.rs`.  The reference path is what the test
suite drives, so it is the source of truth — every guest is required to
agree with its reference twin.

## Layout

```
circuits/
  CONTEXT.md             this file
  scoring/
    program/
      Cargo.toml
      src/main.rs        SP1 guest
    README.md
  aggregation/
    program/...          (same shape)
    README.md
  median/
    program/...
    README.md
  sort/
    program/...
    README.md
  ml-inference/
    program/...
    README.md
```

## Circuit map

| id | name           | input size       | output                              | tests |
|----|----------------|------------------|-------------------------------------|------:|
| 1  | scoring        | 64 + 64 + 1      | (u64, u64)                          | 6 |
| 2  | aggregation    | up to 4096 u64   | (u128, u64, u64, u64, u32)          | 6 |
| 3  | median         | 256 + 256 + 256  | (u64, [u8;32], u32)                 | 7 |
| 4  | sort           | 256 + 256 + 256  | ([u8;32], [u8;32], u32)             | 6 |
| 5  | ml-inference   | 32+layers+biases | ([i32;4], [u8;32])                  | 5 |

Tests live alongside the host crate (`packages/zkvm-host/tests/`) because
they cross-check the guest's reference twin against the prover wrapper.

## Determinism contract

Every guest:

* uses `#![no_main]` + `sp1_zkvm::entrypoint!(main)`;
* reads input via `sp1_zkvm::io::read::<T>()`;
* commits output via `sp1_zkvm::io::commit(&output)`;
* does no floating-point arithmetic;
* branches only on public size fields (`count`, layer dims);
* implements every witness check with explicit `assert!` (panic on
  violation — soundness comes from SP1, not the assert message).

## Folding

After the host runs `n` of these guests it folds them into a single
[`FoldedProof`](../zkvm-host/src/folding.rs) via Nova for homogeneous batches
or SuperNova for mixed batches.  The accumulator hash is what the Anchor
verifier (Solana mainnet) consumes as a public input.

## Adding a sixth circuit

1. Create `packages/circuits/<name>/program/Cargo.toml` mirroring the
   existing ones.
2. Implement the guest in `program/src/main.rs` with `#![no_main]` and
   `sp1_zkvm::entrypoint!(main)`.
3. Add the host reference twin to `packages/zkvm-host/src/circuits.rs`
   (`ref_<name>`).
4. Add the input/output schemas to `packages/zkvm-host/src/inputs.rs`.
5. Add a new variant to `CircuitId`, register it in `ALL`, and extend
   `SpProver::run_json`.
6. Add `tests/<name>_test.rs` with at least four cases.
7. Add `<name>` to `CIRCUITS` in `packages/zkvm-host/build.rs`.
8. Add the circuit id to `packages/verifier-program/programs/veria-verifier/src/lib.rs`.
9. Add SDK and CLI bindings (`packages/sdk-ts/src/circuits.ts`,
   `packages/cli/src/commands/fold.ts`).
