export enum VeriaErrorCode {
  NetworkError = 'NETWORK_ERROR',
  InvalidInput = 'INVALID_INPUT',
  CircuitNotFound = 'CIRCUIT_NOT_FOUND',
  ProofGenerationFailed = 'PROOF_GENERATION_FAILED',
  VerifierRejected = 'VERIFIER_REJECTED',
  ProgramIdMissing = 'PROGRAM_ID_MISSING',
  ProofAlreadyVerified = 'PROOF_ALREADY_VERIFIED',
  RpcError = 'RPC_ERROR',
}

export class VeriaError extends Error {
  readonly code: VeriaErrorCode;
  readonly cause?: unknown;

  constructor(code: VeriaErrorCode, message: string, cause?: unknown) {
    super(message);
    this.name = 'VeriaError';
    this.code = code;
    this.cause = cause;
  }
}
