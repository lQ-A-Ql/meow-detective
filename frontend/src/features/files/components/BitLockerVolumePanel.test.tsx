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
    memoryUnlocking: false,
    importing: false,
    catalogImport: undefined,
    inspect: vi.fn(),
    unlock: vi.fn().mockResolvedValue(true),
    unlockFromMemoryImage: vi.fn().mockResolvedValue(true),
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

  it('delegates memory image recovery to the feature model', () => {
    const unlockFromMemoryImage = vi.fn().mockResolvedValue(true);
    render(
      <BitLockerVolumePanel
        partition={partition}
        model={model({ unlockFromMemoryImage })}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: /从内存镜像恢复并验证/ }));
    expect(unlockFromMemoryImage).toHaveBeenCalledTimes(1);
  });

  it('shows the reconstructed recovery password with a copy action', () => {
    render(
      <BitLockerVolumePanel
        partition={partition}
        model={model({
          status: {
            ...model().status!,
            unlocked: true,
            recoveryPasswordReconstruction: {
              status: 'recovered',
              password: '111111-222222-333333-444444-555555-666666-777777-888888',
              volumeGuid: '{VOLUME}',
              protectorGuid: '{PROTECTOR}',
              reverseDatumFingerprint: 'abcdef0123456789',
            },
          },
        })}
      />,
    );

    expect(screen.getByText(/已从内存 VMK 重构恢复密码/)).toBeInTheDocument();
    expect(
      screen.getByText('111111-222222-333333-444444-555555-666666-777777-888888'),
    ).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '复制恢复密码' })).toBeInTheDocument();
  });

  it('shows the explicit reason when reconstruction is unavailable', () => {
    render(
      <BitLockerVolumePanel
        partition={partition}
        model={model({
          status: {
            ...model().status!,
            unlocked: true,
            recoveryPasswordReconstruction: {
              status: 'unavailable',
              reason: "the active VMK does not authenticate any recovery protector's reverse datum",
            },
          },
        })}
      />,
    );

    expect(screen.getByText(/内存 VMK 无法重构恢复密码/)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '复制恢复密码' })).not.toBeInTheDocument();
  });
});
