import type { InfrastructureGraph } from '@/types/models';
import { COMMANDS } from './commands';
import { apiClient } from './client';

export async function getInfrastructureGraph(): Promise<InfrastructureGraph> {
  return apiClient.request(COMMANDS.infrastructure.GET_GRAPH);
}
