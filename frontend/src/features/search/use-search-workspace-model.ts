import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useSearchParams } from 'react-router';
import { useCurrentCase, useDataSources } from '@/features/case/hooks';
import { useContentIndexModel } from './use-content-index-model';
import { MAX_LOADED_SEARCH_RESULTS, useInfiniteSearchResults } from '@/features/search/hooks';
import { useFileNavigation } from '@/features/files/hooks/use-file-navigation';
import { useQueryClient } from '@tanstack/react-query';
import { useSearchSelection } from '@/features/search/use-search-page-model';
import { useSearchPreviewModel } from '@/features/search/use-search-preview-model';
import type {
  SearchFileHit,
  SearchRequestOptions,
  SearchSortDirection,
  SearchSortKey,
} from '@/types/models';

/**
 * A single click opens the preview dialog, a double click locates the file in the
 * browser. The click handler therefore waits out the double-click window before
 * opening the modal: a modal that opened on the first click would cover the grid and
 * swallow the second click, so "double-click to locate" could never fire.
 */
const SINGLE_CLICK_PREVIEW_DELAY_MS = 220;

const DEFAULT_QUERY = '';
const DEFAULT_OPTIONS: SearchRequestOptions = {
  matchPath: false,
  entryType: 'any',
  extensions: [],
  dataSourceIds: [],
  sortKey: 'name',
  sortDirection: 'asc',
};

