import type { RegistryBrowserSource } from './registryBrowser';

export interface RegistryHardwareRequest { sessionId: string; }

export interface RegistryHardwareDevice {
  kind: string;
  deviceId: string;
  name: string;
  manufacturer?: string;
  properties: Record<string, string>;
  keyPath: string;
  valueName?: string;
  sourceOffset?: number;
}

export interface RegistryHardwareSummary {
  source: RegistryBrowserSource;
  devices: RegistryHardwareDevice[];
  warnings: string[];
}
