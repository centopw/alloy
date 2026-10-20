import { describe, expect, it } from 'vitest';
import { looksLikeFilePath, pastedImageFiles, pastedImageName, voiceExtension } from './files';

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

describe('pasted pictures', () => {
  it('names a pasted picture so its type can be derived', () => {
    expect(pastedImageName({ name: 'Screenshot 1.png', type: 'image/png' })).toBe('Screenshot 1.png');
    expect(pastedImageName({ name: 'image', type: 'image/jpeg' })).toBe('image.jpg');
    expect(pastedImageName({ name: '', type: 'image/webp' })).toBe('pasted-image.webp');
    expect(pastedImageName({ name: '', type: '' })).toBe('pasted-image.png');
  });

  it('takes only the pictures from the clipboard files', () => {
    const picture = new File(['x'], 'a.png', { type: 'image/png' });
    const document = new File(['x'], 'a.pdf', { type: 'application/pdf' });
    expect(pastedImageFiles({ files: [picture, document] } as unknown as DataTransfer)).toEqual([picture]);
    expect(pastedImageFiles(null)).toEqual([]);
  });

  it('treats one line that starts like a file path as a candidate, and ordinary text as text', () => {
    expect(looksLikeFilePath('/Users/a/Captures/shot.png')).toBe(true);
    expect(looksLikeFilePath('"/Users/a/shot 1.png"\n')).toBe(true);
    expect(looksLikeFilePath('file:///Users/a/shot.png')).toBe(true);
    expect(looksLikeFilePath('C:\\Users\\a\\shot.png')).toBe(true);
    expect(looksLikeFilePath('see /Users/a/shot.png')).toBe(false);
    expect(looksLikeFilePath('/one.png\n/two.png')).toBe(false);
    expect(looksLikeFilePath('hello')).toBe(false);
    expect(looksLikeFilePath('')).toBe(false);
  });
});
