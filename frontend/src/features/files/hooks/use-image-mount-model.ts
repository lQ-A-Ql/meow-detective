import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useCurrentCase, useDataSources } from '@/features/case/hooks';
import { confirmEmulationBoot } from '@/features/emulation/boot-consent';
import { EMULATION_SESSIONS_QUERY_KEY } from '@/features/emulation/query-keys';
import {
  launchEmulation,
  listEmulationSessions,
  prepareEmulation,
  releaseEmulation,
} from '@/lib/api/emulation';
import { listMounts, mountImage, mountPhysicalImage, unmountImage } from '@/lib/api/mount';
import { errorMessage } from '@/lib/errors';
import { openDialog as openPlatformDialog, singleDialogPath } from '@/lib/platform/dialog';
import type {
  DataSourcePartition,
  DataSourceSummary,
  EmulationSessionStatus,
  MountMode,
  MountStatus,
} from '@/types/models';

const MOUNT_QUERY_KEY = ['mounts'] as const;
type ImageAccessMode = MountMode | 'emulation';

const MOUNT_POINT_OPTIONS: readonly string[] = [
  'A:', 'B:', 'C:', 'D:', 'E:', 'F:', 'G:', 'H:', 'I:', 'J:', 'K:', 'L:', 'M:',
  'N:', 'O:', 'P:', 'Q:', 'R:', 'S:', 'T:', 'U:', 'V:', 'W:', 'X:', 'Y:', 'Z:',
] as const;

function isActiveMount(status: MountStatus) {
  return status.state === 'preparing'
    || status.state === 'mounted'
    || status.state === 'unmounting';
}

function isActiveEmulation(status: EmulationSessionStatus) {
  return status.state !== 'released' && status.state !== 'failedCleanupPending';
}

function findSelectedPartition(
  source: DataSourceSummary | undefined,
  partitionIndex: string,
): DataSourcePartition | undefined {
  if (!source || !partitionIndex) {
    return undefined;
  }
  return source.partitions?.find((partition) => String(partition.index) === partitionIndex);
}

