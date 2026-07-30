import { useCallback, useEffect, useMemo, useState } from 'react';
import { useSearchParams } from 'react-router';
import { useDataSources } from '@/features/case/hooks';
import { useInfiniteSearchResults } from '@/features/search/hooks';
import { useSearchSelection } from '@/features/search/use-search-page-model';
import { useSearchPreviewModel } from '@/features/search/use-search-preview-model';
import type {
  SearchFileHit,
  SearchRequestOptions,
  SearchSortDirection,
  SearchSortKey,
} from '@/types/models';

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
  const searchQuery = useInfiniteSearchResults(activeQuery, 100, options);
  const searchHits = useMemo(
    () => searchQuery.data?.pages.flatMap((page) => page.items) ?? [],
    [searchQuery.data],
  );
  const firstPage = searchQuery.data?.pages[0];
  const { selectedSearchHitId, setSelectedSearchHitId } = useSearchSelection();
  const preview = useSearchPreviewModel();
  const { openHit: openHitPreview } = preview;
  const selectedHit = searchHits.find((hit) => hit.fileId === selectedSearchHitId);

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

  const setOption = useCallback(<K extends keyof SearchRequestOptions>(key: K, value: SearchRequestOptions[K]) => {
    setOptions((current) => ({ ...current, [key]: value }));
  }, []);
  const setExtensionInput = useCallback((value: string) => {
    setExtensionInputState(value);
    const extensions = value
      .split(/[;,\s]+/)
      .map((extension) => extension.trim().replace(/^\.+/, ''))
      .filter(Boolean);
    setOptions((current) => ({ ...current, extensions }));
  }, []);
  const onHitRowClick = useCallback((hit: SearchFileHit) => {
    setSelectedSearchHitId(hit.fileId);
    openHitPreview(hit);
  }, [openHitPreview, setSelectedSearchHitId]);
  const loadNextPage = useCallback(() => { void searchQuery.fetchNextPage(); }, [searchQuery]);
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
    clearQuery,
    coverage: firstPage?.coverage,
    dataSources: dataSources ?? [],
    extensionInput,
    hasMore: searchQuery.hasNextPage,
    initialLoadFailed: searchQuery.isError && searchHits.length === 0,
    loadContextKey: `${activeQuery}:${JSON.stringify(options)}`,
    loadMoreFailed: searchQuery.isFetchNextPageError,
    loadNextPage,
    loadingMore: searchQuery.isFetchingNextPage,
    onHitRowClick,
    options,
    preview,
    queryInput,
    retry,
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
    truncated: firstPage?.truncated ?? false,
  };
}

export type SearchWorkspaceModel = ReturnType<typeof useSearchWorkspaceModel>;
