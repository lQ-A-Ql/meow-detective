import { openDialog, singleDialogPath } from '@/lib/platform/dialog';

export function useImportDataSourceDialogModel() {
  async function pickSourcePath(filterName: string): Promise<string | undefined> {
    try {
      const selected = await openDialog({
        directory: false,
        multiple: false,
        filters: [
          { name: filterName, extensions: ['e01', 'E01', 'dd', 'raw', 'img'] },
        ],
      });
      return singleDialogPath(selected) ?? undefined;
    } catch {
      return undefined;
    }
  }

  async function pickDirectoryPath(): Promise<string | undefined> {
    try {
      const selected = await openDialog({ directory: true, multiple: false });
      return singleDialogPath(selected) ?? undefined;
    } catch {
      return undefined;
    }
  }

  return {
    pickDirectoryPath,
    pickSourcePath,
  };
}
