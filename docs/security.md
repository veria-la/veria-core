# Security

## Trust model

VERIA inherits soundness from SP1 (Succinct Labs) and the Nova folding scheme. Concretely:

- **SP1 zkVM soundness**: a passing proof implies the guest program executed exactly as specified on the committed inputs. The prover cannot forge a passing proof for a different output.
- **Nova folding soundness**: folded instances are sound under the SXDH (Symmetric External Diffie-Hellman) assumption used by the Pedersen commitments in Nova. See Kothapalli, Setty, Tzialla (2022) §4.
- **On-chain verifier soundness**: the Anchor verifier program is the trusted root. Once `verify_proof` returns `Ok(())` and writes the `ProofRecord` PDA, the result is final.

## Threat model

| Adversary | Goal | Mitigation |
|-----------|------|------------|
| Malicious prover | Forge a passing proof for wrong output | SP1 + Nova soundness |
| Malicious Compute API operator | Serve invalid proofs | On-chain verifier rejects; client can also verify locally via SDK |
| Malicious caller | Spam Anchor verifier | Compute-unit cost + Solana fee market |
| MEV / front-run | Steal verified result | Result is public on-chain; PDA derivation is deterministic from proof hash |
| Replay across networks | Reuse devnet proof on mainnet | Verifier hashes the cluster genesis into the public inputs |

## Boundary checks

### Client-side (`apps/web/`)

- **No secret in `NEXT_PUBLIC_*`.** every third-party provider key all live server-side only.
- **Wallet adapter uses public RPC only** (`https://api.mainnet-beta.solana.com`). RPC provider usage is proxied through a same-origin Route Handler.
- **CSP**: `default-src 'self'`, `connect-src` includes only the API origin and public RPC.
- **Build-time leak check**: a generic `api-key=` / `provider-token=` sweep across `.next/` must return zero matches.

### API (the private Compute API service)

- **CORS**: explicit allowlist (`veria.fun`, `www.veria.fun`, plus the deploy preview domain). No wildcard. `allow_credentials=True`.
- **Rate limit**: per-IP token bucket (50 req/min default) on `/fold`. SP1 host time is expensive.
- **Input bounds**: every circuit input is bounds-checked at the FastAPI handler before reaching the host (size cap, type, max count).

### Anchor verifier (`packages/verifier-program/`)

- **Stack discipline**: heap-allocate large proof bytes (`Box<Vec<u8>>`) to stay under Solana's 4 KiB stack limit.
- **Compute units**: proof verification is benchmarked at <200K CUs (Solana 1.4M limit).
- **PDA seeds**: `[b"proof", &proof_hash]`. The proof hash is `sha256(proof_bytes || public_inputs)` so distinct verifications never collide.
- **Re-init guard**: `ProofRecord` uses `init` not `init_if_needed`. A repeat verification with the same hash is rejected, preventing replay rewriting.

### CLI / SDK (`packages/cli/`, `packages/sdk-ts/`)

- **Mainnet by default**, no devnet fallback. Devnet usage requires `--cluster devnet` explicit flag.
- **Keypair handling**: CLI reads keypair from `~/.config/solana/id.json` only when explicitly invoked with `--sign`. The default `verify` flow reads results without signing.
- **Proof hash check**: before sending the on-chain tx, the CLI recomputes the proof hash locally to make sure the cached blob has not been tampered with.

## Auditing

Pre-mainnet, the following must complete:

- [ ] Internal review of all five circuits — input bounds, fixed-point overflow, deterministic loop structure.
- [ ] Nova folding adapter review — instance well-formedness, error path coverage.
- [ ] Anchor verifier review — stack usage, CU budget, PDA seeds, re-init guard.
- [ ] External audit (Trail of Bits / Halborn / OtterSec) once tier-3 budget allocated.

## Disclosure

Found a vulnerability? Email security at the project domain. Bounty:

| Severity | Reward (in $VERIA equivalent) |
|----------|-------------------------------|
| Critical (forged proof, drained funds) | up to 100,000 USD |
| High (DoS of verifier, double-verify) | up to 25,000 USD |
| Medium (input bound bypass) | up to 5,000 USD |
| Low (CLI / SDK issue) | up to 1,000 USD |

Disclosure window: 90 days from confirmed receipt.
