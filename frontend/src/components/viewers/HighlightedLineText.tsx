import { Fragment } from 'react';
import { SearchMatch } from '@/app/components/ui/search-highlight';
import type { HighlightPiece } from '@/features/files/text-viewer/highlight-pieces';

export function HighlightedLineText({ pieces }: { pieces: readonly HighlightPiece[] }) {
  return (
    <>
      {pieces.map((piece) => {
        if (piece.kind === 'search') {
          return (
            <SearchMatch key={piece.start} matchIndex={piece.matchIndex} active={piece.active}>
              {piece.text}
            </SearchMatch>
          );
        }
        if (piece.kind === 'syntax')
          return (
            <span key={piece.start} data-token={piece.token}>
              {piece.text}
            </span>
          );
        return <Fragment key={piece.start}>{piece.text}</Fragment>;
      })}
    </>
  );
}
