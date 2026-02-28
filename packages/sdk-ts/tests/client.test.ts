import { describe, expect, it } from 'vitest';
import { CIRCUITS, circuitByNumericId, listCircuits } from '../src/circuits';
import { VeriaClient } from '../src/client';
import { VeriaError, VeriaErrorCode } from '../src/errors';
import { fromHex, proofHash, toHex } from '../src/utils/hash';

describe('CIRCUITS', () => {
  it('lists five circuits', () => {
    expect(listCircuits()).toHaveLength(5);
  });

  it('exposes deterministic numeric ids', () => {
    expect(CIRCUITS.scoring.numericId).toBe(1);
    expect(CIRCUITS.aggregation.numericId).toBe(2);
    expect(CIRCUITS.median.numericId).toBe(3);
    expect(CIRCUITS.sort.numericId).toBe(4);
    expect(CIRCUITS['ml-inference'].numericId).toBe(5);
  });

  it('reverse-lookup by numeric id', () => {
    expect(circuitByNumericId(3)?.id).toBe('median');
    expect(circuitByNumericId(99)).toBeUndefined();
  });
});

describe('hash utils', () => {
  it('proofHash is deterministic over (proof || pub)', () => {
    const a = proofHash(new Uint8Array([1, 2, 3]), new Uint8Array([4, 5]));
    const b = proofHash(new Uint8Array([1, 2, 3]), new Uint8Array([4, 5]));
    expect(toHex(a)).toBe(toHex(b));
  });

  it('proofHash distinguishes concatenation boundary', () => {
    const a = proofHash(new Uint8Array([1, 2]), new Uint8Array([3, 4]));
    const b = proofHash(new Uint8Array([1]), new Uint8Array([2, 3, 4]));
    expect(toHex(a)).not.toBe(toHex(b));
  });

  it('hex round-trips', () => {
    const bytes = new Uint8Array([0, 1, 0x0a, 0xff]);
    expect(fromHex(toHex(bytes))).toEqual(bytes);
    expect(fromHex('0xff00')).toEqual(new Uint8Array([0xff, 0x00]));
  });
});

describe('VeriaClient.computeCost', () => {
  it('100 sub-proofs save >99.8%', () => {
    const c = new VeriaClient();
    const bd = c.computeCost(100);
    expect(bd.subProofs).toBe(100);
    expect(bd.directTotalSol).toBeCloseTo(0.5, 6);
    expect(bd.foldedSol).toBeCloseTo(0.0001, 6);
    expect(bd.savingsPct).toBeGreaterThan(99.8);
  });

  it('1000 sub-proofs save >99.99%', () => {
    const c = new VeriaClient();
    const bd = c.computeCost(1000);
    expect(bd.savingsPct).toBeGreaterThan(99.99);
  });
});

describe('VeriaClient.fold', () => {
  it('rejects unknown circuit', async () => {
    const c = new VeriaClient();
    await expect(
      c.fold({ circuit: 'doesnotexist' as any, input: {} }),
    ).rejects.toBeInstanceOf(VeriaError);
  });

  it('uses provided fetcher and parses hex fields', async () => {
    let captured: { url: string; body: unknown } | undefined;
    const fakeFetch = async (input: RequestInfo | URL, init?: RequestInit) => {
      captured = { url: String(input), body: JSON.parse(String(init?.body)) };
      return new Response(
        JSON.stringify({
          job_id: 'job-1',
          circuit: 'scoring',
          sub_proof_count: 10,
          proof_bytes_hex: 'deadbeef',
          public_inputs_hex: '01',
          cost_sol: 0.0001,
          direct_cost_sol: 0.05,
          savings_pct: 99.8,
          created_at: 1700000000,
        }),
        { status: 200 },
      );
    };
    const c = new VeriaClient({
      apiUrl: 'http://localhost:8000',
      fetcher: fakeFetch as typeof fetch,
    });
    const res = await c.fold({
      circuit: 'scoring',
      input: { scores: [10, 20, 30] },
    });
    expect(res.jobId).toBe('job-1');
    expect(res.proofBytes).toEqual(new Uint8Array([0xde, 0xad, 0xbe, 0xef]));
    expect(captured?.url).toBe('http://localhost:8000/fold');
  });

  it('verify requires programId', async () => {
    const c = new VeriaClient();
    await expect(
      c.verify({
        proofBytes: new Uint8Array(),
        publicInputs: new Uint8Array(),
        circuit: 'scoring',
      }),
    ).rejects.toMatchObject({ code: VeriaErrorCode.ProgramIdMissing });
  });
});
