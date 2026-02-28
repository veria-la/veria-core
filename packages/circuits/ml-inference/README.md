# ml-inference

Fixed-point MLP forward-pass SP1 circuit.

## Topology

```
features [32]
  -> w1 [16 x 32] + b1 [16]   -> ReLU -> h1 [16]
  -> w2 [8 x 16]  + b2 [8]    -> ReLU -> h2 [8]
  -> w3 [4 x 8]   + b3 [4]              -> logits [4]
```

Scale: `2^16` (i32 storage; i64 intermediate; right shift by 16 after each
multiplication; saturating cast back to i32 at the end of each layer).

## Input

```rust
struct MlInput {
    features: [i32; 32],
    w1: [[i32; 32]; 16], b1: [i32; 16],
    w2: [[i32; 16]; 8],  b2: [i32; 8],
    w3: [[i32; 8];  4],  b3: [i32; 4],
}
```

## Output

```rust
struct MlOutput {
    logits:       [i32; 4],
    model_commit: [u8; 32],   // SHA-256 over w1 || b1 || w2 || b2 || w3 || b3
}
```

The model commitment lets the on-chain verifier pin "this prediction came
from this model" without exposing the weights themselves on-chain.

## Tests

Four edge cases in `packages/zkvm-host/tests/ml_inference_test.rs`:

* `zero_weights_yields_zero_logits`
* `saturated_relu_clips_at_zero`
* `deep_negative_path`
* `positive_path_passes_through`

Plus a model-commitment determinism check.

```bash
cargo test -p veria-zkvm-host --test ml_inference_test
```

## Use cases

Verifiable AI agent decisions, on-chain ML classification, anti-fraud scoring.
