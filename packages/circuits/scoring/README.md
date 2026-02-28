# scoring

Weighted-average SP1 circuit.

## Layout

```
scoring/
  program/        SP1 guest program (RISC-V target)
  README.md       this file
```

## Input

```rust
struct ScoringInput {
    scores:  [u64; 64],   // padded with zeros
    weights: [u64; 64],   // fixed-point, scale 2^32
    count:   u32,         // <= 64
}
```

## Output

```rust
struct ScoringOutput {
    weighted_avg_fp: u64, // fixed-point, scale 2^32
    total_weight:    u64,
}
```

## Semantics

```
weighted_sum = sum_i scores[i] * weights[i]   (i in 0..count, u128 intermediates)
total_weight = sum_i weights[i]
weighted_avg_fp = floor(weighted_sum / total_weight)   (0 when total_weight == 0)
```

`count > 64` is clamped to `64` in the guest as a defense-in-depth check; the
host rejects oversized inputs before they reach the guest.

## Tests

Five edge cases live in `packages/zkvm-host/tests/scoring_test.rs`:

* `empty_count_returns_zero`
* `single_element_passes_through`
* `all_equal_weights_is_arithmetic_mean`
* `weighted_mix_skews_toward_higher_weight`
* `overflow_edge_uses_u128_intermediates`

plus a simulator-vs-reference cross-check.  Run with:

```bash
cargo test -p veria-zkvm-host --test scoring_test
```

## Use cases

Credit scoring, reputation aggregation, AI confidence weighting.
