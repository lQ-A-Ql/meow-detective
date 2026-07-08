import { Monitor, Server, Folder, Database, HardDrive } from 'lucide-react';
import type { DataSourceSummary } from '@/types/models';

export type DataSourcePlatform = 'windows' | 'linux' | 'unknown';

const LINUX_FILESYSTEMS = new Set([
  'ext2',
  'ext3',
  'ext4',
  'xfs',
  'btrfs',
  'lvm',
  'linux_lvm',
  'linux-swap',
  'swap',
]);

const WINDOWS_FILESYSTEMS = new Set([
  'ntfs',
  'fat',
  'fat12',
  'fat16',
  'fat32',
  'exfat',
]);

function normalizeToken(value: string | undefined): string {
  return value?.trim().toLowerCase().replace(/\s+/g, '_') ?? '';
}

function pathLooksWindows(value: string): boolean {
  return /^[a-z]:[\\/]/i.test(value)
    || value.includes('\\windows\\')
    || value.includes('/windows/')
    || value.includes('win10')
    || value.includes('win11')
    || value.includes('windows');
}

function pathLooksLinux(value: string): boolean {
  return value.includes('/etc/')
    || value.includes('/var/')
    || value.includes('/usr/')
    || value.includes('/home/')
    || value.includes('linux')
    || value.includes('ubuntu')
    || value.includes('debian')
    || value.includes('centos')
    || value.includes('proxmox')
    || value.includes('pve');
}

function partitionDescriptor(dataSource: DataSourceSummary): string {
  return normalizeToken(
    dataSource.partitions
      ?.map((partition) => [
        partition.name,
        partition.kindLabel,
        partition.filesystem,
        partition.typeGuid,
        partition.unlockHint,
      ].filter(Boolean).join(' '))
      .join(' ') ?? '',
  );
}

/** User-facing label for a data source kind. */
export function sourceKindLabel(kind: string): string {
  switch (kind) {
    case 'e01':
      return 'E01';
    case 'raw':
      return 'RAW';
    case 'logical_directory':
      return '目录';
    default:
      return kind;
  }
}

/** Display-only platform inference for data-source analysis filters. */
export function inferDataSourcePlatform(dataSource: DataSourceSummary): DataSourcePlatform {
  const filesystems = dataSource.partitions
    ?.map((partition) => normalizeToken(partition.filesystem))
    .filter(Boolean) ?? [];
  const partitions = partitionDescriptor(dataSource);

  if (filesystems.some((filesystem) => LINUX_FILESYSTEMS.has(filesystem))) {
    return 'linux';
  }
  if (filesystems.some((filesystem) => WINDOWS_FILESYSTEMS.has(filesystem))) {
    return 'windows';
  }
  if (pathLooksLinux(partitions)) {
    return 'linux';
  }
  if (pathLooksWindows(partitions)) {
    return 'windows';
  }

  const descriptor = normalizeToken(`${dataSource.name} ${dataSource.sourcePath} ${dataSource.readerKind ?? ''}`);
  if (pathLooksLinux(descriptor)) {
    return 'linux';
  }
  if (pathLooksWindows(descriptor)) {
    return 'windows';
  }

  return 'unknown';
}

export function dataSourcePlatformLabel(dataSource: DataSourceSummary): string {
  switch (inferDataSourcePlatform(dataSource)) {
    case 'windows':
      return 'Windows';
    case 'linux':
      return 'Linux';
    default:
      return '未知';
  }
}

/** Icon component for a data source kind, sized for inline badges. */
export function sourceKindIcon(kind: string) {
  switch (kind) {
    case 'e01':
      return HardDrive;
    case 'raw':
      return Server;
    case 'logical_directory':
      return Folder;
    default:
      return Monitor;
  }
}

/** Larger icon for forms/selection UI (not inline badges). */
export function sourceKindIconLarge(kind: string) {
  switch (kind) {
    case 'e01':
      return Database;
    case 'raw':
      return Server;
    case 'logical_directory':
      return Folder;
    default:
      return Monitor;
  }
}
