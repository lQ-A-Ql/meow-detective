import { useTranslation } from 'react-i18next';
import type { RegistryHardwareDevice } from '@/types/analysisRegistry';
import { hardwareGroups, hardwareMemorySummary } from './hardware-model';

export function useRegistryHardwareModel(devices: RegistryHardwareDevice[], warnings: string[]) {
  const { t } = useTranslation();
  return {
    t,
    groups: hardwareGroups(devices, t),
    memory: hardwareMemorySummary(devices, t),
    warnings,
  };
}

export type RegistryHardwareModel = ReturnType<typeof useRegistryHardwareModel>;
