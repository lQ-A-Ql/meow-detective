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
  return apiClient.request('search_files_request', () =>
    apiClient.getMockProvider().searchFiles(query),
    { request: { query, offset, limit } },
  );
}
