import { useCallback, useEffect, useState } from 'react';
import {
  forgetPersistedBitLockerKey,
  importUnlockedBitLockerCatalog,
  inspectBitLockerVolume,
  lockBitLockerVolume,
  restorePersistedBitLockerKey,
  unlockBitLockerWithPassword,
  unlockBitLockerWithRecoveryPassword,
} from '@/lib/api/files';
import { isApiErrorDto } from '@/lib/errors';
import type { BitLockerCatalogImport, BitLockerVolumeStatus } from '@/types/models';
import type { BitLockerTarget } from '@/features/files/bitlocker';

export type BitLockerUnlockMethod = 'password' | 'recoveryPassword';

export interface BitLockerVolumeModel {
  status?: BitLockerVolumeStatus;
  catalog?: BitLockerCatalogImport;
  loading: boolean;
  unlocking: boolean;
  importing: boolean;
  error?: string;
  inspect: () => Promise<void>;
  unlock: (method: BitLockerUnlockMethod, credential: string) => Promise<boolean>;
  restore: () => Promise<boolean>;
  importCatalog: () => Promise<boolean>;
  lock: () => Promise<boolean>;
  forget: () => Promise<boolean>;
}

function safeErrorMessage(error: unknown): string {
  if (isApiErrorDto(error)) {
    return error.message;
  }
  return 'BitLocker 操作失败，请查看错误抽屉中的详细信息。';
}

export function useBitLockerVolumeModel(target?: BitLockerTarget): BitLockerVolumeModel {
  const [status, setStatus] = useState<BitLockerVolumeStatus>();
  const [catalog, setCatalog] = useState<BitLockerCatalogImport>();
  const [loading, setLoading] = useState(false);
  const [unlocking, setUnlocking] = useState(false);
  const [importing, setImporting] = useState(false);
  const [error, setError] = useState<string>();

  const inspect = useCallback(async () => {
    if (!target) {
      return;
    }
    setLoading(true);
    setError(undefined);
    try {
      setStatus(await inspectBitLockerVolume(target.dataSourceId, target.partitionIndex));
    } catch (reason) {
      setError(safeErrorMessage(reason));
    } finally {
      setLoading(false);
    }
  }, [target]);

  useEffect(() => {
    setStatus(undefined);
    setCatalog(undefined);
    setError(undefined);
    if (target) {
      void inspect();
    }
  }, [inspect, target]);

  const unlock = useCallback(async (method: BitLockerUnlockMethod, credential: string) => {
    if (!target || !credential) {
      return false;
    }
    setUnlocking(true);
    setError(undefined);
    try {
      const next = method === 'password'
        ? await unlockBitLockerWithPassword(target.dataSourceId, target.partitionIndex, credential)
        : await unlockBitLockerWithRecoveryPassword(
          target.dataSourceId,
          target.partitionIndex,
          credential,
        );
      setStatus(next);
      return true;
    } catch (reason) {
      setError(safeErrorMessage(reason));
      return false;
    } finally {
      setUnlocking(false);
    }
  }, [target]);

  const restore = useCallback(async () => {
    if (!target) {
      return false;
    }
    setLoading(true);
    setError(undefined);
    try {
      setStatus(await restorePersistedBitLockerKey(target.dataSourceId, target.partitionIndex));
      return true;
    } catch (reason) {
      setError(safeErrorMessage(reason));
      return false;
    } finally {
      setLoading(false);
    }
  }, [target]);

  const importCatalog = useCallback(async () => {
    if (!target) {
      return false;
    }
    setImporting(true);
    setError(undefined);
    try {
      const result = await importUnlockedBitLockerCatalog(
        target.dataSourceId,
        target.partitionIndex,
      );
      setCatalog(result);
      setStatus(result.volume);
      return true;
    } catch (reason) {
      setError(safeErrorMessage(reason));
      return false;
    } finally {
      setImporting(false);
    }
  }, [target]);

  const lock = useCallback(async () => {
    if (!target) {
      return false;
    }
    setLoading(true);
    setError(undefined);
    try {
      setStatus(await lockBitLockerVolume(target.dataSourceId, target.partitionIndex));
      setCatalog(undefined);
      return true;
    } catch (reason) {
      setError(safeErrorMessage(reason));
      return false;
    } finally {
      setLoading(false);
    }
  }, [target]);

  const forget = useCallback(async () => {
    if (!target) {
      return false;
    }
    setLoading(true);
    setError(undefined);
    try {
      setStatus(await forgetPersistedBitLockerKey(target.dataSourceId, target.partitionIndex));
      return true;
    } catch (reason) {
      setError(safeErrorMessage(reason));
      return false;
    } finally {
      setLoading(false);
    }
  }, [target]);

  return {
    status,
    catalog,
    loading,
    unlocking,
    importing,
    error,
    inspect,
    unlock,
    restore,
    importCatalog,
    lock,
    forget,
  };
}
