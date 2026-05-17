import { apiClient } from './client';

export async function getArtifactFamilies() {
  return apiClient.request('get_artifact_families', () => apiClient.getMockProvider().getArtifactFamilies());
}

export async function getArtifactRows(family?: string) {
  return apiClient.request('get_artifact_rows', () => apiClient.getMockProvider().getArtifactRows(family), { family });
}
