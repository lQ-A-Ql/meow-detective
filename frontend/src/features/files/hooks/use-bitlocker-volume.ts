import { useQueryClient } from '@tanstack/react-query';
import { useCallback, useEffect, useState } from 'react';
import {
  forgetPersistedBitLockerKey,
  cancelBitLockerDictionaryAttack,
  getBitLockerDictionaryAttackStatus,
  importUnlockedBitLockerCatalog,
  inspectBitLockerVolume,
  lockBitLockerVolume,
  restorePersistedBitLockerKey,
  startBitLockerDictionaryAttack,
  unlockBitLockerWithPassword,
  unlockBitLockerWithRecoveryPassword,
  unlockBitLockerWithMemoryImage,
} from '@/lib/api/files';
import { isApiErrorDto } from '@/lib/errors';
import { openDialog, singleDialogPath } from '@/lib/platform/dialog';
import type {
  BitLockerCatalogImport,
  BitLockerDictionaryAttack,
  BitLockerDictionaryBackend,
  BitLockerVolumeStatus,
} from '@/types/models';
import type { BitLockerTarget } from '@/features/files/bitlocker';

export type BitLockerUnlockMethod = 'password' | 'recoveryPassword';
export type BitLockerCatalogImportPhase = 'catalog' | 'refreshing';

export interface BitLockerCatalogImportLifecycle {
  phase: BitLockerCatalogImportPhase;
  startedAt: number;
}

export interface BitLockerVolumeModel {
  status?: BitLockerVolumeStatus;
  catalog?: BitLockerCatalogImport;
  loading: boolean;
  unlocking: boolean;
  memoryUnlocking: boolean;
  importing: boolean;
  dictionaryAttack?: BitLockerDictionaryAttack;
  dictionaryAttacking: boolean;
  dictionaryCancelling: boolean;
  catalogImport?: BitLockerCatalogImportLifecycle;
  error?: string;
  inspect: () => Promise<void>;
  unlock: (method: BitLockerUnlockMethod, credential: string) => Promise<boolean>;
  unlockFromMemoryImage: () => Promise<boolean>;
  restore: () => Promise<boolean>;
  importCatalog: () => Promise<boolean>;
  lock: () => Promise<boolean>;
  forget: () => Promise<boolean>;
  startDictionaryAttack: (backend?: BitLockerDictionaryBackend) => Promise<boolean>;
  cancelDictionaryAttack: () => Promise<boolean>;
}

function safeErrorMessage(error: unknown): string {
  if (isApiErrorDto(error)) {
    return error.message;
  }
  return 'BitLocker 操作失败，请查看错误抽屉中的详细信息。';
}

const CATALOG_QUERY_KEYS = [
  ['files', 'tree'],
  ['files', 'rows'],
  ['files', 'rows-page'],
  ['files', 'children'],
  ['files', 'children-page'],
] as const;

async function refreshFileCatalogQueries(queryClient: ReturnType<typeof useQueryClient>) {
  await Promise.all(
    CATALOG_QUERY_KEYS.map((queryKey) =>
      queryClient.invalidateQueries({ queryKey }),
    ),
  );
}

