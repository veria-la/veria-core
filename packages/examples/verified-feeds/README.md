# Verified Feeds

A Pyth-style price oracle where **every** update is gated by a Nova-folded
**median** proof verified on-chain through the [VERIA verifier](../../verifier-program)
CPI primitive. No verified proof, no price write — atomically, in one
transaction.

## How it works

`publish_feed` cross-program-invokes `veria_verifier::cpi::verify_proof` with a
median proof (`circuit_id = 2`). The verifier re-derives
`sha256(proof_bytes || public_inputs)`, checks the cluster prefix and vk epoch,
discharges the SP1 Groth16 pairing check, and writes a `ProofRecord` PDA. Only
if that CPI returns `Ok` does the program copy `price_u64` into the `PriceFeed`
PDA. Any verification failure aborts the whole transaction.

```text
publisher ──▶ publish_feed ──CPI──▶ veria_verifier::verify_proof
                  │                          │ (median proof, ProofRecord PDA)
                  └────── on Ok ─────────────┘
                  ▼
            PriceFeed PDA { price, verified_at, proof_record, publisher }
```

## Security

- **Authority** — a feed is bound to its first publisher; only that key may
  update it (`Unauthorized`).
- **Staleness** — observations referencing a slot older than
  `MAX_STALENESS_SLOTS` (~3 min) or in the future are rejected.
- **Soundness** — price integrity reduces entirely to the median circuit; a
  forged median can never satisfy the folded proof.

## Build

```bash
anchor build          # resolves veria-verifier from git (features = ["cpi"])
```

`PriceFeed { price: u64, verified_at: i64, proof_record: Pubkey, publisher: Pubkey, bump: u8 }`,
PDA seeds `[b"feed", feed_id]`.
