# sort

Permutation-proof SP1 circuit.

## Input

```rust
struct SortInput {
    input:  [u64; 256],
    sorted: [u64; 256],
    perm:   [u16; 256],   // unused in compute; useful when external code needs the mapping
    count:  u32,          // <= 256
}
```

## Output

```rust
struct SortOutput {
    sorted_commit: [u8; 32],  // SHA-256 over sorted[..count] big-endian
    input_commit:  [u8; 32],  // SHA-256 over input[..count] big-endian
    count:         u32,
}
```

## Witness checks

1. `sorted[i] >= sorted[i-1]` for every `i in 1..count` — monotonicity.
2. Running-product multiset equality:
   `prod (r + input[i]) == prod (r + sorted[i])`, with `r` derived from the
   commitments via Fiat-Shamir (`derive_challenge`). Both products are reduced
   mod 2^64. This is the same shape as Plookup (Gabizon, Williamson 2020).

## Tests

Four edge cases in `packages/zkvm-host/tests/sort_test.rs`:

* `random_input`
* `with_duplicates`
* `presorted_input`
* `reverse_input`

Plus rejection of tampered sorted vectors.

```bash
cargo test -p veria-zkvm-host --test sort_test
```

## Use cases

Verified leaderboards, priority queues, tournament brackets.
