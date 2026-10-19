import { describe, expect, it } from 'vitest';
import { voiceExtension } from './files';

describe('voiceExtension', () => {
  it('names the container of a recorded clip', () => {
    expect(voiceExtension('audio/webm;codecs=opus')).toBe('webm');
    expect(voiceExtension('audio/ogg;codecs=opus')).toBe('ogg');
    expect(voiceExtension('audio/mp4')).toBe('m4a');
  });

  it('falls back for an unknown type', () => {
    expect(voiceExtension('audio/wav')).toBe('bin');
  });
});
