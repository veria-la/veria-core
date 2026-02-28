# aggregation

Single-pass SUM / AVG / MIN / MAX SP1 circuit.

## Input

```rust
// Wire format on SP1 stdin:
//   header: AggregationHeader { count: u32 }
//   data:   u64 * count
```

The host serializes a `Vec<u64>` of length `<= AGG_MAX (4096)`.

## Output

```rust
struct AggregationOutput {
    sum_u128: u128,
    avg_u64:  u64,    // floor(sum / count); 0 when count == 0
    min_u64:  u64,
    max_u64:  u64,
    count:    u32,
}
```

## Tests

Four edge cases live in `packages/zkvm-host/tests/aggregation_test.rs`:

* `random_pile_matches_naive`
* `monotone_sequence`
* `single_element`
* `empty_input`

Run with:

```bash
cargo test -p veria-zkvm-host --test aggregation_test
```

## Use cases

Oracle price aggregation across N feeds, RWA basket value.
