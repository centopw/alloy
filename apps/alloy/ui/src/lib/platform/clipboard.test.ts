import { describe, expect, it, vi } from 'vitest';
import { canCopyImage, convertBlobToPng, copyImageBlob, copyText } from './clipboard';

describe('canCopyImage', () => {
  it('returns false when ClipboardItem is undefined', () => {
    const original = globalThis.ClipboardItem;
    // @ts-expect-error test override
    delete globalThis.ClipboardItem;
    expect(canCopyImage()).toBe(false);
    globalThis.ClipboardItem = original;
  });

  it('returns true when ClipboardItem and navigator.clipboard.write exist', () => {
    class MockClipboardItem {}
    // @ts-expect-error test mock
    globalThis.ClipboardItem = MockClipboardItem;
    const originalWrite = navigator.clipboard?.write;
    Object.defineProperty(navigator, 'clipboard', {
      value: { write: vi.fn(), writeText: vi.fn() },
      configurable: true,
    });
    expect(canCopyImage()).toBe(true);
    if (originalWrite) {
      navigator.clipboard.write = originalWrite;
    }
  });
});

describe('convertBlobToPng', () => {
  it('returns the same blob directly when already image/png without canvas conversion', async () => {
    const pngBlob = new Blob(['fake png data'], { type: 'image/png' });
    const result = await convertBlobToPng(pngBlob);
    expect(result).toBe(pngBlob);
    expect(result.type).toBe('image/png');
  });
});

describe('copyImageBlob', () => {
  it('throws an error if ClipboardItem is not supported', async () => {
    // @ts-expect-error test override
    delete globalThis.ClipboardItem;
    const blob = new Blob(['test'], { type: 'image/png' });
    await expect(copyImageBlob(blob)).rejects.toThrow('Clipboard image copy is not supported');
  });

  it('writes PNG blob to navigator.clipboard.write using ClipboardItem', async () => {
    let writtenItem: unknown = null;
    class MockClipboardItem {
      data: Record<string, Blob>;
      constructor(data: Record<string, Blob>) {
        this.data = data;
      }
    }
    // @ts-expect-error test mock
    globalThis.ClipboardItem = MockClipboardItem;
    const writeMock = vi.fn().mockImplementation(async (items: unknown[]) => {
      writtenItem = items[0];
    });
    Object.defineProperty(navigator, 'clipboard', {
      value: { write: writeMock },
      configurable: true,
    });

    const pngBlob = new Blob(['png data'], { type: 'image/png' });
    await copyImageBlob(pngBlob);
    expect(writeMock).toHaveBeenCalledTimes(1);
    expect(writtenItem).toBeInstanceOf(MockClipboardItem);
    expect((writtenItem as MockClipboardItem).data['image/png']).toBe(pngBlob);
  });
});

describe('copyText', () => {
  it('calls navigator.clipboard.writeText when available', async () => {
    const writeTextMock = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText: writeTextMock },
      configurable: true,
    });
    await copyText('hello world');
    expect(writeTextMock).toHaveBeenCalledWith('hello world');
  });
});
