import { apiClient } from './client';

export async function getArtifactFamilies() {
  return apiClient.request('get_artifact_families', () => apiClient.getMockProvider().getArtifactFamilies());
}

export async function getArtifactRows(family?: string) {
  return apiClient.request('get_artifact_rows', () => apiClient.getMockProvider().getArtifactRows(family), { family });
}

export async function getArtifactById(artifactId: string) {
  return apiClient.request(
    'get_artifact_by_id',
    () => apiClient.getMockProvider().getArtifactById(artifactId),
    { request: { artifactId } },
  );
}

export async function getArtifactFamilyCounts() {
  return apiClient.request<{ family: string; count: number }[]>(
    'get_artifact_family_counts',
    () => Promise.resolve([]),
  );
}
