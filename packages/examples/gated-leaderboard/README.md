# Gated Leaderboard

An anti-cheat ranking board where a season's top-N is committed **only** after
a Nova-folded **sort** proof is verified on-chain through the
[VERIA verifier](../../verifier-program) CPI primitive.

## Why a sort proof

The `sort` circuit (`circuit_id = 3`) proves two things at once:

1. The published `top_scores` are the **descending sort** of the season's
   submitted scores.
2. A **permutation witness** establishes that the output multiset equals the
   input multiset.

That permutation argument is the anti-cheat core: you cannot inject an extra
entry, drop a rival, or rewrite a score without breaking multiset equality —
which makes the folded proof unsatisfiable, so `verify_proof` rejects it and
`submit_ranking` aborts.

```text
submitter ──▶ submit_ranking ──CPI──▶ veria_verifier::verify_proof
                   │                         │ (sort proof + permutation witness)
                   └────── on Ok ────────────┘
                   ▼
             Leaderboard PDA { season_id, top_ranks[10], top_scores[10], ... }
```

## Security

- **Tamper-proof set** — multiset preservation from the permutation witness.
- **Local sanity** — `top_scores` are additionally checked non-increasing for
  a precise error before the CPI.
- **Per-season isolation** — board PDA seeded by `[b"board", season_id]`.

## Build

```bash
anchor build          # resolves veria-verifier from git (features = ["cpi"])
```

`Leaderboard { season_id: u32, top_ranks: [Pubkey; 10], top_scores: [u64; 10], proof_record: Pubkey, last_update: i64, bump: u8 }`.
