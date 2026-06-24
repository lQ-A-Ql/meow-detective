import { ArtifactRow } from '@/types/models';
import { COMMANDS } from './commands';
import { apiClient } from './client';

export async function getArtifactFamilies(): Promise<string[]> {
  return apiClient.request(COMMANDS.artifacts.GET_ARTIFACT_FAMILIES);
}

export async function getArtifactRows(family?: string): Promise<ArtifactRow[]> {
  return apiClient.request(COMMANDS.artifacts.GET_ARTIFACT_ROWS, { family });
}

export async function getArtifactById(artifactId: string): Promise<ArtifactRow | null> {
  return apiClient.request(COMMANDS.artifacts.GET_ARTIFACT_BY_ID, { request: { artifactId } });
}

export async function getArtifactFamilyCounts(): Promise<{ family: string; count: number }[]> {
  return apiClient.request(COMMANDS.artifacts.GET_ARTIFACT_FAMILY_COUNTS);
}
