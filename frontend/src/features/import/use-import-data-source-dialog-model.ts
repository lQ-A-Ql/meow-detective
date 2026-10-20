import { useCallback } from 'react';
import { IMPORT_SOURCE_FILE_EXTENSIONS } from '@/design/import-source';
import { openDialog, singleDialogPath } from '@/lib/platform/dialog';
import { listLocalDisks } from '@/lib/api/files';

export function useImportDataSourceDialogModel() {
  const pickSourcePath = useCallback(async (filterName: string): Promise<string | undefined> => {
    try {
      const selected = await openDialog({
        directory: false,
        multiple: false,
        filters: [
          {
            name: filterName,
            extensions: [...IMPORT_SOURCE_FILE_EXTENSIONS],
          },
        ],
      });
      return singleDialogPath(selected) ?? undefined;
    } catch {
      return undefined;
    }
  }, []);

  const pickDirectoryPath = useCallback(async (): Promise<string | undefined> => {
    try {
      const selected = await openDialog({ directory: true, multiple: false });
      return singleDialogPath(selected) ?? undefined;
    } catch {
      return undefined;
    }
  }, []);

  const getLocalDisks = useCallback(async () => {
    return listLocalDisks();
  }, []);

  return {
    pickDirectoryPath,
    pickSourcePath,
    listLocalDisks: getLocalDisks,
  };
}
