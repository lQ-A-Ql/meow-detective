import type { EmulationPreflight } from '@/types/models';

export function derivedBootPolicy(preflight: EmulationPreflight | undefined, hasRecoveryMedia: boolean) {
  const volume = preflight?.directVolume;
  if (!volume) return { allowed: true, hintKey: undefined };
  if (volume.bytesPerSector !== 512) {
    return { allowed: false, hintKey: 'emulationPage.errors.derivedBootSectorSizeUnsupported' };
  }
  if (!volume.verifiedExtentBytes) {
    return { allowed: false, hintKey: 'emulationPage.errors.derivedBootBackupUnverified' };
  }
  if (volume.verifiedExtentBytes > (0xffff_ffff - 2048) * 512) {
    return { allowed: false, hintKey: 'emulationPage.errors.derivedBootMbrCapacity' };
  }
  return hasRecoveryMedia
    ? { allowed: true, hintKey: 'emulationPage.launch.directVolumeDerived' }
    : { allowed: false, hintKey: 'emulationPage.errors.directVolumeRecoveryRequired' };
}
