import { describe, expect, it } from 'vitest';
import {
  calculateVisibleRows,
  computeSheetHeight,
  estimateScrollerHeight
} from './emojiPickerSizing';

describe('emojiPickerSizing', () => {
  describe('computeSheetHeight', () => {
    it('uses 65% of viewport height when collapsed on a normal 780px screen', () => {
      const height = computeSheetHeight({
        viewportHeight: 780,
        isExpanded: false
      });
      // 780 * 0.65 = 507
      expect(height).toBe(507);
    });

    it('uses full visible height minus top margin when expanded on compressed 430px screen', () => {
      const height = computeSheetHeight({
        viewportHeight: 430,
        isExpanded: true,
        safeAreaTop: 0,
        topMargin: 8
      });
      // 430 - 0 - 8 = 422
      expect(height).toBe(422);
    });

    it('respects safeAreaTop when expanded on a notched screen', () => {
      const height = computeSheetHeight({
        viewportHeight: 430,
        isExpanded: true,
        safeAreaTop: 32,
        topMargin: 8
      });
      // 430 - 32 - 8 = 390
      expect(height).toBe(390);
    });

    it('caps collapsed height if 65% exceeds available screen height', () => {
      const height = computeSheetHeight({
        viewportHeight: 100,
        isExpanded: false,
        safeAreaTop: 40,
        topMargin: 10
      });
      // maxHeight = 100 - 40 - 10 = 50, whereas 65% of 100 is 65
      expect(height).toBe(50);
    });
  });

  describe('estimateScrollerHeight', () => {
    it('computes scroller space after deducting fixed overhead', () => {
      // 422 - (28 grip + 56 top + 44 footer + 16 padding) = 422 - 144 = 278
      const scrollerHeight = estimateScrollerHeight(422, 28, 56, 44, 16);
      expect(scrollerHeight).toBe(278);
    });

    it('returns 0 when sheet is shorter than overhead', () => {
      const scrollerHeight = estimateScrollerHeight(100, 28, 56, 44, 16);
      expect(scrollerHeight).toBe(0);
    });
  });

  describe('calculateVisibleRows', () => {
    it('guarantees at least 3 visible rows on a compressed 430px viewport', () => {
      const sheetHeight = computeSheetHeight({
        viewportHeight: 430,
        isExpanded: true,
        safeAreaTop: 0,
        topMargin: 8
      });
      const available = estimateScrollerHeight(sheetHeight);
      const rows = calculateVisibleRows({
        availableHeight: available,
        cellHeight: 44,
        rowGap: 2,
        headingHeight: 28
      });
      // available is 278, (278 - 28 + 2) / 46 = 5.47 -> 5 rows
      expect(rows).toBeGreaterThanOrEqual(3);
      expect(rows).toBe(5);
    });

    it('returns 0 when available height cannot accommodate heading and one row', () => {
      const rows = calculateVisibleRows({
        availableHeight: 50,
        cellHeight: 44,
        headingHeight: 28
      });
      // 50 - 28 = 22 < 44
      expect(rows).toBe(0);
    });

    it('computes exact row count based on cell height and gaps', () => {
      // 166 available - 28 heading = 138 content
      // 3 rows: 44 * 3 + 2 * 2 = 136 <= 138
      const rows = calculateVisibleRows({
        availableHeight: 166,
        cellHeight: 44,
        rowGap: 2,
        headingHeight: 28
      });
      expect(rows).toBe(3);
    });
  });
});
