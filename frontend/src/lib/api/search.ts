import { COMMANDS } from './commands';
import { apiClient } from './client';
import { SearchResultPage } from '@/types/models';

export interface SearchRequest {
  query: string;
  offset?: number;
  limit?: number;
  cursor?: string;
}

export async function searchFiles(
  query: string,
  offset: number = 0,
  limit: number = 50,
  cursor?: string,
): Promise<SearchResultPage> {
  const request: SearchRequest = { query, offset, limit };
  if (cursor) request.cursor = cursor;
  return apiClient.request(COMMANDS.search.SEARCH_FILES_REQUEST, { request });
}
