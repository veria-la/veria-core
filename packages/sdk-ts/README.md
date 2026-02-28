# @veria/sdk

TypeScript SDK for VERIA — Solana-native ZK Coprocessor.

> Few dots. Whole truth.

## Install

```bash
npm install @veria/sdk @solana/web3.js
```

## Quick start

```ts
import { VeriaClient, VeriaVerifier } from '@veria/sdk';
import { Connection, Keypair } from '@solana/web3.js';

const client = new VeriaClient({
  apiUrl: 'https://api.veria.fun',
  programId: process.env.VERIA_PROGRAM_ID!,
});

// Submit a folded proof job
const fold = await client.fold({
  circuit: 'scoring',
  input: { scores: [80, 90, 70], weights: [1, 2, 1], count: 3 },
  subProofCount: 100,
});

console.log(`Cost: ${fold.costSol} SOL  Savings: ${fold.savingsPct.toFixed(2)}%`);

// Verify on Solana (cluster auto-detect)
const verifier = new VeriaVerifier({
  connection: new Connection('https://api.mainnet-beta.solana.com'),
  programId: process.env.VERIA_PROGRAM_ID!,
});

const submitter = Keypair.generate().publicKey;
const tx = verifier.buildVerifyTx({
  submitter,
  circuit: 'scoring',
  proofBytes: fold.proofBytes,
  publicInputs: fold.publicInputs,
});
// Sign & send tx with your wallet, then:
const record = await verifier.fetchRecord(
  verifier.recordPda(fold.proofBytes, fold.publicInputs),
);
console.log(record);
```

## Circuit catalog

| ID | Name | Max input | Tests |
|----|------|----------:|------:|
| 1 | Scoring | 64 | 5 |
| 2 | Aggregation | 4096 | 4 |
| 3 | Median | 256 | 5 |
| 4 | Sort | 256 | 4 |
| 5 | ML Inference | 32 | 4 |

## Modules

- `VeriaClient` — HTTP client for the Compute API (`/fold`, `/verify`).
- `VeriaVerifier` — Anchor verifier program instruction builder + PDA derivation + account fetch.
- `CIRCUITS` — circuit registry (numeric id, schema, test count).
- `proofHash`, `publicInputsHash` — SHA-256 helpers matching the on-chain program.

## Cost model

```ts
const c = new VeriaClient();
console.log(c.computeCost(100));
// {
//   subProofs: 100,
//   directTotalSol: 0.5,
//   foldedSol: 0.0001,
//   savingsPct: 99.98
// }
```

## License

Apache-2.0.
