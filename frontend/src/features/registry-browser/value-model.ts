import type { RegistryValueMetadata } from '@/types/registryBrowser';
import type { RegistryValueRow } from './types';
type Translate = (key: string, options?: Record<string, unknown>) => string;

export function registryValueRows(values: RegistryValueMetadata[], t: Translate): RegistryValueRow[] {
  return values.map((value) => ({ ...value,
    displayName: value.name === '' ? t('registryBrowser.defaultValue') : value.name,
    displayPreview: value.typeCode === 3 || value.readState === 'unsupported'
      ? t('registryBrowser.binaryBytes', { count: value.byteLength })
      : value.preview || t('registryBrowser.emptyString'),
    displayLength: value.byteLength.toLocaleString(),
  }));
}
export function registryFiletimeUtc(filetime?: string): string | undefined {
  if (!filetime || !/^\d+$/.test(filetime)) return undefined;
  const milliseconds = (BigInt(filetime) - 116444736000000000n) / 10000n;
  if (milliseconds < -8640000000000000n || milliseconds > 8640000000000000n) return undefined;
  return new Date(Number(milliseconds)).toISOString();
}

