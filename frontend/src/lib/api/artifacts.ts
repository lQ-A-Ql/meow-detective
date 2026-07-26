import { ArtifactRow } from '@/types/models';
import { COMMANDS } from './commands';
import { apiClient } from './client';

export interface ArtifactPage {
  total: number;
  items: ArtifactRow[];
  nextCursor?: string;
}

export async function getArtifactFamilies(): Promise<string[]> {
  return apiClient.request(COMMANDS.artifacts.GET_ARTIFACT_FAMILIES);
}

export async function getArtifactRows(family?: string): Promise<ArtifactRow[]> {
  return apiClient.request(COMMANDS.artifacts.GET_ARTIFACT_ROWS, { family });
}

export async function getArtifactRowsPage(
  family: string | undefined,
  cursor: string | undefined,
  limit: number,
): Promise<ArtifactPage> {
  return apiClient.request(COMMANDS.artifacts.GET_ARTIFACT_ROWS_REQUEST, {
    request: { family, limit, cursor },
  });
}

export async function getArtifactById(artifactId: string): Promise<ArtifactRow | null> {
  return apiClient.request(COMMANDS.artifacts.GET_ARTIFACT_BY_ID, { request: { artifactId } });
}

export async function getArtifactFamilyCounts(): Promise<{ family: string; count: number }[]> {
  return apiClient.request(COMMANDS.artifacts.GET_ARTIFACT_FAMILY_COUNTS);
}
