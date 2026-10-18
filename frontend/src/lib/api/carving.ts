import { apiClient } from './client';
import { COMMANDS } from './commands';
import type { CarvedArtifactPage, CarvedRange, CarvedExport } from '@/types/carving';
export function searchCarvedArtifacts(dataSourceId: string, query: string, cursor?: string): Promise<CarvedArtifactPage> {
  return apiClient.request(COMMANDS.carving.SEARCH, { request: { dataSourceId, query, cursor } });
}
export function readCarvedArtifactRange(artifactId: string): Promise<CarvedRange> {
  return apiClient.request(COMMANDS.carving.READ, { request: { artifactId, offset: '0', length: 65536 } });
}
export function exportCarvedArtifact(artifactId: string, destinationPath: string): Promise<CarvedExport> {
  return apiClient.request(COMMANDS.carving.EXPORT, { request: { artifactId, destinationPath } });
}
