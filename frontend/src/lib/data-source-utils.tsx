import { Monitor, Server, Folder, Database, HardDrive } from 'lucide-react';

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
