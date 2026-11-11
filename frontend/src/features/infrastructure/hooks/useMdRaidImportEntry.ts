import { useCallback, useState } from 'react';
import { useImportDataSource } from '@/features/files/hooks';
import { useImportDataSourceDialogModel } from '@/features/import/use-import-data-source-dialog-model';
import type { ImportDataSourceRequest } from '@/types/models';

export function useMdRaidImportEntry() {
  const importMutation = useImportDataSource();
  const dialog = useImportDataSourceDialogModel();
  const [open, setOpen] = useState(false);
  const openDialog = useCallback(() => setOpen(true), []);

  const importRaidEvidence = useCallback(
    (request: ImportDataSourceRequest) => {
      importMutation.mutate(request, {
        onSuccess: () => setOpen(false),
      });
    },
    [importMutation],
  );

  return {
    open,
    openDialog,
    setOpen,
    importRaidEvidence,
    importPending: importMutation.isPending,
    pickSourcePath: dialog.pickSourcePath,
    pickDirectoryPath: dialog.pickDirectoryPath,
    listLocalDisks: dialog.listLocalDisks,
  };
}
