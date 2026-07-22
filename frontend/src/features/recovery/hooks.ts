import { useEffect, useMemo, useRef, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { useCurrentCase } from '@/features/case/hooks';
import {
  exportDeletedRecovery,
  listDeletedRecoveries,
  readDeletedRecoveryRange,
  runDeletedRecovery,
} from '@/lib/api/files';
import { errorMessage, isApiErrorDto } from '@/lib/errors';
import { saveDialog } from '@/lib/platform/dialog';
import type {
  DataSourcePartition,
  DataSourceSummary,
  DeletedFileRecovery,
  RecoveryProvenanceRange,
} from '@/types/models';
import type {
  DeletedRecoveryPreviewWindow,
  DeletedRecoveryViewModel,
  DeletedRecoveryViewState,
} from './types';

const PAGE_SIZE = 100;
const MAX_READ_LENGTH = 1024 * 1024;

function isSupportedPartition(
  partition: DataSourcePartition,
  platform: DataSourceSummary['platform'],
) {
  const filesystem = partition.filesystem?.trim().toLowerCase();
  return platform === 'windows'
    ? filesystem === 'ntfs'
    : filesystem === 'ext4' || filesystem === 'xfs';
}

function isMissingScan(error: unknown) {
  return isApiErrorDto(error) && error.code === 'RECOVERY_SCAN_NOT_FOUND';
}

function decodeBase64(value: string) {
  const encoded = atob(value);
  return Array.from(encoded, (character) => character.charCodeAt(0));
}

function recoveredFileName(recovery: DeletedFileRecovery) {
  const normalizedPath = recovery.originalPath?.replace(/\\/g, '/');
  const name = normalizedPath?.split('/').filter(Boolean).at(-1);
  return name || `recovered-inode-${recovery.inode}`;
}

function verifiedContentRanges(recovery?: DeletedFileRecovery) {
  if (!recovery) {
    return [];
  }
  return recovery.provenanceRanges.filter(
    (range) => range.rangeRole === 'content'
      && range.length > 0
      && range.allocationState === 'free'
      && Boolean(range.sha256),
  );
}

export function useDeletedRecoveryModel(
  source: DataSourceSummary | undefined,
  enabled: boolean,
): DeletedRecoveryViewModel {
  const currentCase = useCurrentCase();
  const queryClient = useQueryClient();
  const partitions = useMemo(
    () => source?.platform === 'windows' || source?.platform === 'linux'
      ? (source.partitions ?? []).filter(
          (partition) => isSupportedPartition(partition, source.platform),
        )
      : [],
    [source],
  );
  const partitionIdentity = partitions.map((partition) => partition.index).join(',');
  const [selectedPartitionIndex, setSelectedPartitionIndex] = useState<number>();
  const [offset, setOffset] = useState(0);
  const [selectedRecoveryId, setSelectedRecoveryId] = useState<string>();
  const [selectedRangeOrdinal, setSelectedRangeOrdinal] = useState<number>();
  const [preview, setPreview] = useState<DeletedRecoveryPreviewWindow>();
  const sourceIdRef = useRef(source?.id);
  sourceIdRef.current = source?.id;

  useEffect(() => {
    setSelectedPartitionIndex(partitions[0]?.index);
    setOffset(0);
    setSelectedRecoveryId(undefined);
    setSelectedRangeOrdinal(undefined);
    setPreview(undefined);
  }, [partitionIdentity, source?.id]);

  const queryKey = [
    'recovery',
    'deleted',
    currentCase.data?.id ?? null,
    source?.id ?? null,
    selectedPartitionIndex ?? null,
    offset,
  ] as const;
  const recoveryQuery = useQuery({
    queryKey,
    queryFn: () => listDeletedRecoveries(
      source?.id ?? '',
      selectedPartitionIndex ?? 0,
      offset,
      PAGE_SIZE,
    ),
    enabled: enabled
      && currentCase.isSuccess
      && Boolean(currentCase.data)
      && (source?.platform === 'windows' || source?.platform === 'linux')
      && selectedPartitionIndex !== undefined,
    retry: false,
  });

  const scanMutation = useMutation({
    mutationFn: ({
      dataSourceId,
      partitionIndex,
    }: {
      dataSourceId: string;
      partitionIndex: number;
    }) => runDeletedRecovery(dataSourceId, partitionIndex),
    onSuccess: async (result, request) => {
      if (request.dataSourceId !== sourceIdRef.current || result.dataSourceId !== request.dataSourceId) {
        return;
      }
      setOffset(0);
      setSelectedRecoveryId(undefined);
      setPreview(undefined);
      await queryClient.invalidateQueries({
        queryKey: ['recovery', 'deleted', currentCase.data?.id ?? null, source?.id ?? null],
      });
    },
  });

  const readMutation = useMutation({
    mutationFn: async ({
      recovery,
      range,
    }: {
      recovery: DeletedFileRecovery;
      range: RecoveryProvenanceRange;
    }) => {
      const response = await readDeletedRecoveryRange(
        recovery.dataSourceId,
        recovery.id,
        range.logicalOffset,
        Math.min(range.length, MAX_READ_LENGTH),
      );
      return {
        dataSourceId: recovery.dataSourceId,
        recoveryId: response.recoveryId,
        offset: response.offset,
        bytes: decodeBase64(response.bytesBase64),
        declaredSize: response.declaredSize,
        eof: response.eof,
        verifiedRangeOrdinals: response.verifiedRangeOrdinals,
      };
    },
    onSuccess: ({ dataSourceId, ...result }) => {
      if (dataSourceId === sourceIdRef.current) {
        setPreview(result satisfies DeletedRecoveryPreviewWindow);
      }
    },
  });

  const exportMutation = useMutation({
    mutationFn: async (recovery: DeletedFileRecovery) => {
      const destinationPath = await saveDialog({ defaultPath: recoveredFileName(recovery) });
      if (!destinationPath) {
        return undefined;
      }
      const result = await exportDeletedRecovery(
        recovery.dataSourceId,
        recovery.id,
        destinationPath,
        false,
      );
      return { dataSourceId: recovery.dataSourceId, result };
    },
    onSuccess: (outcome) => {
      if (outcome && outcome.dataSourceId === sourceIdRef.current) {
        toast.success(`恢复文件已导出 (${outcome.result.bytesWritten} bytes)`);
      }
    },
    onError: (error) => toast.error(`恢复文件导出失败: ${errorMessage(error)}`),
  });

  const resetScanMutation = scanMutation.reset;
  const resetReadMutation = readMutation.reset;
  const resetExportMutation = exportMutation.reset;
  useEffect(() => {
    resetScanMutation();
    resetReadMutation();
    resetExportMutation();
  }, [partitionIdentity, resetExportMutation, resetReadMutation, resetScanMutation, source?.id]);

  const recoveries = recoveryQuery.data?.recoveries ?? [];
  const selectedRecovery = recoveries.find((recovery) => recovery.id === selectedRecoveryId);
  const contentRanges = useMemo(
    () => verifiedContentRanges(selectedRecovery),
    [selectedRecovery],
  );

  useEffect(() => {
    if (selectedRecoveryId && !selectedRecovery) {
      setSelectedRecoveryId(undefined);
      setSelectedRangeOrdinal(undefined);
      setPreview(undefined);
    }
  }, [selectedRecovery, selectedRecoveryId]);

  useEffect(() => {
    if (!contentRanges.some((range) => range.ordinal === selectedRangeOrdinal)) {
      setSelectedRangeOrdinal(contentRanges[0]?.ordinal);
    }
  }, [contentRanges, selectedRangeOrdinal]);

  const missingScan = isMissingScan(recoveryQuery.error);
  let state: DeletedRecoveryViewState;
  if (partitions.length === 0) {
    state = 'unsupported';
  } else if (recoveryQuery.isLoading) {
    state = 'loading';
  } else if (missingScan) {
    state = 'unscanned';
  } else if (recoveryQuery.error) {
    state = 'error';
  } else if (recoveryQuery.data) {
    state = 'ready';
  } else {
    state = 'unscanned';
  }

  const scanError = scanMutation.variables?.dataSourceId === source?.id
    ? scanMutation.error
    : undefined;
  const readError = readMutation.variables?.recovery.dataSourceId === source?.id
    ? readMutation.error
    : undefined;
  const exportError = exportMutation.variables?.dataSourceId === source?.id
    ? exportMutation.error
    : undefined;
  const operationError = scanError ?? readError ?? exportError;
  const error = recoveryQuery.error && !missingScan
    ? errorMessage(recoveryQuery.error)
    : operationError
      ? errorMessage(operationError)
      : undefined;
  const selectedRange = contentRanges.find((range) => range.ordinal === selectedRangeOrdinal);
  const scanResult = scanMutation.data;
  const exportResult = exportMutation.data;

  function selectPartition(partitionIndex: number) {
    setSelectedPartitionIndex(partitionIndex);
    setOffset(0);
    setSelectedRecoveryId(undefined);
    setSelectedRangeOrdinal(undefined);
    setPreview(undefined);
    scanMutation.reset();
    readMutation.reset();
    exportMutation.reset();
  }

  function selectRecovery(recoveryId: string) {
    setSelectedRecoveryId(recoveryId);
    setSelectedRangeOrdinal(undefined);
    setPreview(undefined);
    readMutation.reset();
    exportMutation.reset();
  }

  function changePage(nextOffset: number) {
    setOffset(nextOffset);
    setSelectedRecoveryId(undefined);
    setSelectedRangeOrdinal(undefined);
    setPreview(undefined);
  }

  return {
    partitions,
    selectedPartitionIndex,
    selectPartition,
    state,
    error,
    page: recoveryQuery.data,
    recoveries,
    total: recoveryQuery.data?.total ?? 0,
    failures: scanResult && scanResult.dataSourceId === source?.id ? scanResult.failures : [],
    selectedRecovery,
    selectedRecoveryId,
    selectRecovery,
    contentRanges,
    selectedRangeOrdinal,
    selectRange: (ordinal) => {
      setSelectedRangeOrdinal(ordinal);
      setPreview(undefined);
    },
    preview,
    lastExport: exportResult && exportResult.dataSourceId === source?.id
      ? exportResult.result
      : undefined,
    scanning: scanMutation.isPending,
    reading: readMutation.isPending,
    exporting: exportMutation.isPending,
    runScan: () => {
      if (source?.id && selectedPartitionIndex !== undefined) {
        scanMutation.mutate({ dataSourceId: source.id, partitionIndex: selectedPartitionIndex });
      }
    },
    readSelectedRange: () => {
      if (selectedRecovery && selectedRange) {
        readMutation.mutate({ recovery: selectedRecovery, range: selectedRange });
      }
    },
    exportSelected: () => {
      if (selectedRecovery?.completeness === 'complete') {
        exportMutation.mutate(selectedRecovery);
      }
    },
    hasPreviousPage: offset > 0,
    hasNextPage: offset + recoveries.length < (recoveryQuery.data?.total ?? 0),
    previousPage: () => changePage(Math.max(0, offset - PAGE_SIZE)),
    nextPage: () => changePage(offset + PAGE_SIZE),
  };
}
