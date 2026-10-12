import type { TextDocumentIndex, TextPage, TextPageLine, TextSearchIndex } from './types';

export const TEXT_PAGE_SIZE = 1000;
export const MAX_TEXT_SEGMENT_LENGTH = 8 * 1024;

/** Index page boundaries without allocating an array for every logical line. */
export function indexTextDocument(content: string): TextDocumentIndex {
  const pages = [{ startOffset: 0, firstLineNumber: 1 }];
  let totalLines = 1;
  let cursor = content.indexOf('\n');
  while (cursor !== -1) {
    totalLines += 1;
    if ((totalLines - 1) % TEXT_PAGE_SIZE === 0) pages.push({ startOffset: cursor + 1, firstLineNumber: totalLines });
    cursor = content.indexOf('\n', cursor + 1);
  }
  return { pages, totalLines };
}

export function pageForOffset(index: TextDocumentIndex, offset: number): number {
  let low = 0;
  let high = index.pages.length;
  while (low < high) {
    const middle = Math.floor((low + high) / 2);
    if (index.pages[middle].startOffset <= offset) low = middle + 1;
    else high = middle;
  }
  return Math.max(0, low - 1);
}

function avoidSplittingMatch(start: number, end: number, search?: TextSearchIndex): number {
  if (!search || search.starts.length === 0) return end;
  let low = 0;
  let high = search.starts.length;
  while (low < high) {
    const middle = Math.floor((low + high) / 2);
    if (search.starts[middle] < end) low = middle + 1;
    else high = middle;
  }
  const match = low - 1;
  return match >= 0 && search.starts[match] > start && search.ends[match] > end
    && search.ends[match] - search.starts[match] <= MAX_TEXT_SEGMENT_LENGTH ? search.starts[match] : end;
}

export function readTextPage(content: string, index: TextDocumentIndex, pageNumber: number, search?: TextSearchIndex): TextPage {
  const boundary = index.pages[pageNumber];
  const startLine = boundary.firstLineNumber;
  const endLine = Math.min(index.totalLines, startLine + TEXT_PAGE_SIZE - 1);
  const lines: TextPageLine[] = [];
  let cursor = boundary.startOffset;
  for (let lineNumber = startLine; lineNumber <= endLine; lineNumber += 1) {
    const newline = content.indexOf('\n', cursor);
    const rawEnd = newline === -1 ? content.length : newline;
    // CRLF is one line separator; offsets remain in the unmodified preview string.
    const end = newline !== -1 && content.charCodeAt(rawEnd - 1) === 13 ? rawEnd - 1 : rawEnd;
    let segmentStart = cursor;
    do {
      let segmentEnd = Math.min(end, segmentStart + MAX_TEXT_SEGMENT_LENGTH);
      if (segmentEnd < end && /[\uD800-\uDBFF]/.test(content[segmentEnd - 1]) && /[\uDC00-\uDFFF]/.test(content[segmentEnd])) segmentEnd -= 1;
      if (segmentEnd < end) segmentEnd = avoidSplittingMatch(segmentStart, segmentEnd, search);
      lines.push({ content: content.slice(segmentStart, segmentEnd), startOffset: segmentStart, endOffset: segmentEnd,
        logicalStartOffset: cursor, lineNumber, continuation: segmentStart !== cursor });
      segmentStart = segmentEnd;
    } while (segmentStart < end);
    cursor = rawEnd + 1;
  }
  return { lines, startLine, endLine };
}

export function rowForOffset(lines: readonly TextPageLine[], offset: number): number {
  let low = 0;
  let high = lines.length;
  while (low < high) {
    const middle = Math.floor((low + high) / 2);
    if (lines[middle].startOffset <= offset) low = middle + 1;
    else high = middle;
  }
  return Math.max(0, low - 1);
}

export function textPosition(content: string, index: TextDocumentIndex, offset: number) {
  const boundary = index.pages[pageForOffset(index, offset)];
  let line = boundary.firstLineNumber;
  let lineStart = boundary.startOffset;
  let newline = content.indexOf('\n', lineStart);
  while (newline !== -1 && newline < offset) {
    line += 1;
    lineStart = newline + 1;
    newline = content.indexOf('\n', lineStart);
  }
  return { line, column: Array.from(content.slice(lineStart, offset)).length + 1 };
}
