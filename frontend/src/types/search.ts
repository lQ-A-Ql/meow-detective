export type SearchEntryType = 'any' | 'file' | 'directory';
export type SearchSortKey = 'name' | 'path' | 'size' | 'modifiedAt';
export type SearchSortDirection = 'asc' | 'desc';

export interface SearchCoverage {
  readySourceCount: number;
  indexedSourceCount: number;
  expectedEntryCount: number;
  indexedEntryCount: number;
  missingSourceIds: string[];
  complete: boolean;
}

export interface SearchFileHit {
  fileId: string;
  dataSourceId: string;
  dataSourceName: string;
  name: string;
  path: string;
  entryType: string;
  extension?: string;
  size?: number;
  modifiedAt?: string;
  deleted: boolean;
  hidden: boolean;
  system: boolean;
  encrypted: boolean;
}

export interface SearchRequestOptions {
  matchPath: boolean;
  entryType: SearchEntryType;
  extensions: string[];
  dataSourceIds: string[];
  sortKey: SearchSortKey;
  sortDirection: SearchSortDirection;
}

export interface SearchResultPage {
  total: number;
  available: number;
  truncated: boolean;
  tookMs: number;
  items: SearchFileHit[];
  coverage: SearchCoverage;
  nextCursor?: string;
}
