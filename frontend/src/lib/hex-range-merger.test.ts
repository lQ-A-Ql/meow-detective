import { describe, expect, it } from 'vitest';
import { mergeLoadedRanges } from './hex-range-merger';

describe('mergeLoadedRanges', () => {
  it('adds first range to empty list', () => {
    expect(mergeLoadedRanges([], { start: 0, end: 100 })).toEqual([{ start: 0, end: 100 }]);
  });

  it('merges adjacent ranges into one', () => {
    expect(
      mergeLoadedRanges([{ start: 0, end: 100 }], { start: 100, end: 200 }),
    ).toEqual([{ start: 0, end: 200 }]);
  });

  it('merges overlapping ranges', () => {
    expect(
      mergeLoadedRanges(
        [
          { start: 0, end: 100 },
          { start: 200, end: 300 },
        ],
        { start: 50, end: 250 },
      ),
    ).toEqual([{ start: 0, end: 300 }]);
  });

  it('keeps disjoint ranges separate', () => {
    expect(
      mergeLoadedRanges(
        [
          { start: 0, end: 100 },
          { start: 200, end: 300 },
        ],
        { start: 400, end: 500 },
      ),
    ).toEqual([
      { start: 0, end: 100 },
      { start: 200, end: 300 },
      { start: 400, end: 500 },
    ]);
  });

  it('sorts ranges by start offset after merge', () => {
    expect(
      mergeLoadedRanges(
        [
          { start: 1000, end: 2000 },
          { start: 0, end: 500 },
        ],
        { start: 500, end: 1000 },
      ),
    ).toEqual([
      { start: 0, end: 2000 },
    ]);
  });

  it('extends existing range when new range ends later', () => {
    expect(
      mergeLoadedRanges([{ start: 1024, end: 2048 }], { start: 0, end: 3072 }),
    ).toEqual([{ start: 0, end: 3072 }]);
  });

  it('does not mutate input ranges', () => {
    const initial = [{ start: 0, end: 100 }];
    const result = mergeLoadedRanges(initial, { start: 100, end: 200 });
    expect(initial).toEqual([{ start: 0, end: 100 }]);
    expect(result).toEqual([{ start: 0, end: 200 }]);
  });
});
