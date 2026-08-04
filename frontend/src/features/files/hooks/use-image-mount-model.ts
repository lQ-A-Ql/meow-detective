import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useCurrentCase, useDataSources } from '@/features/case/hooks';
import { listMounts, mountImage, mountPhysicalImage, unmountImage } from '@/lib/api/mount';
import { errorMessage } from '@/lib/errors';
import type {
  DataSourcePartition,
  DataSourceSummary,
  MountMode,
  MountStatus,
} from '@/types/models';

const MOUNT_QUERY_KEY = ['mounts'] as const;

const MOUNT_POINT_OPTIONS: readonly string[] = [
  'A:', 'B:', 'C:', 'D:', 'E:', 'F:', 'G:', 'H:', 'I:', 'J:', 'K:', 'L:', 'M:',
  'N:', 'O:', 'P:', 'Q:', 'R:', 'S:', 'T:', 'U:', 'V:', 'W:', 'X:', 'Y:', 'Z:',
] as const;

function isActiveMount(status: MountStatus) {
  return status.state === 'preparing'
    || status.state === 'mounted'
    || status.state === 'unmounting';
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
  const currentCase = useCurrentCase();
  const dataSourcesQuery = useDataSources();
  const queryClient = useQueryClient();
  const [dialogOpen, setDialogOpen] = useState(false);
  const [selectedSourceId, setSelectedSourceId] = useState('');
  const [mountMode, setMountMode] = useState<MountMode>('logicalPartition');
  const [selectedPartitionIndex, setSelectedPartitionIndex] = useState('');
  const [mountPoint, setMountPoint] = useState('auto');

  const mountsQuery = useQuery({
    queryKey: MOUNT_QUERY_KEY,
    queryFn: listMounts,
    enabled: currentCase.isSuccess && Boolean(currentCase.data),
    refetchInterval: (query) => query.state.data?.some(isActiveMount) ? 1500 : false,
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
      if (mount.target.dataSourceId !== selectedSourceId || mount.target.mode !== mountMode) {
        return false;
      }
      return mountMode === 'physicalDisk'
        || String(mount.target.partitionIndex) === selectedPartitionIndex;
    }),
    [mountMode, mountsQuery.data, selectedPartitionIndex, selectedSourceId],
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

  const openDialog = useCallback(() => {
    mountMutation.reset();
    unmountMutation.reset();
    setMountPoint('auto');
    setMountMode('logicalPartition');
    setDialogOpen(true);
    void mountsQuery.refetch();
  }, [mountMutation, mountsQuery, unmountMutation]);

  const setDialogOpenSafely = useCallback((open: boolean) => {
    if (!open && (mountMutation.isPending || unmountMutation.isPending)) {
      return;
    }
    setDialogOpen(open);
  }, [mountMutation.isPending, unmountMutation.isPending]);

  const submit = useCallback(async () => {
    await mountMutation.mutateAsync();
  }, [mountMutation]);

  const unmount = useCallback(async (mountId: string) => {
    await unmountMutation.mutateAsync(mountId);
  }, [unmountMutation]);

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
    mounts: mountsQuery.data ?? [],
    selectedMount,
    isLoadingMounts: mountsQuery.isLoading,
    isSubmitting: mountMutation.isPending || unmountMutation.isPending,
    isMounting: mountMutation.isPending,
    isUnmounting: unmountMutation.isPending,
    error: mountMutation.error || unmountMutation.error
      ? errorMessage(mountMutation.error ?? unmountMutation.error, '挂载操作失败。')
      : undefined,
    submit,
    unmount,
    refresh: mountsQuery.refetch,
    mountPointOptions: MOUNT_POINT_OPTIONS,
  };
}

export type ImageMountModel = ReturnType<typeof useImageMountModel>;