export function useSearchWorkspaceModel() {
  const [searchParams] = useSearchParams();
  const urlQuery = searchParams.get('q');
  const initialQuery = urlQuery?.trim() || DEFAULT_QUERY;
  const [queryInput, setQueryInput] = useState(initialQuery);
  const [activeQuery, setActiveQuery] = useState(initialQuery);
  const [extensionInput, setExtensionInputState] = useState('');
  const [options, setOptions] = useState<SearchRequestOptions>(DEFAULT_OPTIONS);
  const { data: dataSources } = useDataSources();
  const { data: currentCase } = useCurrentCase();
  const cache = useQueryClient();
  const caseId = currentCase?.id;
  const [owner, setOwner] = useState(caseId);
  if (owner !== caseId) {
    setOwner(caseId);
    setOptions(DEFAULT_OPTIONS);
    setExtensionInputState('');
  }
  const indexSources = useMemo(() => (dataSources ?? []).filter((source) => !source.importState || source.importState === 'ready'), [dataSources]);
  const [indexTarget, setIndexTarget] = useState<{ caseId?: string; sourceId?: string }>({ caseId });
  const indexSourceId = indexTarget.caseId === caseId && indexSources.some((source) => source.id === indexTarget.sourceId)
    ? indexTarget.sourceId : indexSources[0]?.id;
  const contentIndex = useContentIndexModel(caseId, indexSourceId);
  const fileNavigation = useFileNavigation(caseId);
  const searchQuery = useInfiniteSearchResults(caseId, activeQuery, 100, options);
  const searchHits = useMemo(
    () => Array.from(new Map(searchQuery.data?.pages.flatMap((page) => page.items).map((hit) => [hit.fileId, hit] as const) ?? []).values()),
    [searchQuery.data],
  );
  const firstPage = searchQuery.data?.pages[0];
  const { selectedSearchHitId, setSelectedSearchHitId } = useSearchSelection();
  const preview = useSearchPreviewModel(caseId);
  const { openHit: openHitPreview } = preview;
  const selectedHit = searchHits.find((hit) => hit.fileId === selectedSearchHitId);
  const pages = searchQuery.data?.pages;
  const localLimitReached = searchHits.length >= MAX_LOADED_SEARCH_RESULTS && Boolean(pages?.[pages.length - 1]?.nextCursor);

  useEffect(() => {
    const nextQuery = urlQuery?.trim() || DEFAULT_QUERY;
    setQueryInput(nextQuery);
    setActiveQuery(nextQuery);
  }, [urlQuery]);

  useEffect(() => {
    if (queryInput === activeQuery) return;
    const timer = window.setTimeout(() => setActiveQuery(queryInput.trim()), 180);
    return () => window.clearTimeout(timer);
  }, [activeQuery, queryInput]);

  useEffect(() => {
    const extensions = Array.from(new Set(extensionInput.split(/[;,\s]+/)
      .map((extension) => extension.trim().replace(/^\.+/, '').toLowerCase()).filter(Boolean)));
    if (extensions.join(';') === options.extensions.join(';')) return;
    const timer = window.setTimeout(() => setOptions((current) => ({ ...current, extensions })), 180);
    return () => window.clearTimeout(timer);
  }, [extensionInput, options.extensions]);

  const setOption = useCallback(<K extends keyof SearchRequestOptions>(key: K, value: SearchRequestOptions[K]) => {
    setOptions((current) => ({ ...current, [key]: value }));
  }, []);
  const setExtensionInput = useCallback((value: string) => {
    setExtensionInputState(value);
  }, []);
  // Pending single-click preview, cancelled by a double click (or by unmount).
  const pendingPreviewTimer = useRef<number | undefined>(undefined);
  const cancelPendingPreview = useCallback(() => {
    if (pendingPreviewTimer.current !== undefined) {
      window.clearTimeout(pendingPreviewTimer.current);
      pendingPreviewTimer.current = undefined;
    }
  }, []);
  useEffect(() => cancelPendingPreview, [cancelPendingPreview]);
  const onHitRowClick = useCallback((hit: SearchFileHit) => {
    setSelectedSearchHitId(hit.fileId);
    cancelPendingPreview();
    pendingPreviewTimer.current = window.setTimeout(() => {
      pendingPreviewTimer.current = undefined;
      openHitPreview(hit);
    }, SINGLE_CLICK_PREVIEW_DELAY_MS);
  }, [cancelPendingPreview, openHitPreview, setSelectedSearchHitId]);
  const previewSelected = useCallback(() => {
    cancelPendingPreview();
    if (selectedHit) openHitPreview(selectedHit);
  }, [cancelPendingPreview, selectedHit, openHitPreview]);
  const jumpToFile = useCallback((hit: SearchFileHit) => {
    cancelPendingPreview();
    setSelectedSearchHitId(hit.fileId);
    preview.onOpenChange(false);
    fileNavigation.jump(hit.fileId);
  }, [cancelPendingPreview, fileNavigation, preview, setSelectedSearchHitId]);
  const loadNextPage = useCallback(() => {
    if (searchQuery.hasNextPage && !searchQuery.isFetching) void searchQuery.fetchNextPage();
  }, [searchQuery]);
  const retry = useCallback(() => { void searchQuery.refetch(); }, [searchQuery]);
  const clearQuery = useCallback(() => setQueryInput(''), []);
  const toggleSort = useCallback((key: string) => {
    const sortKey = key as SearchSortKey;
    setOptions((current) => ({
      ...current,
      sortKey,
      sortDirection: current.sortKey === sortKey && current.sortDirection === 'asc' ? 'desc' : 'asc',
    }));
  }, []);

  return {
    activeQuery,
    contentIndex: { ...contentIndex, build: () => {
      setIndexTarget({ caseId, sourceId: indexSourceId });
      contentIndex.build();
    } },
    indexSources,
    indexSourceId,
    setIndexSourceId: (sourceId: string) => { if (!contentIndex.loading) setIndexTarget({ caseId, sourceId }); },
    clearQuery,
    coverage: firstPage?.coverage,
    dataSources: dataSources ?? [],
    extensionInput,
    hasMore: searchQuery.hasNextPage,
    initialLoadFailed: searchQuery.isError && searchHits.length === 0,
    loadContextKey: `${caseId}:${activeQuery}:${JSON.stringify(options)}`,
    loadMoreFailed: searchQuery.isFetchNextPageError,
    loadNextPage,
    loadingMore: searchQuery.isFetchingNextPage,
    onHitRowClick,
    jumpToFile,
    previewSelected,
    navigating: fileNavigation.pending,
    navigationError: fileNavigation.errorText,
    available: Boolean(caseId),
    updating: searchQuery.isFetching || queryInput.trim() !== activeQuery,
    options,
    preview,
    queryInput,
    retry,
    refresh: () => { void cache.resetQueries({ queryKey: ['search', caseId] }); contentIndex.refresh(); },
    searchHits,
    searchQueryStateKey: searchQuery.dataUpdatedAt,
    searchTookMs: searchQuery.data?.pages.reduce((total, page) => total + page.tookMs, 0) ?? 0,
    selectedHit,
    setOption,
    setExtensionInput,
    setQueryInput,
    sortDirection: options.sortDirection as SearchSortDirection,
    sortKey: options.sortKey,
    toggleSort,
    totalHits: firstPage?.total ?? 0,
    truncated: (firstPage?.truncated ?? false) || localLimitReached,
  };
}

export type SearchWorkspaceModel = ReturnType<typeof useSearchWorkspaceModel>;
