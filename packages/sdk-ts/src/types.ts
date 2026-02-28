import type { CircuitId } from './circuits.js';

export interface FoldRequest {
  circuit: CircuitId;
  input: unknown;
  subProofCount?: number;
  metadata?: Record<string, string>;
}

export interface FoldResponse {
  jobId: string;
  circuit: CircuitId;
  subProofCount: number;
  proofBytes: Uint8Array;
  publicInputs: Uint8Array;
  costSol: number;
  directCostSol: number;
  savingsPct: number;
  createdAt: number;
}

export interface VerifyRequest {
  proofBytes: Uint8Array;
  publicInputs: Uint8Array;
  circuit: CircuitId;
}

export interface VerifyResponse {
  signature: string;
  proofHash: string;
  explorerUrl: string;
  programId: string;
  recordPda: string;
  verifiedAt: number;
}

export interface ProofRecord {
  circuitId: number;
  publicInputsHash: Uint8Array;
  verifiedAt: number;
  submitter: string;
  bump: number;
}

export interface CostBreakdown {
  subProofs: number;
  directSolPerSubProof: number;
  directTotalSol: number;
  foldedSol: number;
  savingsSol: number;
  savingsPct: number;
}
