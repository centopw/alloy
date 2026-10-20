import { describe, expect, it, vi } from 'vitest';
import { stagePictureFromPath } from './stageHelper';

vi.mock('$lib/platform/files', () => ({
  importPastedImagePath: vi.fn(async (path: string) => {
    if (path.startsWith('/tmp/good-')) {
      return '/app/cache/picks/uuid/valid-image.png';
    }
    return null;
  }),
}));

describe('stagePictureFromPath', () => {
  it('returns a staged picture when the path is successfully imported', async () => {
    const result = await stagePictureFromPath('/tmp/good-image.png');
    expect(result).toEqual({
      path: '/app/cache/picks/uuid/valid-image.png',
      name: 'valid-image.png',
      preview: null,
    });
  });

  it('returns null when the path cannot be imported', async () => {
    const result = await stagePictureFromPath('/tmp/invalid-file.txt');
    expect(result).toBeNull();
  });
});
