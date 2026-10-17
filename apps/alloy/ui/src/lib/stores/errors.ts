import type { EngineResponse } from '../generated/index.js';

/** Extract a readable message from a rejected engine call (EngineError {code,message} or Error). */
export function errorMessage(e: unknown): string {
  if (e && typeof e === 'object') {
    const o = e as { message?: unknown; code?: unknown };
    if (typeof o.message === 'string') return o.message;
  }
  return typeof e === 'string' ? e : 'Something went wrong';
}

export function errorCode(e: unknown): string | null {
  if (e && typeof e === 'object' && typeof (e as { code?: unknown }).code === 'string') {
    return (e as { code: string }).code;
  }
  return null;
}

/** Narrow a response to one variant or throw (a protocol bug, not a user error). */
export function expect<T extends EngineResponse['type']>(
  res: EngineResponse,
  type: T
): Extract<EngineResponse, { type: T }> {
  if (res.type !== type) throw new Error(`Expected ${type}, got ${res.type}`);
  return res as Extract<EngineResponse, { type: T }>;
}
