/**
 * Waveform processing and generation helpers for audio and voice messages.
 * Compliant with Matrix MSC1767 and MSC3245/MSC3246 audio waveform conventions (amplitudes 0..1024).
 */

const MAX_AMPLITUDE = 1024;

/**
 * Downsamples an array of amplitude levels (0..1024) into exactly `bars` buckets.
 * Uses peak-of-bucket downsampling, clamping input levels and normalizing output to 0..1.
 */
export function downsample(levels: readonly number[], bars: number): number[] {
  if (bars <= 0) return [];
  if (levels.length === 0) return new Array(bars).fill(0);

  const result: number[] = new Array(bars);
  const n = levels.length;

  for (let i = 0; i < bars; i++) {
    const start = Math.floor((i * n) / bars);
    const end = Math.max(start + 1, Math.floor(((i + 1) * n) / bars));
    const limit = Math.min(end, n);

    let peak = 0;
    for (let j = start; j < limit; j++) {
      const val = levels[j];
      if (val > peak) peak = val;
    }

    // Clamp input amplitude to 0..MAX_AMPLITUDE and normalize to 0..1
    const clamped = Math.max(0, Math.min(MAX_AMPLITUDE, peak));
    result[i] = clamped / MAX_AMPLITUDE;
  }

  return result;
}

/**
 * Generates deterministic pseudo-random plausible waveform bars from a string seed (e.g. URI).
 * Used as fallback for audio files lacking waveform metadata.
 * Values are smooth, natural-looking amplitudes normalized between 0.12 and 0.95.
 */
export function fallbackBars(seed: string, bars: number): number[] {
  if (bars <= 0) return [];

  // FNV-1a 32-bit hash of seed
  let h = 0x811c9dc5;
  for (let i = 0; i < seed.length; i++) {
    h ^= seed.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }

  // Mulberry32 PRNG
  function prng(): number {
    h = Math.imul(h ^ (h >>> 15), h | 1);
    h ^= h + Math.imul(h ^ (h >>> 7), h | 61);
    return ((h ^ (h >>> 14)) >>> 0) / 4294967296;
  }

  // Generate plausible envelope with smooth variations
  const raw: number[] = new Array(bars);
  for (let i = 0; i < bars; i++) {
    raw[i] = prng();
  }

  // Smooth adjacent values to look like real audio speech/music contours
  const result: number[] = new Array(bars);
  for (let i = 0; i < bars; i++) {
    const prev = raw[Math.max(0, i - 1)];
    const curr = raw[i];
    const next = raw[Math.min(bars - 1, i + 1)];
    const smoothed = 0.25 * prev + 0.5 * curr + 0.25 * next;

    // Apply a soft envelope tapering at edges
    const pos = i / (bars - 1 || 1);
    const envelope = Math.sin(pos * Math.PI) * 0.4 + 0.6; // 0.6 at ends, 1.0 in middle

    // Scale to range [0.12, 0.95]
    const val = 0.12 + smoothed * 0.83 * envelope;
    result[i] = Math.round(Math.max(0.12, Math.min(0.95, val)) * 1000) / 1000;
  }

  return result;
}

/**
 * Returns the bar index (0 .. bars - 1) corresponding to a playback fraction (0..1).
 */
export function barIndexAt(fraction: number, bars: number): number {
  if (bars <= 0) return 0;
  const clamped = Math.max(0, Math.min(1, fraction));
  return Math.min(bars - 1, Math.floor(clamped * bars));
}
