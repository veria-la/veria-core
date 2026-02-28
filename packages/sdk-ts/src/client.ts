import { CIRCUITS, type CircuitId } from './circuits.js';
import { VeriaError, VeriaErrorCode } from './errors.js';
import type {
  CostBreakdown,
  FoldRequest,
  FoldResponse,
  VerifyRequest,
  VerifyResponse,
} from './types.js';
import { fromHex, proofHash, toHex } from './utils/hash.js';

export interface VeriaClientOptions {
  apiUrl?: string;
  programId?: string;
  fetcher?: typeof fetch;
  timeoutMs?: number;
  apiKey?: string;
}

const DEFAULT_API = 'https://api.veria.fun';
const DEFAULT_DIRECT_SOL_PER_SUB = 0.005;
const DEFAULT_FOLDED_SOL = 0.0001;

export class VeriaClient {
  private readonly apiUrl: string;
  private readonly fetcher: typeof fetch;
  private readonly timeoutMs: number;
  private readonly apiKey?: string;
  readonly programId?: string;

  constructor(opts: VeriaClientOptions = {}) {
    this.apiUrl = (opts.apiUrl ?? DEFAULT_API).replace(/\/$/, '');
    this.fetcher = opts.fetcher ?? fetch;
    this.timeoutMs = opts.timeoutMs ?? 60_000;
    this.apiKey = opts.apiKey;
    this.programId = opts.programId;
  }

  async fold(req: FoldRequest): Promise<FoldResponse> {
    if (!(req.circuit in CIRCUITS)) {
      throw new VeriaError(
        VeriaErrorCode.CircuitNotFound,
        `Unknown circuit: ${req.circuit}`,
      );
    }

    const body = {
      circuit: req.circuit,
      input: req.input,
      sub_proof_count: req.subProofCount ?? 100,
      metadata: req.metadata ?? {},
    };

    const res = await this.post<RawFoldResponse>('/fold', body);
    return {
      jobId: res.job_id,
      circuit: res.circuit as CircuitId,
      subProofCount: res.sub_proof_count,
      proofBytes: fromHex(res.proof_bytes_hex),
      publicInputs: fromHex(res.public_inputs_hex),
      costSol: res.cost_sol,
      directCostSol: res.direct_cost_sol,
      savingsPct: res.savings_pct,
      createdAt: res.created_at,
    };
  }

  async verify(req: VerifyRequest): Promise<VerifyResponse> {
    if (!this.programId) {
      throw new VeriaError(
        VeriaErrorCode.ProgramIdMissing,
        'programId is required for on-chain verify(). Set VeriaClient({ programId }).',
      );
    }

    const body = {
      circuit: req.circuit,
      proof_bytes_hex: toHex(req.proofBytes),
      public_inputs_hex: toHex(req.publicInputs),
      program_id: this.programId,
    };
    const res = await this.post<RawVerifyResponse>('/verify', body);
    return {
      signature: res.signature,
      proofHash: res.proof_hash_hex,
      explorerUrl: res.explorer_url,
      programId: res.program_id,
      recordPda: res.record_pda,
      verifiedAt: res.verified_at,
    };
  }

  computeCost(subProofCount: number): CostBreakdown {
    const directTotal = subProofCount * DEFAULT_DIRECT_SOL_PER_SUB;
    const folded = DEFAULT_FOLDED_SOL;
    const savings = directTotal - folded;
    return {
      subProofs: subProofCount,
      directSolPerSubProof: DEFAULT_DIRECT_SOL_PER_SUB,
      directTotalSol: directTotal,
      foldedSol: folded,
      savingsSol: savings,
      savingsPct: (savings / directTotal) * 100,
    };
  }

  localProofHash(proofBytes: Uint8Array, publicInputs: Uint8Array): string {
    return toHex(proofHash(proofBytes, publicInputs));
  }

  private async post<T>(path: string, body: unknown): Promise<T> {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.timeoutMs);

    try {
      const res = await this.fetcher(`${this.apiUrl}${path}`, {
        method: 'POST',
        headers: {
          'content-type': 'application/json',
          ...(this.apiKey ? { authorization: `Bearer ${this.apiKey}` } : {}),
        },
        body: JSON.stringify(body),
        signal: controller.signal,
      });

      if (!res.ok) {
        const detail = await res.text();
        throw new VeriaError(
          res.status === 404
            ? VeriaErrorCode.CircuitNotFound
            : VeriaErrorCode.NetworkError,
          `${path} -> ${res.status}: ${detail.slice(0, 256)}`,
        );
      }

      return (await res.json()) as T;
    } catch (exc) {
      if (exc instanceof VeriaError) {
        throw exc;
      }
      throw new VeriaError(
        VeriaErrorCode.NetworkError,
        `Network error calling ${path}: ${(exc as Error).message}`,
        exc,
      );
    } finally {
      clearTimeout(timer);
    }
  }
}

interface RawFoldResponse {
  job_id: string;
  circuit: string;
  sub_proof_count: number;
  proof_bytes_hex: string;
  public_inputs_hex: string;
  cost_sol: number;
  direct_cost_sol: number;
  savings_pct: number;
  created_at: number;
}

interface RawVerifyResponse {
  signature: string;
  proof_hash_hex: string;
  explorer_url: string;
  program_id: string;
  record_pda: string;
  verified_at: number;
}
