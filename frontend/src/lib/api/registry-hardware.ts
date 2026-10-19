import type { RegistryHardwareRequest, RegistryHardwareSummary } from '@/types/registryHardware';
import { registryBrowserClient, type RegistryBrowserClient } from './registry-browser';
import { apiClient } from './client';
import { COMMANDS } from './commands';

export type RegistryHardwareClient = RegistryBrowserClient & {
  readHardware: (request: RegistryHardwareRequest) => Promise<RegistryHardwareSummary>;
};

export const registryHardwareClient: RegistryHardwareClient = {
  ...registryBrowserClient,
  readHardware: (request) => apiClient.request(COMMANDS.registryBrowser.HARDWARE, { request }),
};
