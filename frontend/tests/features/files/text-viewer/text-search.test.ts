import { describe, expect, it } from 'vitest';
import { indexTextSearch, searchRangesForLine } from '@/features/files/text-viewer/text-search';
import { indexTextDocument, pageForOffset, readTextPage, textPosition } from '@/features/files/text-viewer/text-pages';

describe('text preview indexing', () => {
  it('keeps exact page boundaries for trailing blank lines and CRLF text', () => {
    const content = 'item\r\n'.repeat(1000);
    const index = indexTextDocument(content);
    expect(index.totalLines).toBe(1001);
    expect(index.pages).toEqual([{ startOffset: 0, firstLineNumber: 1 }, { startOffset: content.length, firstLineNumber: 1001 }]);
    expect(readTextPage(content, index, 0).lines[0].content).toBe('item');
    expect(readTextPage(content, index, 1).lines[0].content).toBe('');
    expect(pageForOffset(index, content.length)).toBe(1);
  });

  it('treats regex punctuation as literal characters', () => {
    const content = 'a.*[x] aZZx a.*[x]';
    const matches = indexTextSearch(content, 'a.*[x]');
    expect(Array.from(matches.starts)).toEqual([0, 12]);
    expect(Array.from(matches.ends)).toEqual([6, 18]);
  });

  it('keeps original Unicode positions with case-insensitive matching', () => {
    const content = '\u0130 PASS 中文\tpass';
    const matches = indexTextSearch(content, 'pass');
    expect(Array.from(matches.starts)).toEqual([2, 10]);
    expect(content.slice(matches.starts[1], matches.ends[1])).toBe('pass');
    expect(textPosition(content, indexTextDocument(content), matches.starts[1])).toEqual({ line: 1, column: 11 });
  });

  it('shares occurrence identity across visual segments and never splits a surrogate pair', () => {
    const content = 'x'.repeat(8191) + '😀needle';
    const page = readTextPage(content, indexTextDocument(content), 0);
    expect(page.lines[0].content.length).toBe(8191);
    expect(page.lines[1].content).toBe('😀needle');
    const cross = 'x'.repeat(8190) + 'needle';
    const lines = readTextPage(cross, indexTextDocument(cross), 0).lines;
    const index = indexTextSearch(cross, 'needle');
    expect(lines.map((line) => searchRangesForLine(index, line.startOffset, line.endOffset, 0))).toEqual([
      [{ start: 8190, end: 8192, matchIndex: 0, active: true }],
      [{ start: 0, end: 4, matchIndex: 0, active: true }],
    ]);
  });

  it('grows compact buffers without dropping occurrences', () => {
    const matches = indexTextSearch('pass '.repeat(2000), 'pass');
    expect(matches.starts.length).toBe(2000);
    expect(matches.starts[1999]).toBe(9995);
    expect(searchRangesForLine(matches, 9990, 10000, 1999)).toHaveLength(2);
  });

  it('moves a visual split before a complete match while preserving text and the segment bound', () => {
    const content = 'x'.repeat(8190) + 'needle end';
    const search = indexTextSearch(content, 'needle');
    const page = readTextPage(content, indexTextDocument(content), 0, search);
    expect(page.lines.map((line) => line.content).join('')).toBe(content);
    expect(page.lines.every((line) => line.content.length <= 8192)).toBe(true);
    expect(page.lines[0].endOffset).toBe(8190);
    expect(page.lines[1].content).toBe('needle end');
    expect(searchRangesForLine(search, page.lines[1].startOffset, page.lines[1].endOffset, 0))
      .toEqual([{ start: 0, end: 6, matchIndex: 0, active: true }]);
  });
});
