# ML-Gated Mint

An NFT mint conditioned on a **verified MLP classification**. The mint succeeds
only when a Nova-folded `ml-inference` proof — verified on-chain through the
[VERIA verifier](../../verifier-program) CPI primitive — shows that a fixed
classifier assigns the minter's private features to the gate's target class.

## How it works

The proof's public inputs are, after the verifier's 8-byte cluster prefix,
**32 feature elements + 4 logit elements** (each a 32-byte field element).
`mint_if_verified`:

1. Parses the 4 logits and computes `argmax`.
2. CPIs into `veria_verifier::cpi::verify_proof` (`circuit_id = 4`) to discharge
   the folded MLP execution proof.
3. Asserts `argmax(logits) == target_class`, then advances the gate's
   `minted_count` against `max_supply`.

```text
minter ──▶ mint_if_verified ──CPI──▶ veria_verifier::verify_proof
               │  argmax==target          │ (ml-inference proof)
               └──────── on Ok ───────────┘
               ▼
         MintGate PDA { collection, target_class, minted_count, max_supply }
```

## Security

- **Honest features** — a prover who lies about their feature vector cannot
  satisfy the folded proof, so `verify_proof` fails.
- **No class shopping** — `target_class` is pinned by the instruction; the
  verifier attests the classification, so a prover cannot mint into an
  arbitrary class.
- **Supply** — `minted_count < max_supply` enforced per collection.

## Build

```bash
anchor build          # resolves veria-verifier from git (features = ["cpi"])
```

`MintGate { collection: Pubkey, target_class: u8, minted_count: u64, max_supply: u64, bump: u8 }`,
PDA seeds `[b"gate", collection]`.
