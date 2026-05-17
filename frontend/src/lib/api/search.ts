import { apiClient } from './client';

export async function searchFiles(query: string) {
  return apiClient.request('search_files', () => apiClient.getMockProvider().searchFiles(query), { query });
}
