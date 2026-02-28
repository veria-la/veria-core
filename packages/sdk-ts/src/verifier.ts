import {
  Connection,
  PublicKey,
  SystemProgram,
  Transaction,
  TransactionInstruction,
} from '@solana/web3.js';
import { CIRCUITS, type CircuitId } from './circuits.js';
import { VeriaError, VeriaErrorCode } from './errors.js';
import type { ProofRecord } from './types.js';
import { proofHash } from './utils/hash.js';

export interface VerifierOptions {
  connection: Connection;
  programId: PublicKey | string;
}

const PROOF_SEED = new TextEncoder().encode('proof');
const CONFIG_SEED = new TextEncoder().encode('config');
const INSTRUCTION_VERIFY_DISC = new Uint8Array([
  0x83, 0xc1, 0xb1, 0xea, 0x6f, 0xc9, 0x9d, 0xa3,
]);

export class VeriaVerifier {
  private readonly connection: Connection;
  readonly programId: PublicKey;

  constructor(opts: VerifierOptions) {
    this.connection = opts.connection;
    this.programId =
      typeof opts.programId === 'string'
        ? new PublicKey(opts.programId)
        : opts.programId;
  }

  recordPda(proofBytes: Uint8Array, publicInputs: Uint8Array): PublicKey {
    const hash = proofHash(proofBytes, publicInputs);
    const [pda] = PublicKey.findProgramAddressSync(
      [PROOF_SEED, hash],
      this.programId,
    );
    return pda;
  }

  configPda(): PublicKey {
    const [pda] = PublicKey.findProgramAddressSync(
      [CONFIG_SEED],
      this.programId,
    );
    return pda;
  }

  buildVerifyIx(args: {
    submitter: PublicKey;
    circuit: CircuitId;
    proofBytes: Uint8Array;
    publicInputs: Uint8Array;
  }): TransactionInstruction {
    const meta = CIRCUITS[args.circuit];
    if (!meta) {
      throw new VeriaError(
        VeriaErrorCode.CircuitNotFound,
        `Unknown circuit: ${args.circuit}`,
      );
    }

    const recordPda = this.recordPda(args.proofBytes, args.publicInputs);
    const configPda = this.configPda();

    const dataLen =
      8 + 1 + 4 + args.proofBytes.length + 4 + args.publicInputs.length;
    const data = new Uint8Array(dataLen);
    let offset = 0;
    data.set(INSTRUCTION_VERIFY_DISC, offset);
    offset += 8;
    data[offset] = meta.numericId;
    offset += 1;
    writeU32LE(data, offset, args.proofBytes.length);
    offset += 4;
    data.set(args.proofBytes, offset);
    offset += args.proofBytes.length;
    writeU32LE(data, offset, args.publicInputs.length);
    offset += 4;
    data.set(args.publicInputs, offset);

    return new TransactionInstruction({
      programId: this.programId,
      keys: [
        { pubkey: args.submitter, isSigner: true, isWritable: true },
        { pubkey: recordPda, isSigner: false, isWritable: true },
        { pubkey: configPda, isSigner: false, isWritable: true },
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      ],
      data: Buffer.from(data),
    });
  }

  buildVerifyTx(args: {
    submitter: PublicKey;
    circuit: CircuitId;
    proofBytes: Uint8Array;
    publicInputs: Uint8Array;
  }): Transaction {
    const tx = new Transaction().add(this.buildVerifyIx(args));
    tx.feePayer = args.submitter;
    return tx;
  }

  async fetchRecord(pda: PublicKey): Promise<ProofRecord | null> {
    const info = await this.connection.getAccountInfo(pda);
    if (!info) return null;
    return parseProofRecord(info.data);
  }
}

function writeU32LE(buf: Uint8Array, offset: number, value: number): void {
  buf[offset] = value & 0xff;
  buf[offset + 1] = (value >>> 8) & 0xff;
  buf[offset + 2] = (value >>> 16) & 0xff;
  buf[offset + 3] = (value >>> 24) & 0xff;
}

function parseProofRecord(data: Uint8Array): ProofRecord {
  if (data.length < 8 + 1 + 32 + 8 + 32 + 1) {
    throw new VeriaError(
      VeriaErrorCode.RpcError,
      `ProofRecord account too small: ${data.length}`,
    );
  }
  let offset = 8;
  const circuitId = data[offset];
  offset += 1;
  const publicInputsHash = data.slice(offset, offset + 32);
  offset += 32;
  const verifiedAt = Number(readI64LE(data, offset));
  offset += 8;
  const submitter = Array.from(data.slice(offset, offset + 32))
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');
  offset += 32;
  const bump = data[offset];
  return {
    circuitId: circuitId ?? 0,
    publicInputsHash,
    verifiedAt,
    submitter,
    bump: bump ?? 0,
  };
}

function readI64LE(buf: Uint8Array, offset: number): bigint {
  let result = 0n;
  for (let i = 0; i < 8; i++) {
    result |= BigInt(buf[offset + i] ?? 0) << BigInt(i * 8);
  }
  if (result >= 1n << 63n) {
    result -= 1n << 64n;
  }
  return result;
}
