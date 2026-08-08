import type {
  DataSourcePartition,
  DeletedFileRecovery,
  DeletedRecoveryExport,
  DeletedRecoveryHashSearch,
  DeletedRecoveryFailure,
  DeletedRecoveryPage,
  RecoveryProvenanceRange,
} from '@/types/models';

export type DeletedRecoveryViewState =
  | 'unsupported'
  | 'unscanned'
  | 'loading'
  | 'ready'
  | 'error';

export interface DeletedRecoveryPreviewWindow {
  recoveryId: string;
  offset: number;
  bytes: number[];
  declaredSize: number;
  eof: boolean;
  verifiedRangeOrdinals: number[];
}

export interface DeletedRecoveryViewModel {
  partitions: DataSourcePartition[];
  selectedPartitionIndex?: number;
  selectPartition: (partitionIndex: number) => void;
  state: DeletedRecoveryViewState;
  error?: string;
  page?: DeletedRecoveryPage;
  recoveries: DeletedFileRecovery[];
  total: number;
  failures: DeletedRecoveryFailure[];
  selectedRecovery?: DeletedFileRecovery;
  selectedRecoveryId?: string;
  selectRecovery: (recoveryId: string) => void;
  contentRanges: RecoveryProvenanceRange[];
  selectedRangeOrdinal?: number;
  selectRange: (ordinal: number) => void;
  preview?: DeletedRecoveryPreviewWindow;
  lastExport?: DeletedRecoveryExport;
  hashQuery: string;
  hashQueryValid: boolean;
  hashSearch?: DeletedRecoveryHashSearch;
  hashSearchError?: string;
  setHashQuery: (value: string) => void;
  runHashSearch: () => void;
  clearHashSearch: () => void;
  scanning: boolean;
  hashSearching: boolean;
  reading: boolean;
  exporting: boolean;
  runScan: () => void;
  readSelectedRange: () => void;
  exportSelected: () => void;
  hasPreviousPage: boolean;
  hasNextPage: boolean;
  previousPage: () => void;
  nextPage: () => void;
}
