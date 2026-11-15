import { describe, expect, it } from 'vitest';
import { asciiBandGeometry, hexBandGeometry } from '@/components/viewers/hex-band-layout';
import { searchSpansForRow } from '@/components/viewers/hex-search-spans';

describe('hex band geometry', () => {
  it('covers a single byte including the inter-byte gap', () => {
    expect(hexBandGeometry(0, 1)).toEqual({ leftCh: 0, widthCh: 2 });
    expect(asciiBandGeometry(0, 1)).toEqual({ leftCh: 0, widthCh: 1 });
  });

  it('crosses the byte-8 group gap with one extra character', () => {
    expect(hexBandGeometry(6, 4)).toEqual({ leftCh: 18, widthCh: 12 });
    expect(asciiBandGeometry(6, 4)).toEqual({ leftCh: 6, widthCh: 5 });
  });

  it('offsets ranges that start after the group gap', () => {
    expect(hexBandGeometry(8, 1)).toEqual({ leftCh: 25, widthCh: 2 });
    expect(asciiBandGeometry(8, 1)).toEqual({ leftCh: 9, widthCh: 1 });
  });
});

describe('search spans per row', () => {
  it('clips matches to the row and keeps row-local indices', () => {
    const ranges = [
      { start: 4, end: 8 },
      { start: 14, end: 20 },
      { start: 32, end: 34 },
    ];
    expect(searchSpansForRow(ranges, 0, 16)).toEqual([
      { start: 4, end: 8 },
      { start: 14, end: 16 },
    ]);
    expect(searchSpansForRow(ranges, 0, 32)).toEqual([
      { start: 4, end: 8 },
      { start: 14, end: 20 },
    ]);
    expect(searchSpansForRow(ranges, 16, 32)).toEqual([{ start: 0, end: 4 }]);
    expect(searchSpansForRow(ranges, 32, 48)).toEqual([{ start: 0, end: 2 }]);
    expect(searchSpansForRow(ranges, 48, 64)).toEqual([]);
  });
});
