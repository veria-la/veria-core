# Circuit Reference

VERIA v0.1.0 ships five circuits. Each circuit is a SP1 guest program in `packages/circuits/<name>/program/src/main.rs`. The host wraps it in a Nova folding scheme for batch submission.

## Circuit Authoring Conventions

- **Determinism**: no syscalls beyond `sp1_zkvm::io::read` / `commit`. No floating point. No allocations after init.
- **Bounded input**: each circuit declares a max input length via a const. Larger inputs are rejected at the host layer.
- **Fixed-point**: floats are encoded as `i64` with scale `2^32`. Multiplication uses `i128` intermediates to avoid overflow.
- **Constant-time loops**: branching on secret data is forbidden — branch on input length only.

## 1. `scoring`

Weighted average of a fixed-length score vector.

**Input**
```rust
struct ScoringInput {
    scores: [u64; 64],   // padded with zeros
    weights: [u64; 64],  // fixed-point, scale 2^32
    count: u32,          // <= 64
}
```

**Output**
```rust
struct ScoringOutput {
    weighted_avg_fp: u64,  // fixed-point
    total_weight: u64,
}
```

**Use cases**: credit scoring, reputation aggregation, AI confidence weighting.

**Test count**: 5 — empty, single, all-equal, weighted, overflow-edge.

## 2. `aggregation`

SUM / AVG / MIN / MAX in one pass, all four committed.

**Input**: `Vec<u64>` up to 4096 entries.

**Output**: `(sum_u128, avg_u64, min_u64, max_u64)`.

**Note**: `avg = sum / n` with floor division — no rounding mode required to be deterministic across hosts.

**Use cases**: oracle price aggregation across N feeds, RWA basket value.

**Test count**: 4 — random, monotone, single, empty.

## 3. `median`

Median with a sortedness witness. The guest receives both the unsorted input and a host-provided sorted permutation, and proves (a) sorted output is a permutation of input, (b) sorted output is monotonic, (c) median is the middle element.

**Input**
```rust
struct MedianInput {
    raw: [u64; 256],
    sorted: [u64; 256],
    perm: [u16; 256],
    count: u32,
}
```

**Output**: `u64` median.

**Use cases**: honest oracle median, Pyth-style aggregation, anti-manipulation feeds.

**Test count**: 5 — odd, even, all-duplicates, monotone, reverse.

## 4. `sort`

Permutation proof: output is monotonic, multiset equals input multiset.

**Input**
```rust
struct SortInput {
    input: [u64; 256],
    sorted: [u64; 256],
    perm: [u16; 256],
    count: u32,
}
```

**Output**: `[u64; 256]` (sorted), implicitly via commitment.

**Verification check**: for `i in 1..count`, `sorted[i] >= sorted[i-1]`. Multiset check uses a running product hash (Schwartz-Zippel style).

**Use cases**: verified leaderboards, priority queues, tournament brackets.

**Test count**: 4 — random, duplicates, presorted, reverse.

## 5. `ml-inference`

Fixed-point MLP forward pass. Two hidden layers, ReLU activation.

**Input**
```rust
struct MlInput {
    features: [i32; 32],          // input vector, fixed-point scale 2^16
    w1: [[i32; 32]; 16],          // hidden layer 1 weights
    b1: [i32; 16],
    w2: [[i32; 16]; 8],           // hidden layer 2 weights
    b2: [i32; 8],
    w3: [[i32; 8]; 4],            // output layer weights
    b3: [i32; 4],
}
```

**Output**: `[i32; 4]` (logits, fixed-point).

**Note**: weights are public inputs — the prover commits to the model weights as part of the proof so a verifier can confirm "this prediction came from this model."

**Use cases**: verifiable AI agent decisions, on-chain ML classification, anti-fraud scoring.

**Test count**: 4 — zero-weights (identity), saturated ReLU, deep negative, positive.

## Folding Across Circuits

Nova folding works on instances of the **same** R1CS shape. For batches that mix circuits, the host switches to SuperNova augmented circuit, which carries a circuit selector through the IVC chain. Cost grows linearly with the number of distinct circuits in the batch, not the number of total folds.

## Adding a Circuit

1. Create `packages/circuits/<name>/program/Cargo.toml` with `sp1-zkvm = "3.0.0"`.
2. Write the guest program in `src/main.rs`. Use `sp1_zkvm::entrypoint!(main)`.
3. Add a builder entry in `packages/zkvm-host/build.rs` so the ELF is compiled at host build time.
4. Add a circuit ID constant in `packages/verifier-program/programs/veria-verifier/src/lib.rs`.
5. Add SDK type bindings in `packages/sdk-ts/src/circuits.ts`.
6. Add CLI command flag mapping in `packages/cli/src/commands/fold.ts`.
7. Add at least four tests covering edge cases.

## Test Header

The web header displays a live count read from the test JSON output:

> `5/5 circuits · 22/22 circuit tests · SP1 v3 · Solana mainnet`

If any circuit test fails, the header turns red and the visualizer is disabled.