export function useImageMountModel() {
  const { t } = useTranslation();
  const currentCase = useCurrentCase();
  const dataSourcesQuery = useDataSources();
  const queryClient = useQueryClient();
  const [dialogOpen, setDialogOpen] = useState(false);
  const [selectedSourceId, setSelectedSourceId] = useState('');
  const [mountMode, setMountMode] = useState<ImageAccessMode>('logicalPartition');
  const [selectedPartitionIndex, setSelectedPartitionIndex] = useState('');
  const [mountPoint, setMountPoint] = useState('auto');
  const [recoveryIsoPath, setRecoveryIsoPath] = useState('');

  const mountsQuery = useQuery({
    queryKey: MOUNT_QUERY_KEY,
    queryFn: listMounts,
    enabled: currentCase.isSuccess && Boolean(currentCase.data),
    refetchInterval: (query) => query.state.data?.some(isActiveMount) ? 1500 : false,
    retry: false,
  });
  const emulationQuery = useQuery({
    queryKey: EMULATION_SESSIONS_QUERY_KEY,
    queryFn: listEmulationSessions,
    enabled: currentCase.isSuccess && Boolean(currentCase.data),
    refetchInterval: (query) => query.state.data?.some(isActiveEmulation) ? 1500 : false,
    retry: false,
  });

  const dataSources = dataSourcesQuery.data ?? [];
  const selectedSource = useMemo(
    () => dataSources.find((source) => source.id === selectedSourceId),
    [dataSources, selectedSourceId],
  );
  const partitions = selectedSource?.partitions ?? [];
  const selectedPartition = findSelectedPartition(selectedSource, selectedPartitionIndex);
  const selectedMount = useMemo(
    () => mountsQuery.data?.find((mount) => {
      if (mountMode === 'emulation') {
        return false;
      }
      if (mount.target.dataSourceId !== selectedSourceId || mount.target.mode !== mountMode) {
        return false;
      }
      return mountMode === 'physicalDisk'
        || String(mount.target.partitionIndex) === selectedPartitionIndex;
    }),
    [mountMode, mountsQuery.data, selectedPartitionIndex, selectedSourceId],
  );
  const selectedEmulation = useMemo(
    () => emulationQuery.data?.find((session) => (
      session.dataSourceId === selectedSourceId && isActiveEmulation(session)
    )),
    [emulationQuery.data, selectedSourceId],
  );

  useEffect(() => {
    if (dataSources.length === 0) {
      setSelectedSourceId('');
      return;
    }
    if (!dataSources.some((source) => source.id === selectedSourceId)) {
      setSelectedSourceId(dataSources[0].id);
    }
  }, [dataSources, selectedSourceId]);

  useEffect(() => {
    if (partitions.length === 0) {
      setSelectedPartitionIndex('');
      return;
    }
    if (!partitions.some((partition) => String(partition.index) === selectedPartitionIndex)) {
      setSelectedPartitionIndex(String(partitions[0].index));
    }
  }, [partitions, selectedPartitionIndex]);

  const invalidateMounts = useCallback(() => {
    void queryClient.invalidateQueries({ queryKey: MOUNT_QUERY_KEY });
  }, [queryClient]);
  const invalidateEmulations = useCallback(() => {
    void queryClient.invalidateQueries({ queryKey: EMULATION_SESSIONS_QUERY_KEY });
  }, [queryClient]);

  const mountMutation = useMutation({
    mutationFn: () => {
      if (!selectedSourceId) {
        throw new Error('请选择数据源。');
      }
      if (mountMode === 'physicalDisk') {
        return mountPhysicalImage({ dataSourceId: selectedSourceId });
      }
      if (!selectedPartitionIndex) {
        throw new Error('请选择分区。');
      }
      return mountImage({
        dataSourceId: selectedSourceId,
        partitionIndex: Number(selectedPartitionIndex),
        mountPoint: mountPoint === 'auto' ? undefined : mountPoint,
      });
    },
    onSuccess: invalidateMounts,
  });

  const unmountMutation = useMutation({
    mutationFn: (mountId: string) => unmountImage(mountId),
    onSuccess: invalidateMounts,
  });
  const emulationMutation = useMutation({
    mutationFn: async (allowDirectBoot: boolean) => {
      if (!selectedSourceId) {
        throw new Error(t('fileBrowser.mount.selectSourceError'));
      }
      const prepared = await prepareEmulation({
        dataSourceId: selectedSourceId,
        recoveryIsoPath: recoveryIsoPath || undefined,
        allowDirectBoot,
      });
      return launchEmulation(prepared.sessionId);
    },
    onSuccess: invalidateEmulations,
  });
  const releaseEmulationMutation = useMutation({
    mutationFn: (sessionId: string) => releaseEmulation(sessionId),
    onSuccess: invalidateEmulations,
  });

  const openDialog = useCallback(() => {
    mountMutation.reset();
    unmountMutation.reset();
    emulationMutation.reset();
    releaseEmulationMutation.reset();
    setMountPoint('auto');
    setMountMode('logicalPartition');
    setRecoveryIsoPath('');
    setDialogOpen(true);
    void mountsQuery.refetch();
  }, [emulationMutation, mountMutation, mountsQuery, releaseEmulationMutation, unmountMutation]);

  const setDialogOpenSafely = useCallback((open: boolean) => {
    if (!open && (
      mountMutation.isPending
      || unmountMutation.isPending
      || emulationMutation.isPending
      || releaseEmulationMutation.isPending
    )) {
      return;
    }
    setDialogOpen(open);
  }, [
    emulationMutation.isPending,
    mountMutation.isPending,
    releaseEmulationMutation.isPending,
    unmountMutation.isPending,
  ]);

  const submit = useCallback(async () => {
    if (mountMode === 'emulation') {
      const allowDirectBoot = recoveryIsoPath.length === 0;
      if (!confirmEmulationBoot(recoveryIsoPath, t('fileBrowser.mount.directBootConfirm'))) {
        return;
      }
      await emulationMutation.mutateAsync(allowDirectBoot);
      return;
    }
    await mountMutation.mutateAsync();
  }, [emulationMutation, mountMode, mountMutation, recoveryIsoPath.length, t]);

  const unmount = useCallback(async (mountId: string) => {
    await unmountMutation.mutateAsync(mountId);
  }, [unmountMutation]);
  const pickRecoveryIso = useCallback(async () => {
    const selected = await openPlatformDialog({
      directory: false,
      multiple: false,
      filters: [{ name: 'WinPE ISO', extensions: ['iso'] }],
    });
    const path = singleDialogPath(selected);
    if (path) {
      setRecoveryIsoPath(path);
    }
  }, []);
  const releaseSelectedEmulation = useCallback(async (sessionId: string) => {
    await releaseEmulationMutation.mutateAsync(sessionId);
  }, [releaseEmulationMutation]);

  return {
    dialogOpen,
    openDialog,
    setDialogOpen: setDialogOpenSafely,
    dataSources,
    selectedSourceId,
    setSelectedSourceId,
    selectedSource,
    mountMode,
    setMountMode,
    partitions,
    selectedPartitionIndex,
    setSelectedPartitionIndex,
    selectedPartition,
    mountPoint,
    setMountPoint,
    recoveryIsoPath,
    setRecoveryIsoPath,
    pickRecoveryIso,
    mounts: mountsQuery.data ?? [],
    selectedMount,
    emulationSessions: emulationQuery.data ?? [],
    selectedEmulation,
    isLoadingMounts: mountsQuery.isLoading,
    isSubmitting: mountMutation.isPending
      || unmountMutation.isPending
      || emulationMutation.isPending
      || releaseEmulationMutation.isPending,
    isMounting: mountMutation.isPending,
    isUnmounting: unmountMutation.isPending,
    isEmulating: emulationMutation.isPending,
    isReleasingEmulation: releaseEmulationMutation.isPending,
    error: mountMutation.error || unmountMutation.error || emulationMutation.error || releaseEmulationMutation.error
      ? errorMessage(
        mountMutation.error
          ?? unmountMutation.error
          ?? emulationMutation.error
          ?? releaseEmulationMutation.error,
        t('fileBrowser.mount.operationFailed'),
      )
      : undefined,
    submit,
    unmount,
    releaseEmulation: releaseSelectedEmulation,
    refresh: mountsQuery.refetch,
    mountPointOptions: MOUNT_POINT_OPTIONS,
  };
}

export type ImageMountModel = ReturnType<typeof useImageMountModel>;
