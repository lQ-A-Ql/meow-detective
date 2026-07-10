export interface FileTreeNode {
  id: string;
  name: string;
  depth: number;
  hasChildren: boolean;
  dataSourceId?: string;
  entryType?: 'file' | 'directory';
  size?: number;
  deleted: boolean;
  hidden: boolean;
  system: boolean;
  encrypted?: boolean;
  nodeType?: string;
  status?: string;
  expanded?: boolean;
  active?: boolean;
}

export interface FileChildrenPage {
  children: FileTreeNode[];
  totalCount: number;
  offset?: number;
  limit?: number;
  truncated?: boolean;
}

export interface FileEntryRow {
  id: string;
  parentId?: string;
  path: string;
  name: string;
  entryType: 'file' | 'directory';
  size?: number;
  ext?: string;
  deleted: boolean;
  hidden: boolean;
  system: boolean;
  encrypted?: boolean;
  createdAt?: string;
  modifiedAt?: string;
  accessedAt?: string;
  changedAt?: string;
  hashSha256?: string;
}

export interface FileRowsPage {
  rows: FileEntryRow[];
  totalCount: number;
  offset: number;
  limit: number;
  truncated: boolean;
}

export interface FileJumpContext {
  target: FileEntryRow;
  directory: FileEntryRow;
  ancestorDirectoryIds: string[];
  rowOffset: number;
  requiresShowHidden: boolean;
}

export type ImportTargetPlatform = 'windows' | 'linux' | 'macos' | 'unknown';
export type ImportSourceKind = 'auto' | 'linuxCluster';

export interface ImportDataSourceRequest {
  sourcePath: string;
  sourceKind?: ImportSourceKind;
  platform?: ImportTargetPlatform;
  profile?: string;
}
