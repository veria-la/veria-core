# median

Median with a sortedness + permutation witness.

## Input

```rust
struct MedianInput {
    raw:    [u64; 256],
    sorted: [u64; 256],
    perm:   [u16; 256],   // perm[i] = original index of sorted[i] in raw
    count:  u32,          // <= 256
}
```

## Output

```rust
struct MedianOutput {
    median:        u64,
    sorted_commit: [u8; 32],  // SHA-256 over sorted[..count] big-endian
    count:         u32,
}
```

## Witness checks (all panic on violation)

1. `sorted[i] >= sorted[i-1]` for every `i in 1..count` — sortedness.
2. `perm[i] in 0..count` and is unique — permutation validity.
3. `sorted[i] == raw[perm[i]]` for every `i in 0..count` — sorted is a
   permutation of raw.

For even counts the lower median is returned (no floating-point average).

## Tests

Five edge cases in `packages/zkvm-host/tests/median_test.rs`:

* `odd_count`
* `even_count_takes_lower_middle`
* `all_duplicates`
* `already_monotone`
* `reverse_input`

Plus a witness-rejection test.

```bash
cargo test -p veria-zkvm-host --test median_test
```

## Use cases

Honest oracle median, Pyth-style aggregation, anti-manipulation feeds.
