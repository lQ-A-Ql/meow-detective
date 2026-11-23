import { COMMANDS } from './commands';
import { apiClient } from './client';
import type { CapabilityKind, CapabilityMatrix } from '@/types/models';

export async function getSourceCapabilities(): Promise<CapabilityMatrix> {
  return apiClient.request(COMMANDS.capability.GET_SOURCE_CAPABILITIES, undefined);
}

export async function retrySourceCapability(
  dataSourceId: string,
  capability: Extract<CapabilityKind, 'timeline' | 'artifacts'>,
): Promise<CapabilityMatrix> {
  return apiClient.request(COMMANDS.capability.RETRY_SOURCE_CAPABILITY, { dataSourceId, capability });
}
