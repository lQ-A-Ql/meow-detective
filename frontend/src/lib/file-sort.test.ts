import { describe, it, expect } from 'vitest';
import { sortFileEntries, naturalCompare } from '@/lib/file-sort';
import type { FileEntryRow } from '@/types/models';

function row(partial: Partial<FileEntryRow> & { id: string; name: string }): FileEntryRow {
  return {
    entryType: 'file',
    deleted: false,
    hidden: false,
    system: false,
    ...partial,
  } as FileEntryRow;
}

describe('naturalCompare', () => {
  it('orders numeric suffixes like Explorer (file2 < file10)', () => {
    expect(naturalCompare('file2', 'file10')).toBeLessThan(0);
    expect(naturalCompare('file10', 'file2')).toBeGreaterThan(0);
  });

  it('is case-insensitive then deterministic on raw chars', () => {
    expect(naturalCompare('Alpha', 'alpha')).toBeLessThan(0);
    expect(naturalCompare('a', 'a')).toBe(0);
  });
});

describe('sortFileEntries', () => {
  it('keeps directories before files even when descending', () => {
    const rows = [
      row({ id: '1', name: 'zeta.txt', entryType: 'file' }),
      row({ id: '2', name: 'alpha', entryType: 'directory' }),
      row({ id: '3', name: 'beta', entryType: 'directory' }),
    ];
    const ordered = sortFileEntries(rows, 'name', 'desc').map((r) => r.name);
    expect(ordered).toEqual(['beta', 'alpha', 'zeta.txt']);
  });

  it('applies natural name order to files', () => {
    const rows = [
      row({ id: '1', name: 'file10.log' }),
      row({ id: '2', name: 'file2.log' }),
      row({ id: '3', name: 'file1.log' }),
    ];
    const ordered = sortFileEntries(rows, 'name', 'asc').map((r) => r.name);
    expect(ordered).toEqual(['file1.log', 'file2.log', 'file10.log']);
  });

  it('sinks hidden/system then deleted then both after normal entries', () => {
    const rows = [
      row({ id: '1', name: 'normal.txt' }),
      row({ id: '2', name: 'deleted.txt', deleted: true }),
      row({ id: '3', name: 'hidden.txt', hidden: true }),
      row({ id: '4', name: 'both.txt', hidden: true, deleted: true }),
    ];
    const ordered = sortFileEntries(rows, 'name', 'asc').map((r) => r.name);
    expect(ordered).toEqual(['normal.txt', 'hidden.txt', 'deleted.txt', 'both.txt']);
  });

  it('keeps status buckets fixed under descending sort', () => {
    const rows = [
      row({ id: '1', name: 'aaa.txt', hidden: true }),
      row({ id: '2', name: 'zzz.txt' }),
    ];
    const ordered = sortFileEntries(rows, 'name', 'desc').map((r) => r.name);
    // Normal bucket precedes hidden bucket regardless of direction.
    expect(ordered).toEqual(['zzz.txt', 'aaa.txt']);
  });

  it('sorts by size descending within files', () => {
    const rows = [
      row({ id: '1', name: 'small.bin', size: 10 }),
      row({ id: '2', name: 'big.bin', size: 9000 }),
      row({ id: '3', name: 'mid.bin', size: 500 }),
    ];
    const ordered = sortFileEntries(rows, 'size', 'desc').map((r) => r.name);
    expect(ordered).toEqual(['big.bin', 'mid.bin', 'small.bin']);
  });
});
