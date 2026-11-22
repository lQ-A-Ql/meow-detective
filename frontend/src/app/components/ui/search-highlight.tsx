import { Fragment, type ReactNode } from 'react';

export interface SearchHighlightRange {
  start: number;
  end: number;
  matchIndex: number;
  active: boolean;
}

export function SearchMatch({
  children,
  matchIndex,
  active,
}: {
  children: ReactNode;
  matchIndex: number;
  active: boolean;
}) {
  return (
    <mark
      className="text-search-match"
      data-match-index={matchIndex}
      data-current={active ? 'true' : undefined}
      aria-current={active ? 'true' : undefined}
    >
      {children}
    </mark>
  );
}

/** One inline highlight primitive: never changes glyph widths or row height. */
export function SearchHighlight({ text, ranges }: { text: string; ranges: readonly SearchHighlightRange[] }) {
  if (ranges.length === 0) return <>{text || '\u00A0'}</>;
  let cursor = 0;
  const fragments = ranges.map((range) => {
    const before = text.slice(cursor, range.start);
    const match = text.slice(range.start, range.end);
    cursor = range.end;
    return (
      <Fragment key={range.matchIndex}>
        {before}
        <SearchMatch matchIndex={range.matchIndex} active={range.active}>
          {match}
        </SearchMatch>
      </Fragment>
    );
  });
  return (
    <>
      {fragments}
      {text.slice(cursor)}
    </>
  );
}
