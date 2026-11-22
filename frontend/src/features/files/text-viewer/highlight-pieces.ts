import type { SearchHighlightRange } from '@/app/components/ui/search-highlight';
import type { SyntaxRange } from './syntax-highlight';

export type HighlightPiece = { start: number; text: string } & (
  { kind: 'plain' } | { kind: 'syntax'; token: string } | { kind: 'search'; matchIndex: number; active: boolean }
);

export function highlightPieces(
  text: string,
  syntax: readonly SyntaxRange[],
  search: readonly SearchHighlightRange[],
): HighlightPiece[] {
  if (!text) return [{ kind: 'plain', start: 0, text: '\u00A0' }];
  const boundaries = new Set([0, text.length]);
  for (const range of [...syntax, ...search]) {
    boundaries.add(Math.min(text.length, Math.max(0, range.start)));
    boundaries.add(Math.min(text.length, Math.max(0, range.end)));
  }
  const points = [...boundaries].sort((left, right) => left - right);
  const pieces: HighlightPiece[] = [];
  let searchIndex = 0;
  let syntaxIndex = 0;
  for (let index = 0; index + 1 < points.length; index += 1) {
    const start = points[index];
    const end = points[index + 1];
    if (start === end) continue;
    while (searchIndex < search.length && search[searchIndex].end <= start) searchIndex += 1;
    while (syntaxIndex < syntax.length && syntax[syntaxIndex].end <= start) syntaxIndex += 1;
    const match = search[searchIndex];
    const token = syntax[syntaxIndex];
    const value = text.slice(start, end);
    if (match && match.start <= start && match.end > start) {
      const previous = pieces[pieces.length - 1];
      if (previous?.kind === 'search' && previous.matchIndex === match.matchIndex) previous.text += value;
      else pieces.push({ kind: 'search', start, text: value, matchIndex: match.matchIndex, active: match.active });
    } else if (token && token.start <= start && token.end > start) {
      pieces.push({ kind: 'syntax', start, text: value, token: token.kind });
    } else {
      pieces.push({ kind: 'plain', start, text: value });
    }
  }
  return pieces;
}
