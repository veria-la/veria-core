export { VeriaClient, type VeriaClientOptions } from './client.js';
export { CIRCUITS, type CircuitId, type CircuitMeta } from './circuits.js';
export { VeriaVerifier, type VerifierOptions } from './verifier.js';
export { proofHash, publicInputsHash } from './utils/hash.js';
export {
  type FoldRequest,
  type FoldResponse,
  type VerifyRequest,
  type VerifyResponse,
  type ProofRecord,
} from './types.js';
export { VeriaError, VeriaErrorCode } from './errors.js';
