# veria-cli

VERIA — Solana-native ZK Coprocessor CLI.

> Few dots. Whole truth.

## Install

### npm

```bash
npm install -g veria-cli
veria --help
```

### Homebrew

```bash
brew tap veria-labs/veria
brew install veria
```

## Commands

### `veria fold <input.json> --circuit <id>`

Submit a folded ZK computation job to the Compute API. Returns proof bytes, public inputs, and cost breakdown.

```bash
veria fold input.json --circuit scoring --sub-proofs 100 --output proof.bin
```

### `veria verify --proof-file proof.bin --public-file pub.bin --circuit scoring`

Build the Anchor verify instruction and check whether the proof has already been verified on-chain.

```bash
VERIA_PROGRAM_ID=$VERIA_PROGRAM_ID veria verify \
  --circuit scoring \
  --proof-file proof.bin \
  --public-file pub.bin \
  --cluster mainnet-beta
```

### `veria circuits`

List the five built-in circuits.

```
ID  Name             Max input  Tests  Description
1   Scoring                 64      5  Weighted average over a fixed-length score vector.
2   Aggregation           4096      4  SUM, AVG, MIN, MAX in a single pass.
3   Median                 256      5  Median with sortedness witness.
4   Sort                   256      4  Permutation proof: monotonic, multiset equal.
5   ML Inference            32      4  Fixed-point MLP forward pass.
```

### `veria cost --sub-proofs 100`

Show the SOL cost comparison without submitting a job.

### `veria deploy-circuit --elf circuit.elf --name "My Circuit"`

(Dev preview.) Register a custom circuit ELF with the registry.

## Environment

| Variable | Default |
|----------|---------|
| `VERIA_API_URL` | `https://api.veria.fun` |
| `VERIA_PROGRAM_ID` | (required for `verify`) |
| `VERIA_DEBUG` | unset; set to `1` for full stack traces |

## License

Apache-2.0.
