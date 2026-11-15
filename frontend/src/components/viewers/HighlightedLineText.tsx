import { Fragment } from 'react';
import type { SearchHighlightRange } from '@/app/components/ui/search-highlight';
import type { SyntaxRange } from '@/features/files/text-viewer/syntax-highlight';

interface Props {
  text: string;
  syntax: readonly SyntaxRange[];
  search: readonly SearchHighlightRange[];
}

type Piece =
  | { kind: 'search'; start: number; end: number; range: SearchHighlightRange }
  | { kind: 'syntax'; start: number; end: number; token: string };

function mergeBoundaries(textLength: number, syntax: readonly SyntaxRange[], search: readonly SearchHighlightRange[]) {
  const boundaries = new Set<number>([0, textLength]);
  for (const range of syntax) {
    boundaries.add(Math.max(0, range.start));
    boundaries.add(Math.min(textLength, range.end));
  }
  for (const range of search) {
    boundaries.add(Math.max(0, range.start));
    boundaries.add(Math.min(textLength, range.end));
  }
  return Array.from(boundaries).sort((a, b) => a - b);
}

export function HighlightedLineText({ text, syntax, search }: Props) {
  if (!text) return <>{'\u00A0'}</>;
  if (syntax.length === 0 && search.length === 0) return <>{text}</>;

  const boundaries = mergeBoundaries(text.length, syntax, search);
  const pieces: Piece[] = [];
  let searchIndex = 0;
  let syntaxIndex = 0;
  for (let i = 0; i < boundaries.length - 1; i += 1) {
    const start = boundaries[i];
    const end = boundaries[i + 1];
    if (end <= start) continue;
    while (searchIndex < search.length && search[searchIndex].end <= start) searchIndex += 1;
    const searchRange = search[searchIndex];
    if (searchRange && searchRange.start <= start && searchRange.end > start) {
      const previous = pieces[pieces.length - 1];
      if (
        previous?.kind === 'search' &&
        previous.end === start &&
        previous.range.matchIndex === searchRange.matchIndex
      ) {
        previous.end = end;
      } else {
        pieces.push({ kind: 'search', start, end, range: searchRange });
      }
      continue;
    }
    while (syntaxIndex < syntax.length && syntax[syntaxIndex].end <= start) syntaxIndex += 1;
    const syntaxRange = syntax[syntaxIndex];
    if (syntaxRange && syntaxRange.start <= start && syntaxRange.end > start) {
      const previous = pieces[pieces.length - 1];
      if (previous?.kind === 'syntax' && previous.end === start && previous.token === syntaxRange.kind) {
        previous.end = end;
      } else {
        pieces.push({ kind: 'syntax', start, end, token: syntaxRange.kind });
      }
    }
  }

  let cursor = 0;
  return (
    <>
      {pieces.map((piece) => {
        const before = text.slice(cursor, piece.start);
        const content = text.slice(piece.start, piece.end);
        cursor = piece.end;
        if (piece.kind === 'search') {
          return (
            <Fragment key={`search-${piece.range.matchIndex}`}>
              {before}
              <mark
                className="text-search-match"
                data-match-index={piece.range.matchIndex}
                data-current={piece.range.active ? 'true' : undefined}
                aria-current={piece.range.active ? 'true' : undefined}
              >
                {content}
              </mark>
            </Fragment>
          );
        }
        return (
          <Fragment key={`syntax-${piece.start}-${piece.token}`}>
            {before}
            <span data-token={piece.token}>{content}</span>
          </Fragment>
        );
      })}
      {text.slice(cursor)}
    </>
  );
}
