import { describe, expect, it } from 'vitest';
import { derivedBootPolicy } from '@/features/emulation/derived-boot-policy';
import type { EmulationPreflight } from '@/types/models';

const direct: EmulationPreflight = {
  dataSourceId: 'source-1', installs: [], recommendedBootRoute: 'recoveryMedia',
  directVolume: { bytesPerSector: 512, logicalMediaBytes: 2_000_396_321_280,
    declaredVolumeBytes: 275_402_194_432, verifiedExtentBytes: 275_402_194_944, volumeSerial: '4482D035A65A1162' },
};

describe('derived boot policy', () => {
  it('requires recovery media for a direct volume and admits a verified selection', () => {
    expect(derivedBootPolicy(direct, false).allowed).toBe(false);
    expect(derivedBootPolicy(direct, true).allowed).toBe(true);
    expect(derivedBootPolicy({ ...direct, directVolume: undefined }, false).allowed).toBe(true);
  });
  it('rejects unverified backup and unsupported 4K/MBR capacity', () => {
    const volume = direct.directVolume!;
    for (const change of [{ verifiedExtentBytes: undefined }, { bytesPerSector: 4096 },
      { verifiedExtentBytes: 0xffff_ffff * 512 }]) {
      expect(derivedBootPolicy({ ...direct, directVolume: { ...volume, ...change } }, true).allowed).toBe(false);
    }
  });
});
