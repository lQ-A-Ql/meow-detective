export interface SearchSpan {
  start: number;
  end: number;
}

export function searchSpansForRow(
  ranges: ReadonlyArray<{ start: number; end: number }>,
  rowStart: number,
  rowEnd: number,
): SearchSpan[] {
  let low = 0;
  let high = ranges.length;
  while (low < high) {
    const middle = Math.floor((low + high) / 2);
    if (ranges[middle].end <= rowStart) low = middle + 1;
    else high = middle;
  }
  const spans: SearchSpan[] = [];
  for (let i = low; i < ranges.length && ranges[i].start < rowEnd; i += 1) {
    const start = Math.max(rowStart, ranges[i].start) - rowStart;
    const end = Math.min(rowEnd, ranges[i].end) - rowStart;
    if (end > start) spans.push({ start, end });
  }
  return spans;
}
