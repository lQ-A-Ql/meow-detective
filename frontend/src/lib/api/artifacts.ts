import { ArtifactRow } from '@/types/models';
import { apiClient } from './client';

export async function getArtifactFamilies(): Promise<string[]> {
  return apiClient.request('get_artifact_families');
}

export async function getArtifactRows(family?: string): Promise<ArtifactRow[]> {
  return apiClient.request('get_artifact_rows', { family });
}

export async function getArtifactById(artifactId: string): Promise<ArtifactRow | null> {
  return apiClient.request('get_artifact_by_id', { request: { artifactId } });
}

export async function getArtifactFamilyCounts(): Promise<{ family: string; count: number }[]> {
  return apiClient.request('get_artifact_family_counts');
}
