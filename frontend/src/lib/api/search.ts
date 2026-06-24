import { COMMANDS } from './commands';
import { apiClient } from './client';
import { SearchResultPage } from '@/types/models';

export interface SearchRequest {
  query: string;
  offset?: number;
  limit?: number;
}

export async function searchFiles(
  query: string,
  offset: number = 0,
  limit: number = 50,
): Promise<SearchResultPage> {
  return apiClient.request(COMMANDS.search.SEARCH_FILES_REQUEST, { request: { query, offset, limit } });
}
