/** Share of `votes` in `total` as a whole percentage from 0 to 100. Zero when nothing is counted. */
export function pollPercent(votes: number, total: number): number {
  if (total <= 0 || votes <= 0) return 0;
  return Math.min(100, Math.round((votes / total) * 100));
}
