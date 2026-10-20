/**
 * Pure sizing calculations for the mobile emoji picker bottom sheet.
 * Computes viewport bounds, available scrolling space, and guaranteed visible rows
 * when the Android software keyboard (IME) compresses the WebView.
 */

export interface SheetHeightOptions {
  viewportHeight: number;
  isExpanded: boolean;
  safeAreaTop?: number;
  topMargin?: number;
  defaultFraction?: number;
}

export interface VisibleRowsOptions {
  availableHeight: number;
  cellHeight?: number;
  rowGap?: number;
  headingHeight?: number;
}

/**
 * Computes the sheet height in pixels.
 * When expanded (search focused or query active on phone), the sheet uses the full
 * visible height minus top safe area and margin. Otherwise, it uses the default
 * fraction (65% of viewport), capped by the maximum allowed height.
 */
export function computeSheetHeight(options: SheetHeightOptions): number {
  const {
    viewportHeight,
    isExpanded,
    safeAreaTop = 0,
    topMargin = 8,
    defaultFraction = 0.65
  } = options;

  const maxHeight = Math.max(0, viewportHeight - safeAreaTop - topMargin);
  if (isExpanded) {
    return maxHeight;
  }
  return Math.min(Math.round(viewportHeight * defaultFraction), maxHeight);
}

/**
 * Calculates the number of completely visible emoji rows in the scroller.
 * Returns 0 if available height cannot accommodate heading plus at least one row.
 */
export function calculateVisibleRows(options: VisibleRowsOptions): number {
  const {
    availableHeight,
    cellHeight = 44,
    rowGap = 2,
    headingHeight = 28
  } = options;

  const contentHeight = availableHeight - headingHeight;
  if (contentHeight < cellHeight) {
    return 0;
  }
  return Math.floor((contentHeight + rowGap) / (cellHeight + rowGap));
}

/**
 * Estimates remaining vertical height allocated to the scrollable results region
 * inside the sheet, subtracting fixed headers (grip, search bar), footer and padding.
 */
export function estimateScrollerHeight(
  sheetHeight: number,
  gripHeight = 28,
  topHeight = 56,
  footerHeight = 44,
  padding = 16
): number {
  const overhead = gripHeight + topHeight + footerHeight + padding;
  return Math.max(0, sheetHeight - overhead);
}
