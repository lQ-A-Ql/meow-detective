import { describe, expect, it } from 'vitest';
import {
  MAX_SYNTAX_PAGE_LENGTH,
  indexSyntaxRanges,
  languageForExtension,
  syntaxRangesForLine,
} from '@/features/files/text-viewer/syntax-highlight';

function tokensFor(text: string, language: string) {
  const index = indexSyntaxRanges(text, language);
  if (!index) return [];
  return Array.from(index.kinds).map((kind, i) => ({
    kind,
    text: text.slice(index.starts[i], index.ends[i]),
  }));
}

describe('syntax highlighting', () => {
  it('maps preview extensions to Prism languages case-insensitively', () => {
    expect(languageForExtension('py')).toBe('python');
    expect(languageForExtension('.PY')).toBe('python');
    expect(languageForExtension('psm1')).toBe('powershell');
    expect(languageForExtension('zsh')).toBe('bash');
    expect(languageForExtension('yml')).toBe('yaml');
    expect(languageForExtension('conf')).toBe('ini');
    expect(languageForExtension('mjs')).toBe('javascript');
    expect(languageForExtension('cts')).toBe('typescript');
    expect(languageForExtension('log')).toBeUndefined();
    expect(languageForExtension()).toBeUndefined();
  });

  it('indexes python keywords, functions, strings, and numbers', () => {
    const text = 'def greet(name):\n    return "hi" + 42\n';
    const tokens = tokensFor(text, 'python');
    expect(tokens).toContainEqual({ kind: 'keyword', text: 'def' });
    expect(tokens).toContainEqual({ kind: 'keyword', text: 'return' });
    expect(tokens).toContainEqual({ kind: 'function', text: 'greet' });
    expect(tokens).toContainEqual({ kind: 'string', text: '"hi"' });
    expect(tokens).toContainEqual({ kind: 'number', text: '42' });
  });

  it('flattens nested tokens and keeps inner highlights', () => {
    const tokens = tokensFor('const label = `hello ${name}`;', 'javascript');
    expect(tokens.some((token) => token.kind === 'string' && token.text.includes('hello'))).toBe(true);
    expect(tokens.some((token) => token.kind === 'variable' && token.text === 'name')).toBe(true);
  });

  it('skips pages above the tokenization cap', () => {
    expect(indexSyntaxRanges('x'.repeat(MAX_SYNTAX_PAGE_LENGTH + 1), 'python')).toBeUndefined();
    expect(indexSyntaxRanges('def x():', 'unknown-language')).toBeUndefined();
  });

  it('rebases intersecting ranges onto a line segment', () => {
    const page = 'def greet():\n    return 42';
    const index = indexSyntaxRanges(page, 'python');
    const lineStart = page.indexOf('return');
    const lineEnd = page.length;
    const ranges = syntaxRangesForLine(index, lineStart, lineEnd, 0);
    expect(ranges).toContainEqual({ start: 0, end: 6, kind: 'keyword' });
    expect(
      ranges.some(
        (range) => range.kind === 'number' && page.slice(lineStart + range.start, lineStart + range.end) === '42',
      ),
    ).toBe(true);
    expect(ranges.every((range) => range.start >= 0 && range.end <= lineEnd - lineStart)).toBe(true);
  });
});
