import { sha256 } from '@noble/hashes/sha256';

export function proofHash(
  proofBytes: Uint8Array,
  publicInputs: Uint8Array,
): Uint8Array {
  const concat = new Uint8Array(proofBytes.length + publicInputs.length);
  concat.set(proofBytes, 0);
  concat.set(publicInputs, proofBytes.length);
  return sha256(concat);
}

export function publicInputsHash(publicInputs: Uint8Array): Uint8Array {
  return sha256(publicInputs);
}

export function toHex(bytes: Uint8Array): string {
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');
}

export function fromHex(hex: string): Uint8Array {
  const clean = hex.startsWith('0x') ? hex.slice(2) : hex;
  if (clean.length % 2 !== 0) {
    throw new Error('Invalid hex string length');
  }
  const out = new Uint8Array(clean.length / 2);
  for (let i = 0; i < out.length; i++) {
    const hi = clean.charCodeAt(i * 2);
    const lo = clean.charCodeAt(i * 2 + 1);
    out[i] = (hexNibble(hi) << 4) | hexNibble(lo);
  }
  return out;
}

function hexNibble(code: number): number {
  if (code >= 48 && code <= 57) return code - 48;
  if (code >= 97 && code <= 102) return code - 87;
  if (code >= 65 && code <= 70) return code - 55;
  throw new Error('Invalid hex character');
}
