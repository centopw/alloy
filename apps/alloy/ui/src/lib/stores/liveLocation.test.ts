import { describe, expect, it } from 'vitest';
import { BEACON_INTERVAL_MS, beaconDue } from './liveLocation.svelte';

describe('beaconDue', () => {
  it('sends the first position at once', () => {
    expect(beaconDue(null, 1_000)).toBe(true);
  });

  it('then waits out the interval', () => {
    expect(beaconDue(1_000, 1_000 + BEACON_INTERVAL_MS - 1)).toBe(false);
    expect(beaconDue(1_000, 1_000 + BEACON_INTERVAL_MS)).toBe(true);
  });
});