export function useBitLockerVolumeModel(target?: BitLockerTarget): BitLockerVolumeModel {
  const queryClient = useQueryClient();
  const [status, setStatus] = useState<BitLockerVolumeStatus>();
  const [catalog, setCatalog] = useState<BitLockerCatalogImport>();
  const [loading, setLoading] = useState(false);
  const [unlocking, setUnlocking] = useState(false);
  const [memoryUnlocking, setMemoryUnlocking] = useState(false);
  const [importing, setImporting] = useState(false);
  const [dictionaryAttack, setDictionaryAttack] = useState<BitLockerDictionaryAttack>();
  const [dictionaryCancelling, setDictionaryCancelling] = useState(false);
  const [catalogImport, setCatalogImport] = useState<BitLockerCatalogImportLifecycle>();
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
    setCatalogImport(undefined);
    setImporting(false);
    setDictionaryAttack(undefined);
    setDictionaryCancelling(false);
    setError(undefined);
    if (target) {
      void inspect();
    }
  }, [inspect, target]);

  useEffect(() => {
    if (!target || !dictionaryAttack || !['queued', 'running', 'cancelling'].includes(dictionaryAttack.phase)) {
      return;
    }
    let active = true;
    const poll = async () => {
      try {
        const next = await getBitLockerDictionaryAttackStatus(
          target.dataSourceId,
          target.partitionIndex,
        );
        if (!active || !next) {
          return;
        }
        setDictionaryAttack(next);
        if (next.phase === 'found') {
          await inspect();
        }
      } catch (reason) {
        if (active) {
          setError(safeErrorMessage(reason));
        }
      }
    };
    void poll();
    const timer = window.setInterval(() => void poll(), 750);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [dictionaryAttack?.phase, inspect, target]);

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

  const unlockFromMemoryImage = useCallback(async () => {
    if (!target) {
      return false;
    }
    setError(undefined);
    try {
      const memoryImagePath = singleDialogPath(await openDialog({
        multiple: false,
        directory: false,
        filters: [{ name: 'Windows memory image', extensions: ['mem', 'raw', 'dmp', 'bin'] }],
      }));
      if (!memoryImagePath) {
        return false;
      }
      setMemoryUnlocking(true);
      setStatus(await unlockBitLockerWithMemoryImage(
        target.dataSourceId,
        target.partitionIndex,
        memoryImagePath,
      ));
      return true;
    } catch (reason) {
      setError(safeErrorMessage(reason));
      return false;
    } finally {
      setMemoryUnlocking(false);
    }
  }, [target]);

  const importCatalog = useCallback(async () => {
    if (!target) {
      return false;
    }
    const startedAt = Date.now();
    setImporting(true);
    setCatalogImport({ phase: 'catalog', startedAt });
    setError(undefined);
    try {
      const result = await importUnlockedBitLockerCatalog(
        target.dataSourceId,
        target.partitionIndex,
      );
      setCatalogImport({ phase: 'refreshing', startedAt });
      await refreshFileCatalogQueries(queryClient);
      setCatalog(result);
      setStatus(result.volume);
      return true;
    } catch (reason) {
      setError(safeErrorMessage(reason));
      return false;
    } finally {
      setCatalogImport(undefined);
      setImporting(false);
    }
  }, [queryClient, target]);

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

  const startDictionaryAttack = useCallback(async (backend: BitLockerDictionaryBackend = 'cpu') => {
    if (!target || dictionaryAttack?.phase === 'queued' || dictionaryAttack?.phase === 'running' || dictionaryAttack?.phase === 'cancelling') {
      return false;
    }
    setError(undefined);
    try {
      const selectedPath = singleDialogPath(await openDialog({
        multiple: false,
        directory: false,
        filters: [{
          name: 'BitLocker password dictionary',
          extensions: ['txt', 'dic', 'lst'],
        }],
      }));
      if (!selectedPath) {
        return false;
      }
      const next = await startBitLockerDictionaryAttack(
        target.dataSourceId,
        target.partitionIndex,
        selectedPath,
        backend,
      );
      setDictionaryAttack(next);
      if (next.phase === 'found') {
        await inspect();
      }
      return true;
    } catch (reason) {
      setError(safeErrorMessage(reason));
      return false;
    }
  }, [dictionaryAttack?.phase, inspect, target]);

  const cancelDictionaryAttack = useCallback(async () => {
    if (!target || !dictionaryAttack || !['queued', 'running'].includes(dictionaryAttack.phase) || dictionaryCancelling) {
      return false;
    }
    setDictionaryCancelling(true);
    setError(undefined);
    try {
      const cancelled = await cancelBitLockerDictionaryAttack(
        target.dataSourceId,
        target.partitionIndex,
      );
      return cancelled;
    } catch (reason) {
      setError(safeErrorMessage(reason));
      return false;
    } finally {
      setDictionaryCancelling(false);
    }
  }, [dictionaryAttack?.phase, dictionaryCancelling, target]);

  return {
    status,
    catalog,
    loading,
    unlocking,
    memoryUnlocking,
    importing,
    dictionaryAttack,
    dictionaryAttacking: dictionaryAttack?.phase === 'queued' || dictionaryAttack?.phase === 'running' || dictionaryAttack?.phase === 'cancelling',
    dictionaryCancelling,
    catalogImport,
    error,
    inspect,
    unlock,
    unlockFromMemoryImage,
    restore,
    importCatalog,
    lock,
    forget,
    startDictionaryAttack,
    cancelDictionaryAttack,
  };
}
