import type { Transport } from './transport.js';
import { MockTransport } from './mock.js';

/**
 * Development-only entry point for the in-memory demo engine.
 *
 * Kept in its own module so the production bundle can drop it: `index.ts` reaches it through a
 * dynamic import inside a statically-false `import.meta.env.DEV` branch. Importing the mock
 * anywhere else would put demo data back into a shipped build.
 */
export function createMockTransport(): Transport {
  return new MockTransport();
}
