import { describe, expect, it, beforeEach, afterEach } from 'vitest';
import { nativeClipboardHasImage, nativeClipboardImage } from './clipboardImage';

describe('clipboardImage', () => {
  beforeEach(() => {
    delete window.AlloyClipboard;
  });

  afterEach(() => {
    delete window.AlloyClipboard;
  });

  describe('nativeClipboardHasImage', () => {
    it('returns false when AlloyClipboard is not available', () => {
      expect(nativeClipboardHasImage()).toBe(false);
    });

    it('returns false when hasImage throws an exception', () => {
      window.AlloyClipboard = {
        hasImage: () => {
          throw new Error('Bridge error');
        },
        saveImage: () => '',
      };
      expect(nativeClipboardHasImage()).toBe(false);
    });

    it('returns true when bridge reports an image is present', () => {
      window.AlloyClipboard = {
        hasImage: () => true,
        saveImage: () => '',
      };
      expect(nativeClipboardHasImage()).toBe(true);
    });

    it('returns false when bridge reports no image', () => {
      window.AlloyClipboard = {
        hasImage: () => false,
        saveImage: () => '',
      };
      expect(nativeClipboardHasImage()).toBe(false);
    });
  });

  describe('nativeClipboardImage', () => {
    it('returns null when AlloyClipboard is not available', () => {
      expect(nativeClipboardImage()).toBeNull();
    });

    it('returns null when saveImage throws an exception', () => {
      window.AlloyClipboard = {
        hasImage: () => true,
        saveImage: () => {
          throw new Error('Bridge error');
        },
      };
      expect(nativeClipboardImage()).toBeNull();
    });

    it('returns null when saveImage returns an empty string', () => {
      window.AlloyClipboard = {
        hasImage: () => true,
        saveImage: () => '',
      };
      expect(nativeClipboardImage()).toBeNull();
    });

    it('returns the path when saveImage returns a non-empty string', () => {
      const testPath = '/data/user/0/chat.alloy/cache/clipboard/uuid/image.png';
      window.AlloyClipboard = {
        hasImage: () => true,
        saveImage: () => testPath,
      };
      expect(nativeClipboardImage()).toBe(testPath);
    });
  });
});
