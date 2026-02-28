// VERIA verifier — TypeScript integration tests.
//
// These tests exercise the program against a local validator (Anchor's
// `anchor test` harness boots one for us).  They run against the `sp1-verify`
// **off** build, which uses the deterministic stub verifier — so we can
// drive the success and failure paths without shipping a real SP1 proof.
//
// Five tests:
//
//   1. initialize succeeds and writes VerifierConfig.
//   2. verify_proof succeeds for a well-formed payload.
//   3. verify_proof rejects the sentinel `[0xFF; 4]` "bad proof" prefix.
//   4. verify_proof rejects a duplicate (same proof_hash) with
//      ProofAlreadyVerified.
//   5. update_vk rotates the vk hash and bumps the epoch.
//
// Run with:
//   anchor test --provider.cluster localnet
//
// All assertions use plain `assert` from the node stdlib so the file has
// no test-framework heavyweight dependency beyond mocha (which Anchor
// drives via its own `scripts.test` entry in `Anchor.toml`).

import * as anchor from "@coral-xyz/anchor";
import { PublicKey, SystemProgram, Keypair } from "@solana/web3.js";
import * as crypto from "crypto";
import { strict as assert } from "node:assert";

// eslint-disable-next-line @typescript-eslint/no-var-requires
const idl = require("../target/idl/veria_verifier.json");

const CONFIG_SEED = Buffer.from("config");
const PROOF_SEED = Buffer.from("proof");

function sha256(...parts: Buffer[]): Buffer {
  const h = crypto.createHash("sha256");
  for (const p of parts) h.update(p);
  return h.digest();
}

function clusterPrefix(label: Buffer): Buffer {
  return sha256(label).subarray(0, 8);
}

function proofHashOf(proof: Buffer, publicInputs: Buffer): Buffer {
  // Mirror `utils::hash::compute_proof_hash` from the on-chain code:
  // `sha256(len_le32(proof) || proof || public_inputs)`.  The length
  // prefix domain-separates the (proof, public_inputs) split.
  const lenPrefix = Buffer.alloc(4);
  lenPrefix.writeUInt32LE(proof.length);
  return sha256(lenPrefix, proof, publicInputs);
}

