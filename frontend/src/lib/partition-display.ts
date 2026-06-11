import type { DataSourcePartition } from '@/types/models';

const PARTITION_ROOT_RE = /^Partition\s+(\d+)(?:\s*\(([^)]+)\))?/i;

function normalizePartitionLabel(value?: string) {
  const text = value?.trim();
  if (!text) {
    return undefined;
  }
  const upper = text.toUpperCase();
  if (upper.includes('RECOVERY') || text.includes('恢复')) {
    return 'RECOVERY';
  }
  if (upper.includes('BITLOCKER')) {
    return 'BITLOCKER';
  }
  if (upper.includes('EXFAT')) {
    return 'EXFAT';
  }
  if (upper.includes('NTFS')) {
    return 'NTFS';
  }
  if (upper.includes('FAT')) {
    return 'FAT';
  }
  return upper || undefined;
}

function isRecoveryPartition(partition: DataSourcePartition) {
  return [
    partition.name,
    partition.filesystem,
    partition.kindLabel,
  ].some((value) => normalizePartitionLabel(value) === 'RECOVERY');
}

export function partitionDisplayLabel(partition: DataSourcePartition) {
  if (isRecoveryPartition(partition)) {
    return 'RECOVERY';
  }
  return normalizePartitionLabel(partition.filesystem)
    ?? normalizePartitionLabel(partition.kindLabel)
    ?? 'UNKNOWN';
}

export function formatPartitionDisplayName(partition: DataSourcePartition) {
  return `分区${partition.index}（${partitionDisplayLabel(partition)}）`;
}

export function formatPartitionRootDisplayName(
  rootName: string,
  partitions: DataSourcePartition[] = [],
) {
  const match = rootName.trim().match(PARTITION_ROOT_RE);
  if (!match) {
    return rootName;
  }

  const index = Number.parseInt(match[1] ?? '', 10);
  if (!Number.isFinite(index)) {
    return rootName;
  }

  const partition = partitions.find((item) => item.index === index);
  if (partition) {
    return formatPartitionDisplayName(partition);
  }

  const label = normalizePartitionLabel(match[2]) ?? 'UNKNOWN';
  return `分区${index}（${label}）`;
}
