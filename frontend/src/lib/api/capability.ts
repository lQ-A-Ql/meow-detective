import { COMMANDS } from './commands';
import { apiClient } from './client';
import type { CapabilityMatrix } from '@/types/models';

export async function getSourceCapabilities(): Promise<CapabilityMatrix> {
  return apiClient.request(COMMANDS.capability.GET_SOURCE_CAPABILITIES, undefined);
}
