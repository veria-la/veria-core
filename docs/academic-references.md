# Academic References

VERIA implements three lines of research and packages them into a Solana-native service. This file enumerates the primary sources.

## zkVM

- **Succinct Labs, "SP1: A Performant, 100% Open-Source, Contributor-Friendly zkVM" (2024).** SP1 implements a RISC-V zkVM with a Plonk-style commitment scheme. VERIA uses `sp1-sdk v3` as the host and `sp1-zkvm v3` as the guest runtime.
- **Risc0, "Zero-Knowledge Virtual Machine" (Bruce, Schneider et al. 2023).** Alternative RISC-V zkVM. VERIA's prover backend is pluggable; Risc0 is a planned secondary backend.
- **Arasu, Setty, et al., "Jolt: SNARKs for Virtual Machines via Lookups" (2024).** Lookup-centric VM with potential SP1 throughput parity. Tracked for roadmap inclusion.

## Folding schemes

- **Kothapalli, Setty, Tzialla, "Nova: Recursive Zero-Knowledge Arguments from Folding Schemes" (CRYPTO 2022).** Original Nova paper. Defines the folding scheme that compresses two R1CS instances into one. `packages/zkvm-host/src/folding.rs` implements the standard Nova adapter.
- **Kothapalli, Setty, "SuperNova: Proving universal machine executions without universal circuits" (ePrint 2022/1758).** Extends Nova to non-uniform IVC — required when a single batch mixes different circuits.
- **Eagen, Gabizon, "Origami: A High-Performance Mechanism for Differentiating Data in SNARK-friendly Hashing" (2023).** Used in some Nova accelerations; reference for future Poseidon optimization.

## Recursive SNARKs / IVC

- **Bitansky, Chiesa, Tromer, "Recursive Composition and Bootstrapping for SNARKs" (STOC 2013).** Foundational paper. IVC = Incremental Verifiable Computation. Nova generalizes the IVC construction with folding.
- **Valiant, "Incrementally Verifiable Computation or Proofs of Knowledge Imply Time/Space Efficiency" (TCC 2008).** The original IVC definition.

## Solana / runtime

- **Yakovenko, "Solana: A new architecture for a high performance blockchain" (whitepaper 2017).** Proof-of-History + Sealevel parallel runtime. The latter is why distinct `ProofRecord` PDAs do not contend.
- **Anchor framework documentation, v0.31 (2025).** The verifier program is written in Anchor.

## Verifiable ML

- **Lee, Kim, Lee, et al., "vCNN: Verifiable Convolutional Neural Network Based on zk-SNARKs" (2020).** Early verifiable inference. VERIA's `ml-inference` circuit is a simpler MLP analog.
- **Modulus Labs, "RemainderNet" (2024).** GKR-based verifiable inference. Used as benchmark for circuit-size comparison.

## Related coprocessor work

- **Axiom, "On-chain access to historical Ethereum state via zk-SNARKs" (2023).** EVM zk coprocessor. VERIA's market positioning is "Axiom for Solana."
- **RiscZero Bonsai (2023).** Hosted prover service. VERIA's Compute API plays a similar role with on-chain Solana verification.

## How VERIA composes these

```
guest program (Rust, no_std)
    -> sp1-zkvm v3 runtime (SP1)
        -> sub-proof (one per fold)
            -> Nova IVC fold (n times)
                -> compressed final proof
                    -> sp1_verifier on-chain (Anchor)
                        -> ProofRecord PDA on Solana mainnet
```

Each arrow is a known, audited construction. VERIA's contribution is the engineering integration into a developer-facing service (Compute API, SDK, CLI) with a permissionless on-chain marketplace.
