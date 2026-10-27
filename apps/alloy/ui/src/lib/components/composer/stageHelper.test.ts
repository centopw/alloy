import { describe, expect, it, vi } from 'vitest';
import { stageFile, stagePictureFromPath, stagedKind } from './stageHelper';

vi.mock('$lib/platform/files', () => ({
  importPastedImagePath: vi.fn(async (path: string) => {
    if (path.startsWith('/tmp/good-')) {
      return '/app/cache/picks/uuid/valid-image.png';
    }
    return null;
  }),
  localFileUrl: vi.fn(async (path: string) => `asset://localhost${path}`),
}));

describe('stagePictureFromPath', () => {
  it('stages an imported picture with a renderable preview', async () => {
    const result = await stagePictureFromPath('/tmp/good-image.png');
    expect(result).toEqual({
      path: '/app/cache/picks/uuid/valid-image.png',
      name: 'valid-image.png',
      kind: 'image',
      preview: 'asset://localhost/app/cache/picks/uuid/valid-image.png',
    });
  });

  it('returns null when the path cannot be imported', async () => {
    const result = await stagePictureFromPath('/tmp/invalid-file.txt');
    expect(result).toBeNull();
  });
});

describe('stagedKind', () => {
  it('recognises pictures by extension, case-insensitively', () => {
    for (const path of ['/tmp/a.png', '/tmp/a.JPG', '/tmp/a.jpeg', '/tmp/a.gif', '/tmp/a.webp', '/tmp/a.avif', '/tmp/a.heic']) {
      expect(stagedKind(path)).toBe('image');
    }
  });

  it('recognises videos by extension', () => {
    for (const path of ['/tmp/a.mp4', '/tmp/a.m4v', '/tmp/a.MOV', '/tmp/a.webm', '/tmp/a.mkv', '/tmp/a.ogv']) {
      expect(stagedKind(path)).toBe('video');
    }
  });

  it('leaves anything else a plain file', () => {
    for (const path of ['/tmp/a.pdf', '/tmp/a.txt', '/tmp/a.zip']) {
      expect(stagedKind(path)).toBe('file');
    }
  });
});

describe('stageFile', () => {
  it('gives a video a preview URL the webview can draw', async () => {
    const result = await stageFile('/tmp/clip.mp4');
    expect(result).toEqual({
      path: '/tmp/clip.mp4',
      name: 'clip.mp4',
      kind: 'video',
      preview: 'asset://localhost/tmp/clip.mp4',
    });
  });

  it('leaves a plain file without a preview', async () => {
    const result = await stageFile('/tmp/report.pdf');
    expect(result.kind).toBe('file');
    expect(result.preview).toBeNull();
  });
});
