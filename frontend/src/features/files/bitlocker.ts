import type { DataSourcePartition } from '@/types/models';

export interface BitLockerTarget {
  dataSourceId: string;
  partitionIndex: number;
}

export function isBitLockerPartition(partition?: DataSourcePartition): boolean {
  if (!partition) {
    return false;
  }
  return [partition.kindLabel, partition.filesystem, partition.status]
    .filter(Boolean)
    .some((value) => value!.toLowerCase().includes('bitlocker'));
}