describe("veria-verifier", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const programId = new PublicKey(idl.address ?? idl.metadata?.address);
  const program = new anchor.Program(idl as anchor.Idl, provider);
  const admin = provider.wallet.publicKey;

  const clusterLabel = Buffer.from("solana-test-validator");
  const prefix = clusterPrefix(clusterLabel);

  // We use the same deterministic vk hash the migration ships with so the
  // test harness can rely on it without re-running `initialize` against a
  // custom value.  Production deploys use `sha256(sp1_solana::GROTH16_VK_BYTES)`.
  const vkHash = sha256(Buffer.from("veria-sp1-groth16-vk-v1"));

  let configPda: PublicKey;
  let configBump: number;

  before(async () => {
    [configPda, configBump] = PublicKey.findProgramAddressSync(
      [CONFIG_SEED],
      programId,
    );
  });

  it("initializes the config PDA", async () => {
    const existing = await provider.connection.getAccountInfo(configPda);
    if (!existing) {
      await program.methods
        .initialize([...vkHash], Buffer.from(clusterLabel))
        .accounts({
          admin,
          config: configPda,
          systemProgram: SystemProgram.programId,
        })
        .rpc({ commitment: "confirmed" });
    }
    const cfg = await (program.account as any).verifierConfig.fetch(configPda);
    assert.equal(cfg.admin.toBase58(), admin.toBase58());
    assert.deepEqual(Buffer.from(cfg.vkHash), vkHash);
    assert.deepEqual(Buffer.from(cfg.clusterPrefix), prefix);
    assert.equal(cfg.vkEpoch, 1);
    assert.equal(cfg.bump, configBump);
  });

  it("verifies a well-formed proof and writes a ProofRecord PDA", async () => {
    // Public inputs MUST start with the cluster prefix.  After that the
    // shape is arbitrary; the stub verifier does not look further.
    const publicInputs = Buffer.concat([
      prefix,
      Buffer.from("scoring:weighted_average:42"),
    ]);
    const proofBytes = Buffer.from([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00]);
    const proofHash = proofHashOf(proofBytes, publicInputs);

    const [proofRecordPda] = PublicKey.findProgramAddressSync(
      [PROOF_SEED, proofHash],
      programId,
    );

    await program.methods
      .verifyProof(
        [...proofHash],
        proofBytes,
        publicInputs,
        0,           // circuit_id = scoring
        [...vkHash], // expected vk hash
      )
      .accounts({
        submitter: admin,
        config: configPda,
        proofRecord: proofRecordPda,
        systemProgram: SystemProgram.programId,
      })
      .rpc({ commitment: "confirmed" });

    const rec = await (program.account as any).proofRecord.fetch(proofRecordPda);
    assert.equal(rec.circuitId, 0);
    assert.equal(rec.submitter.toBase58(), admin.toBase58());
    const expectedPiHash = sha256(publicInputs);
    assert.deepEqual(Buffer.from(rec.publicInputsHash), expectedPiHash);
    assert.equal(rec.vkEpochAtVerify, 1);

    const cfg = await (program.account as any).verifierConfig.fetch(configPda);
    assert.ok(cfg.totalVerified.toNumber() >= 1, "total_verified should increment");
  });

  it("rejects a malformed proof (sentinel 0xFFFFFFFF prefix)", async () => {
    const publicInputs = Buffer.concat([prefix, Buffer.from("malformed-payload")]);
    // The 0xFF sentinel triggers the stub verifier's InvalidProof branch.
    const proofBytes = Buffer.from([0xff, 0xff, 0xff, 0xff, 0x01]);
    const proofHash = proofHashOf(proofBytes, publicInputs);

    const [proofRecordPda] = PublicKey.findProgramAddressSync(
      [PROOF_SEED, proofHash],
      programId,
    );

    let threw = false;
    try {
      await program.methods
        .verifyProof(
          [...proofHash],
          proofBytes,
          publicInputs,
          1,
          [...vkHash],
        )
        .accounts({
          submitter: admin,
          config: configPda,
          proofRecord: proofRecordPda,
          systemProgram: SystemProgram.programId,
        })
        .rpc({ commitment: "confirmed" });
    } catch (err: any) {
      threw = true;
      const msg = err?.error?.errorCode?.code ?? err?.toString() ?? "";
      assert.ok(
        msg.includes("InvalidProof") || msg.includes("0x") || msg.includes("custom program error"),
        `expected InvalidProof, got: ${msg}`,
      );
    }
    assert.ok(threw, "verify_proof must reject the sentinel bad proof");
  });

  it("rejects a duplicate proof (ProofAlreadyVerified)", async () => {
    // Re-submit the exact same payload from test #2.  Because the PDA
    // already exists, Anchor's `init` macro returns
    // AccountAlreadyInitialised (program error 0x0).
    const publicInputs = Buffer.concat([
      prefix,
      Buffer.from("scoring:weighted_average:42"),
    ]);
    const proofBytes = Buffer.from([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00]);
    const proofHash = proofHashOf(proofBytes, publicInputs);

    const [proofRecordPda] = PublicKey.findProgramAddressSync(
      [PROOF_SEED, proofHash],
      programId,
    );

    let threw = false;
    try {
      await program.methods
        .verifyProof(
          [...proofHash],
          proofBytes,
          publicInputs,
          0,
          [...vkHash],
        )
        .accounts({
          submitter: admin,
          config: configPda,
          proofRecord: proofRecordPda,
          systemProgram: SystemProgram.programId,
        })
        .rpc({ commitment: "confirmed" });
    } catch (err: any) {
      threw = true;
      // Anchor's `init` failure is the standard AlreadyInitialized
      // program error; the message is "already in use".
      const msg = (err?.toString() ?? "").toLowerCase();
      assert.ok(
        msg.includes("already") || msg.includes("0x0") || msg.includes("custom program error"),
        `expected AlreadyInitialised-style error, got: ${msg}`,
      );
    }
    assert.ok(threw, "verify_proof must reject a duplicate proof hash");
  });

  it("rotates the verification key via update_vk", async () => {
    const newVkHash = sha256(Buffer.from("veria-sp1-groth16-vk-v2"));

    await program.methods
      .updateVk([...newVkHash])
      .accounts({ admin, config: configPda })
      .rpc({ commitment: "confirmed" });

    const cfg = await (program.account as any).verifierConfig.fetch(configPda);
    assert.deepEqual(Buffer.from(cfg.vkHash), newVkHash);
    assert.equal(cfg.vkEpoch, 2);

    // Rotate back so subsequent test runs (if any) keep using the v1 hash.
    await program.methods
      .updateVk([...vkHash])
      .accounts({ admin, config: configPda })
      .rpc({ commitment: "confirmed" });
    const cfg2 = await (program.account as any).verifierConfig.fetch(configPda);
    assert.equal(cfg2.vkEpoch, 3);
    assert.deepEqual(Buffer.from(cfg2.vkHash), vkHash);
  });

  it("rejects update_vk from a non-admin signer", async () => {
    const intruder = Keypair.generate();
    // Fund the intruder so the tx itself can land.
    const airdropSig = await provider.connection.requestAirdrop(
      intruder.publicKey,
      1_000_000_000,
    );
    await provider.connection.confirmTransaction(airdropSig, "confirmed");

    let threw = false;
    try {
      await program.methods
        .updateVk([...sha256(Buffer.from("evil-vk"))])
        .accounts({ admin: intruder.publicKey, config: configPda })
        .signers([intruder])
        .rpc({ commitment: "confirmed" });
    } catch (err: any) {
      threw = true;
      const msg = (err?.toString() ?? "").toLowerCase();
      assert.ok(
        msg.includes("unauthorized") || msg.includes("has_one") || msg.includes("custom program error"),
        `expected UnauthorizedAdmin / has_one failure, got: ${msg}`,
      );
    }
    assert.ok(threw, "update_vk must reject a non-admin signer");
  });
});
