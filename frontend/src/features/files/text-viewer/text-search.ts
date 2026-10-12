import type { SearchHighlightRange } from '@/app/components/ui/search-highlight';
import type { TextSearchIndex } from './types';

/** Literal, case-insensitive matches retain offsets in the original string.
 * Compact numeric buffers avoid an object allocation for every occurrence. */
export function indexTextSearch(content: string, query: string): TextSearchIndex {
  if (!query) return { starts: new Uint32Array(), ends: new Uint32Array() };
  const pattern = new RegExp(query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'giu');
  let capacity = 256;
  let starts = new Uint32Array(capacity);
  let ends = new Uint32Array(capacity);
  let count = 0;
  for (const match of content.matchAll(pattern)) {
    if (count === capacity) {
      capacity *= 2;
      const nextStarts = new Uint32Array(capacity);
      const nextEnds = new Uint32Array(capacity);
      nextStarts.set(starts);
      nextEnds.set(ends);
      starts = nextStarts;
      ends = nextEnds;
    }
    starts[count] = match.index;
    ends[count] = match.index + match[0].length;
    count += 1;
  }
  return { starts: starts.subarray(0, count), ends: ends.subarray(0, count) };
}

/** Intersections also preserve matches crossing an 8,192-code-unit visual segment boundary. */
export function searchRangesForLine(index: TextSearchIndex, start: number, end: number, active: number): SearchHighlightRange[] {
  let low = 0;
  let high = index.ends.length;
  while (low < high) {
    const middle = Math.floor((low + high) / 2);
    if (index.ends[middle] <= start) low = middle + 1;
    else high = middle;
  }
  const ranges: SearchHighlightRange[] = [];
  for (let i = low; i < index.starts.length && index.starts[i] < end; i += 1) {
    ranges.push({ start: Math.max(start, index.starts[i]) - start, end: Math.min(end, index.ends[i]) - start,
      matchIndex: i, active: i === active });
  }
  return ranges;
}
