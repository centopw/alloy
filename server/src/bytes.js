/** @module bytes - platform-neutral helpers (Node and Workers both expose WebCrypto). */

const encoder = new TextEncoder();
const decoder = new TextDecoder();

/** @param {string} text @returns {Uint8Array} */
export function utf8(text) {
  return encoder.encode(text);
}

/** @param {Uint8Array} bytes @returns {string} */
export function fromUtf8(bytes) {
  return decoder.decode(bytes);
}

/** @param {Uint8Array} bytes @returns {string} */
export function hexOf(bytes) {
  let out = '';
  for (const byte of bytes) out += byte.toString(16).padStart(2, '0');
  return out;
}

/** @param {Uint8Array} bytes @returns {Promise<Uint8Array>} */
export async function sha256(bytes) {
  return new Uint8Array(await crypto.subtle.digest('SHA-256', bytes));
}

/**
 * Compare two byte arrays without an early exit. Lengths are public (digests are fixed size).
 * @param {Uint8Array} a @param {Uint8Array} b @returns {boolean}
 */
export function constantTimeEqual(a, b) {
  if (a.byteLength !== b.byteLength) return false;
  let diff = 0;
  for (let i = 0; i < a.byteLength; i += 1) diff |= a[i] ^ b[i];
  return diff === 0;
}

/**
 * @param {Uint8Array[]} chunks @param {number} total @returns {Uint8Array}
 */
export function concat(chunks, total) {
  const out = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    out.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return out;
}
