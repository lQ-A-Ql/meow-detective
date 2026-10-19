import type { TFunction } from 'i18next';
import { HARDWARE_BYTE_UNITS, REGISTRY_HARDWARE_GROUPS, REGISTRY_HARDWARE_PROPERTIES } from '@/design/registry-hardware';
import type { RegistryHardwareDevice } from '@/types/registryHardware';

function exactBytes(value?: string) {
  return value && /^\d{1,20}$/.test(value) ? BigInt(value) : undefined;
}

export function formatHardwareBytes(value?: string) {
  const bytes = exactBytes(value);
  if (bytes === undefined) return undefined;
  let unit = 0;
  let scale = 1n;
  while (unit < HARDWARE_BYTE_UNITS.length - 1 && bytes >= scale * 1024n) { scale *= 1024n; unit += 1; }
  const tenths = (bytes * 10n + scale / 2n) / scale;
  const decimal = tenths % 10n;
  return `${tenths / 10n}${decimal ? `.${decimal}` : ''} ${HARDWARE_BYTE_UNITS[unit]}`;
}

export function hardwareGroups(devices: RegistryHardwareDevice[], t: TFunction) {
  return REGISTRY_HARDWARE_GROUPS.map(({ kind, icon }) => ({
    kind, icon, title: t(`registryHardware.groups.${kind}`),
    rows: devices.filter((record) => record.kind === kind).map((record) => ({
      id: `${record.deviceId}:${record.keyPath}`, name: record.name || t(`registryHardware.unnamed.${kind}`),
      manufacturer: record.manufacturer || t('registryHardware.notExtracted'),
      source: `${record.keyPath}${record.valueName ? `\\${record.valueName}` : ''}`,
      offset: record.sourceOffset !== undefined ? t('registryHardware.offset', { value: `0x${record.sourceOffset.toString(16)}` }) : undefined,
      details: REGISTRY_HARDWARE_PROPERTIES.flatMap((key) => {
        const raw = record.properties[key];
        if (!raw) return [];
        const value = key === 'capacityBytes' ? formatHardwareBytes(raw)
          : key.endsWith('Mhz') ? t('registryHardware.units.mhz', { value: raw })
            : key.endsWith('MtS') ? t('registryHardware.units.mts', { value: raw }) : raw;
        return value ? [{ label: t(`registryHardware.properties.${key}`), value }] : [];
      }),
    })),
  })).filter((group) => group.rows.length > 0 || ['cpu', 'gpu', 'ram'].includes(group.kind));
}

export function hardwareMemorySummary(devices: RegistryHardwareDevice[], t: TFunction) {
  const modules = devices.filter((record) => record.kind === 'ram');
  const known = modules.map((record) => exactBytes(record.properties.capacityBytes));
  const total = known.reduce<bigint>((sum, bytes) => sum + (bytes ?? 0n), 0n);
  const unknown = known.filter((bytes) => bytes === undefined).length;
  return {
    capacity: total ? t('registryHardware.knownMemory', { value: formatHardwareBytes(total.toString()) }) : undefined,
    unknown: unknown ? t('registryHardware.unknownMemory', { count: unknown }) : undefined,
  };
}

export type HardwareGroup = ReturnType<typeof hardwareGroups>[number];
export type HardwareRow = HardwareGroup['rows'][number];
