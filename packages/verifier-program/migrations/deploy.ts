// VERIA verifier — Anchor migration (idempotent across devnet and mainnet).
//
// Anchor runs this script after `anchor deploy` finishes pushing the .so
// onto the cluster.  The script:
//
//   1. Loads the deployer wallet from `~/.config/solana/id.json`.
//   2. Resolves the VerifierConfig PDA from `[b"config"]`.
//   3. If the PDA does NOT exist yet, calls `initialize` with the bundled
//      SP1 Groth16 vk hash and the cluster label appropriate for the
//      provider (mainnet/devnet/test).
//   4. Logs the resulting program ID and PDA so the operator can copy them
//      into the `_DIRECTION.md` env var manifest.
//
// The migration is idempotent: re-running it against an already-initialised
// program is a no-op (the PDA fetch succeeds, the script logs the current
// vk hash and exits).
//
// Usage:
//   anchor migrate --provider.cluster mainnet \
//                  --provider.wallet ~/.config/solana/id.json

import * as anchor from "@coral-xyz/anchor";
import { PublicKey, SystemProgram } from "@solana/web3.js";
import * as crypto from "crypto";

// The IDL JSON is emitted by `anchor build` at
// `target/idl/veria_verifier.json`.  We pull the program type lazily inside
// the migration body so this file still type-checks on a fresh clone where
// the IDL has not been generated yet.
// eslint-disable-next-line @typescript-eslint/no-var-requires
const idl = require("../target/idl/veria_verifier.json");

const CONFIG_SEED = Buffer.from("config");

function sha256(...parts: Buffer[]): Buffer {
  const h = crypto.createHash("sha256");
  for (const p of parts) h.update(p);
  return h.digest();
}

function clusterLabelFromAnchor(): { label: Buffer; pretty: string } {
  const cluster = (anchor.AnchorProvider.env() as any).connection.rpcEndpoint as string;
  if (cluster.includes("mainnet")) {
    return { label: Buffer.from("solana-mainnet-beta"), pretty: "mainnet" };
  }
  if (cluster.includes("devnet")) {
    return { label: Buffer.from("solana-devnet"), pretty: "devnet" };
  }
  return { label: Buffer.from("solana-test-validator"), pretty: "test" };
}

module.exports = async function (provider: anchor.AnchorProvider) {
  anchor.setProvider(provider);

  const programId = new PublicKey(idl.address ?? idl.metadata?.address);
  const program = new anchor.Program(idl as anchor.Idl, provider);
  const admin = provider.wallet.publicKey;

  const [configPda, configBump] = PublicKey.findProgramAddressSync(
    [CONFIG_SEED],
    programId,
  );

  console.log("=== VERIA verifier migration ===");
  console.log(`program id   : ${programId.toBase58()}`);
  console.log(`admin        : ${admin.toBase58()}`);
  console.log(`config PDA   : ${configPda.toBase58()} (bump=${configBump})`);

  // If the PDA already exists, log a summary and exit.
  const existing = await provider.connection.getAccountInfo(configPda);
  if (existing) {
    console.log("config PDA already exists — migration is a no-op.");
    const cfg = await (program.account as any).verifierConfig.fetch(configPda);
    console.log(`current vk_epoch     : ${cfg.vkEpoch}`);
    console.log(`current total_verified: ${cfg.totalVerified.toString()}`);
    return;
  }

  // Bundled SP1 Groth16 vk hash.  In v0.1 we ship the deterministic
  // placeholder `sha256("veria-sp1-groth16-vk-v1")` until the production
  // sp1-solana vk is wired in.  Production deploys replace this with the
  // real `sha256(sp1_solana::GROTH16_VK_BYTES)`.
  const vkHash = sha256(Buffer.from("veria-sp1-groth16-vk-v1"));
  const { label, pretty } = clusterLabelFromAnchor();
  console.log(`cluster label: ${pretty} (${label.toString()})`);
  console.log(`vk_hash      : ${vkHash.toString("hex")}`);

  const sig = await program.methods
    .initialize([...vkHash], Buffer.from(label))
    .accounts({
      admin,
      config: configPda,
      systemProgram: SystemProgram.programId,
    })
    .rpc({ commitment: "confirmed" });

  console.log(`initialise tx: ${sig}`);
  console.log("=== migration complete ===");
};
