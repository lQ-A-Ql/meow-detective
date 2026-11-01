import type { ImportDataSourceRequest, ImportTargetPlatform, LocalDisk } from '@/types/models';

export type SourceKind = 'auto' | 'linuxEvidenceSet' | 'localDisk';
export type Step = 'platform' | 'source' | 'config' | 'confirm';

export interface ImportDataSourceDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onImport: (request: ImportDataSourceRequest) => void;
  importPending: boolean;
  pickSourcePath: (filterName: string) => Promise<string | undefined>;
  pickDirectoryPath: () => Promise<string | undefined>;
  listLocalDisks?: () => Promise<LocalDisk[]>;
  initialPlatform?: ImportTargetPlatform;
  initialSourceKind?: SourceKind;
  initialStep?: Step;
}
