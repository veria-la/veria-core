export type CircuitId =
  | 'scoring'
  | 'aggregation'
  | 'median'
  | 'sort'
  | 'ml-inference';

export interface CircuitMeta {
  id: CircuitId;
  numericId: number;
  name: string;
  description: string;
  maxInputSize: number;
  testCount: number;
  inputSchema: string;
  outputSchema: string;
}

export const CIRCUITS: Record<CircuitId, CircuitMeta> = {
  scoring: {
    id: 'scoring',
    numericId: 1,
    name: 'Scoring',
    description:
      'Weighted average over a fixed-length score vector. Weights are fixed-point scale 2^32.',
    maxInputSize: 64,
    testCount: 5,
    inputSchema:
      '{ scores: [u64; 64], weights: [u64; 64], count: u32 (<= 64) }',
    outputSchema: '{ weighted_avg_fp: u64, total_weight: u64 }',
  },
  aggregation: {
    id: 'aggregation',
    numericId: 2,
    name: 'Aggregation',
    description:
      'SUM, AVG, MIN, MAX in a single pass over up to 4096 u64 inputs.',
    maxInputSize: 4096,
    testCount: 4,
    inputSchema: '{ values: Vec<u64> (len <= 4096) }',
    outputSchema: '{ sum_u128, avg_u64 (floor), min_u64, max_u64 }',
  },
  median: {
    id: 'median',
    numericId: 3,
    name: 'Median',
    description:
      'Median with sortedness witness and permutation proof. Honest oracle aggregation.',
    maxInputSize: 256,
    testCount: 5,
    inputSchema:
      '{ raw: [u64; 256], sorted: [u64; 256], perm: [u16; 256], count: u32 }',
    outputSchema: '{ median: u64 }',
  },
  sort: {
    id: 'sort',
    numericId: 4,
    name: 'Sort',
    description:
      'Permutation proof: output is monotonic, output multiset equals input multiset.',
    maxInputSize: 256,
    testCount: 4,
    inputSchema:
      '{ input: [u64; 256], sorted: [u64; 256], perm: [u16; 256], count: u32 }',
    outputSchema: '{ sorted: [u64; 256] }',
  },
  'ml-inference': {
    id: 'ml-inference',
    numericId: 5,
    name: 'ML Inference',
    description:
      'Fixed-point MLP forward pass. Two hidden layers (32-16-8-4), ReLU activation. Weights public.',
    maxInputSize: 32,
    testCount: 4,
    inputSchema:
      '{ features: [i32; 32], w1: [[i32; 32]; 16], b1: [i32; 16], w2: [[i32; 16]; 8], b2: [i32; 8], w3: [[i32; 8]; 4], b3: [i32; 4] }',
    outputSchema: '{ logits: [i32; 4] (fixed-point scale 2^16) }',
  },
};

export function circuitByNumericId(numeric: number): CircuitMeta | undefined {
  return Object.values(CIRCUITS).find((c) => c.numericId === numeric);
}

export function listCircuits(): CircuitMeta[] {
  return Object.values(CIRCUITS);
}
