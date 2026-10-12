import { useCallback, useEffect, useMemo, useState } from 'react';
import { indexTextDocument, pageForOffset, readTextPage, textPosition } from './text-pages';
import { indexTextSearch } from './text-search';
import type { TextDocumentIndex, TextScrollRequest, TextSearchIndex, TextViewerProps } from './types';

interface Location {
  index: TextSearchIndex;
  document: TextDocumentIndex;
  page: number;
  active: number;
  request: TextScrollRequest;
}

export function useTextViewerModel({ content, documentKey, searchQuery }: TextViewerProps) {
  const [search, setSearch] = useState({ content, documentKey, external: searchQuery, value: searchQuery ?? '' });
  const sameDocument = search.content === content && search.documentKey === documentKey && search.external === searchQuery;
  const query = sameDocument ? search.value : searchQuery ?? '';
  const document = useMemo(() => indexTextDocument(content), [content, documentKey]);
  const index = useMemo(() => indexTextSearch(content, query), [content, query, documentKey]);
  const initialLocation = useMemo<Location>(() => index.starts.length > 0
    ? { index, document, page: pageForOffset(document, index.starts[0]), active: 0,
      request: { kind: 'match', startOffset: index.starts[0], matchIndex: 0 } }
    : { index, document, page: 0, active: -1, request: { kind: 'top' } }, [index, document]);
  const [storedLocation, setLocation] = useState(initialLocation);
  const location = useMemo<Location>(() => {
    if (storedLocation.index === index) return storedLocation;
    if (index.starts.length > 0 || storedLocation.document !== document) return initialLocation;
    return { index, document, page: storedLocation.page, active: -1, request: { kind: 'keep' } };
  }, [storedLocation, index, document, initialLocation]);

  // Effective state above changes immediately; effects only release stale document references.
  useEffect(() => {
    setSearch((current) => current.content === content && current.documentKey === documentKey && current.external === searchQuery
      ? current : { content, documentKey, external: searchQuery, value: searchQuery ?? '' });
  }, [content, documentKey, searchQuery]);
  useEffect(() => {
    setLocation((current) => current.index === index ? current : location);
  }, [index, location]);

  const setQuery = useCallback((value: string) => {
    setSearch({ content, documentKey, external: searchQuery, value });
  }, [content, documentKey, searchQuery]);
  const moveMatch = useCallback((direction: 1 | -1) => {
    if (index.starts.length === 0) return;
    setLocation((current) => {
      const previous = current.index === index ? current : initialLocation;
      const active = (previous.active + direction + index.starts.length) % index.starts.length;
      const startOffset = index.starts[active];
      return { index, document, page: pageForOffset(document, startOffset), active,
        request: { kind: 'match', startOffset, matchIndex: active } };
    });
  }, [index, document, initialLocation]);
  const movePage = useCallback((direction: 1 | -1) => {
    setLocation((current) => {
      const previous = current.index === index ? current : initialLocation;
      const page = Math.max(0, Math.min(document.pages.length - 1, previous.page + direction));
      return { ...previous, page, request: { kind: 'top' } };
    });
  }, [document.pages.length, index, initialLocation]);

  const page = useMemo(() => readTextPage(content, document, location.page, index), [content, document, location.page, index]);
  const position = useMemo(() => location.active < 0 ? undefined : textPosition(content, document, index.starts[location.active]),
    [content, document, index, location.active]);
  return { query, setQuery, index, page, pageNumber: location.page, pageCount: document.pages.length,
    totalLines: document.totalLines, activeMatchIndex: location.active, matchCount: index.starts.length,
    position, scrollRequest: location.request, moveMatch, movePage };
}

export type TextViewerModel = ReturnType<typeof useTextViewerModel>;
