import type { HexLoadedRange } from '@/types/models';

/**
 * Merge a new range into existing loaded ranges, coalescing adjacent or overlapping ranges.
 *
 * @param ranges - Existing loaded ranges
 * @param nextRange - New range to merge
 * @returns Merged ranges sorted by start offset
 *
 * @example
 * mergeLoadedRanges([{start: 0, end: 100}], {start: 100, end: 200})
 * // => [{start: 0, end: 200}]
 *
 * mergeLoadedRanges([{start: 0, end: 100}, {start: 200, end: 300}], {start: 50, end: 250})
 * // => [{start: 0, end: 300}]
 */
export function mergeLoadedRanges(ranges: HexLoadedRange[], nextRange: HexLoadedRange): HexLoadedRange[] {
  const all = [...ranges, nextRange].sort((left, right) => left.start - right.start);
  const merged: HexLoadedRange[] = [];

  for (const range of all) {
    const last = merged[merged.length - 1];
    if (!last || range.start > last.end) {
      merged.push({ ...range });
      continue;
    }
    last.end = Math.max(last.end, range.end);
  }

  return merged;
}
