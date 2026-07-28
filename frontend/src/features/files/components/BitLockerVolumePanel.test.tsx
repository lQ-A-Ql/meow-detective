import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { BitLockerVolumePanel } from './BitLockerVolumePanel';
import type { BitLockerVolumeModel } from '@/features/files/hooks/use-bitlocker-volume';
import type { DataSourcePartition } from '@/types/models';

const partition: DataSourcePartition = {
  index: 2,
  name: 'Partition 2',
  kindLabel: 'BitLocker',
  status: 'EncryptedBitLocker',
  offset: 0,
  length: 4096,
  filesystem: 'BitLocker',
};

function model(overrides: Partial<BitLockerVolumeModel> = {}): BitLockerVolumeModel {
  return {
    status: {
      dataSourceId: 'source-1',
      partitionIndex: 2,
      unlocked: false,
      encryptionMethod: 'AES-128-CBC + Elephant Diffuser',
      encryptionMethodCode: 0x8002,
      decryptable: true,
      bytesPerSector: 512,
      metadataFingerprint: 'fingerprint',
      metadataCopyCount: 2,
      protectors: [{ code: 1, kind: 'password', label: 'Password', unlockable: true }],
      supportsPassword: true,
      supportsRecoveryPassword: false,
      storedKeyAvailable: false,
    },
    loading: false,
    unlocking: false,
    importing: false,
    catalogImport: undefined,
    inspect: vi.fn(),
    unlock: vi.fn().mockResolvedValue(true),
    restore: vi.fn().mockResolvedValue(true),
    importCatalog: vi.fn().mockResolvedValue(true),
    lock: vi.fn().mockResolvedValue(true),
    forget: vi.fn().mockResolvedValue(true),
    ...overrides,
  };
}

describe('BitLockerVolumePanel', () => {
  it('clears the credential input before sending it to the real feature action', () => {
    const unlock = vi.fn().mockResolvedValue(true);
    render(<BitLockerVolumePanel partition={partition} model={model({ unlock })} />);

    const input = screen.getByLabelText('BitLocker 凭据');
    fireEvent.change(input, { target: { value: 'not-persisted' } });
    fireEvent.click(screen.getByRole('button', { name: /解锁并保存/ }));

    expect(input).toHaveValue('');
    expect(unlock).toHaveBeenCalledWith('password', 'not-persisted');
    expect(screen.queryByDisplayValue('not-persisted')).not.toBeInTheDocument();
  });

  it('separates runtime lock from deleting the persisted key', () => {
    const lock = vi.fn().mockResolvedValue(true);
    const forget = vi.fn().mockResolvedValue(true);
    render(
      <BitLockerVolumePanel
        partition={partition}
        model={model({
          lock,
          forget,
          status: {
            ...model().status!,
            unlocked: true,
            storedKeyAvailable: true,
          },
        })}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: /锁定运行时卷/ }));
    fireEvent.click(screen.getByRole('button', { name: /删除安全存储/ }));
    expect(lock).toHaveBeenCalledTimes(1);
    expect(forget).toHaveBeenCalledTimes(1);
  });
});
