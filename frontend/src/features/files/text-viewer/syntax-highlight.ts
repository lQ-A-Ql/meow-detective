import Prism from 'prismjs';
import 'prismjs/components/prism-python';
import 'prismjs/components/prism-powershell';
import 'prismjs/components/prism-bash';
import 'prismjs/components/prism-json';
import 'prismjs/components/prism-yaml';
import 'prismjs/components/prism-ini';
import 'prismjs/components/prism-javascript';
import 'prismjs/components/prism-typescript';
import type { Token, TokenStream } from 'prismjs';

export interface SyntaxRange {
  start: number;
  end: number;
  kind: string;
}

export interface SyntaxRangeIndex {
  starts: Uint32Array;
  ends: Uint32Array;
  kinds: string[];
}

export const MAX_SYNTAX_PAGE_LENGTH = 256 * 1024;

const LANGUAGE_BY_EXTENSION: Record<string, string> = {
  py: 'python',
  pyw: 'python',
  ps1: 'powershell',
  psm1: 'powershell',
  psd1: 'powershell',
  sh: 'bash',
  bash: 'bash',
  zsh: 'bash',
  ksh: 'bash',
  json: 'json',
  yaml: 'yaml',
  yml: 'yaml',
  ini: 'ini',
  cfg: 'ini',
  conf: 'ini',
  js: 'javascript',
  mjs: 'javascript',
  cjs: 'javascript',
  ts: 'typescript',
  mts: 'typescript',
  cts: 'typescript',
};

const KIND_BY_TOKEN: Record<string, string> = {
  keyword: 'keyword',
  string: 'string',
  comment: 'comment',
  number: 'number',
  boolean: 'number',
  constant: 'number',
  variable: 'variable',
  parameter: 'variable',
  property: 'variable',
  'attr-name': 'variable',
  function: 'function',
  'class-name': 'function',
  builtin: 'function',
  interpolation: 'variable',
  operator: 'operator',
  'interpolation-punctuation': 'punctuation',
  'template-punctuation': 'punctuation',
  punctuation: 'punctuation',
};

export function languageForExtension(extension?: string): string | undefined {
  if (!extension) return undefined;
  return LANGUAGE_BY_EXTENSION[extension.trim().toLowerCase().replace(/^\./, '')];
}

function appendToken(
  stream: TokenStream,
  offset: number,
  inheritedKind: string | undefined,
  starts: number[],
  ends: number[],
  kinds: string[],
): number {
  if (typeof stream === 'string') {
    if (inheritedKind && stream.length > 0) {
      starts.push(offset);
      ends.push(offset + stream.length);
      kinds.push(inheritedKind);
    }
    return offset + stream.length;
  }
  if (Array.isArray(stream)) {
    return stream.reduce((next, token) => appendToken(token, next, inheritedKind, starts, ends, kinds), offset);
  }
  const token = stream as Token;
  const kind = KIND_BY_TOKEN[token.type] ?? inheritedKind;
  return appendToken(token.content, offset, kind, starts, ends, kinds);
}

export function indexSyntaxRanges(pageText: string, language: string): SyntaxRangeIndex | undefined {
  const grammar = Prism.languages[language];
  if (!grammar || pageText.length > MAX_SYNTAX_PAGE_LENGTH) return undefined;
  const starts: number[] = [];
  const ends: number[] = [];
  const kinds: string[] = [];
  appendToken(Prism.tokenize(pageText, grammar), 0, undefined, starts, ends, kinds);
  return { starts: Uint32Array.from(starts), ends: Uint32Array.from(ends), kinds };
}

/** Same binary intersection as the search index, rebased to a line-local segment. */
export function syntaxRangesForLine(
  index: SyntaxRangeIndex | undefined,
  start: number,
  end: number,
  pageStartOffset: number,
): SyntaxRange[] {
  if (!index || index.starts.length === 0) return [];
  let low = 0;
  let high = index.ends.length;
  while (low < high) {
    const middle = Math.floor((low + high) / 2);
    if (index.ends[middle] <= start - pageStartOffset) low = middle + 1;
    else high = middle;
  }
  const ranges: SyntaxRange[] = [];
  for (let i = low; i < index.starts.length; i += 1) {
    const rangeStart = index.starts[i] + pageStartOffset;
    if (rangeStart >= end) break;
    ranges.push({
      start: Math.max(start, rangeStart) - start,
      end: Math.min(end, index.ends[i] + pageStartOffset) - start,
      kind: index.kinds[i],
    });
  }
  return ranges;
}
