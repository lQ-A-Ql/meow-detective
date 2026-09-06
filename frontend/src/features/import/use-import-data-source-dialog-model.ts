import { useCallback } from 'react';
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
            extensions: ['e01', 'E01', 'ewf', 'dd', 'raw', 'img', 'iso', 'vmdk'],
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
    try {
      return await listLocalDisks();
    } catch {
      return [];
    }
  }, []);

  return {
    pickDirectoryPath,
    pickSourcePath,
    listLocalDisks: getLocalDisks,
  };
}
