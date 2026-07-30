import { COMMANDS } from './commands';
import { apiClient } from './client';
import type { SearchRequestOptions, SearchResultPage } from '@/types/models';

export interface SearchRequest {
  query: string;
  offset?: number;
  limit?: number;
  cursor?: string;
  matchPath?: boolean;
  entryType?: SearchRequestOptions['entryType'];
  extensions?: string[];
  dataSourceIds?: string[];
  sortKey?: SearchRequestOptions['sortKey'];
  sortDirection?: SearchRequestOptions['sortDirection'];
}

export async function searchFiles(
  query: string,
  offset: number = 0,
  limit: number = 50,
  cursor?: string,
  options?: Partial<SearchRequestOptions>,
): Promise<SearchResultPage> {
  const request: SearchRequest = { query, offset, limit };
  if (cursor) request.cursor = cursor;
  if (options?.matchPath) request.matchPath = true;
  if (options?.entryType && options.entryType !== 'any') request.entryType = options.entryType;
  if (options?.extensions?.length) request.extensions = options.extensions;
  if (options?.dataSourceIds?.length) request.dataSourceIds = options.dataSourceIds;
  if (options?.sortKey && options.sortKey !== 'name') request.sortKey = options.sortKey;
  if (options?.sortDirection && options.sortDirection !== 'asc') request.sortDirection = options.sortDirection;
  return apiClient.request(COMMANDS.search.SEARCH_FILES_REQUEST, { request });
}
