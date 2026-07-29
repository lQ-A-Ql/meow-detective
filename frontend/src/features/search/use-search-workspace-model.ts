import { useCallback, useEffect, useMemo, useState } from 'react';
import { useSearchParams } from 'react-router';
import { useInfiniteSearchResults } from '@/features/search/hooks';
import { useOpenSearchHitInFiles, useSearchSelection } from '@/features/search/use-search-page-model';
import {
  readSavedSearchQueries,
  removeSavedSearchQuery,
  upsertSavedSearchQuery,
  writeSavedSearchQueries,
} from '@/lib/saved-queries';
import type { SearchHit } from '@/types/models';

const DEFAULT_QUERY = 'content:password AND path:doc';

/** Owns search query state, persisted saved queries, selection, and navigation. */
export function useSearchWorkspaceModel() {
  const [searchParams] = useSearchParams();
  const urlQuery = searchParams.get('q');
  const initialQuery = urlQuery?.trim() || DEFAULT_QUERY;
  const [queryInput, setQueryInput] = useState(initialQuery);
  const [activeQuery, setActiveQuery] = useState(initialQuery);
  const [savedOpen, setSavedOpen] = useState(false);
  const [savedName, setSavedName] = useState('');
  const [savedQueries, setSavedQueries] = useState(() => readSavedSearchQueries());
  const searchQuery = useInfiniteSearchResults(activeQuery);
  const searchHits = useMemo(
    () => searchQuery.data?.pages.flatMap((page) => page.items) ?? [],
    [searchQuery.data],
  );
  const totalHits = searchQuery.data?.pages[0]?.total ?? 0;
  const searchTookMs = searchQuery.data?.pages.reduce(
    (total, page) => total + page.tookMs,
    0,
  ) ?? 0;
  const highScoreHits = useMemo(
    () => searchHits.filter((hit) => hit.score >= 0.8).length,
    [searchHits],
  );
  const { selectedSearchHitId, setSelectedSearchHitId } = useSearchSelection();
  const openSearchHitInFiles = useOpenSearchHitInFiles();
  const selectedHit = searchHits.find((hit) => hit.fileId === selectedSearchHitId) ?? searchHits[0];

  useEffect(() => {
    const nextQuery = urlQuery?.trim() || DEFAULT_QUERY;
    setQueryInput(nextQuery);
    setActiveQuery(nextQuery);
  }, [urlQuery]);

  const submitQuery = useCallback(() => {
    setActiveQuery(queryInput);
  }, [queryInput]);
  const onHitRowClick = useCallback(
    (hit: SearchHit) => setSelectedSearchHitId(hit.fileId),
    [setSelectedSearchHitId],
  );
  const toggleSavedQueries = useCallback(() => {
    setSavedOpen((open) => !open);
  }, []);
  const saveCurrentQuery = useCallback(() => {
    const name = savedName.trim() || queryInput.slice(0, 48);
    const nextQueries = upsertSavedSearchQuery(savedQueries, name, queryInput);
    setSavedQueries(nextQueries);
    writeSavedSearchQueries(nextQueries);
    setSavedName('');
    setSavedOpen(true);
  }, [queryInput, savedName, savedQueries]);
  const runSavedQuery = useCallback((query: string) => {
    setQueryInput(query);
    setActiveQuery(query);
    setSavedOpen(false);
  }, []);
  const removeSavedQuery = useCallback((id: string) => {
    setSavedQueries((currentQueries) => {
      const nextQueries = removeSavedSearchQuery(currentQueries, id);
      writeSavedSearchQueries(nextQueries);
      return nextQueries;
    });
  }, []);
  const openSelectedHitInFiles = useCallback(() => {
    if (selectedHit) {
      openSearchHitInFiles(selectedHit.fileId);
    }
  }, [openSearchHitInFiles, selectedHit]);
  const loadNextPage = useCallback(() => {
    void searchQuery.fetchNextPage();
  }, [searchQuery]);
  const retry = useCallback(() => {
    void searchQuery.refetch();
  }, [searchQuery]);

  return {
    activeQuery,
    highScoreHits,
    hasMore: searchQuery.hasNextPage,
    initialLoadFailed: searchQuery.isError && searchHits.length === 0,
    loadContextKey: activeQuery,
    loadMoreFailed: searchQuery.isFetchNextPageError,
    loadNextPage,
    loadStateKey: searchQuery.dataUpdatedAt,
    loadingMore: searchQuery.isFetchingNextPage,
    onHitRowClick,
    openSelectedHitInFiles,
    queryInput,
    removeSavedQuery,
    retry,
    runSavedQuery,
    saveCurrentQuery,
    savedName,
    savedOpen,
    savedQueries,
    searchHits,
    searchTookMs,
    selectedHit,
    setQueryInput,
    setSavedName,
    submitQuery,
    toggleSavedQueries,
    totalHits,
  };
}

export type SearchWorkspaceModel = ReturnType<typeof useSearchWorkspaceModel>;
