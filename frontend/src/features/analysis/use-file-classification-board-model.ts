import { useCallback, useMemo, useState } from 'react';
import type { PreviewViewerTab } from '@/components/preview/FilePreviewTabs';
import { useFilePreview } from '@/features/files/hooks/use-file-preview';
import { getDefaultFilePreviewTab } from '@/features/files/preview-file-kind';
import type { ClassifiedFileRow, FileEntryRow } from '@/types/models';

const IMAGE_MAGIC_TYPES = new Set(['JPEG', 'PNG', 'GIF', 'BMP', 'WEBP', 'ICO']);

function toFileEntryRow(row: ClassifiedFileRow): FileEntryRow {
  const extension = row.name.includes('.') ? row.name.split('.').pop() : undefined;
  return {
    id: row.fileId,
    path: row.path,
    name: row.name,
    entryType: 'file',
    size: row.size,
    ext: extension,
    deleted: false,
    hidden: false,
    system: false,
    readOnly: false,
    archive: false,
  };
}

function defaultPreviewTab(row: ClassifiedFileRow, file: FileEntryRow): PreviewViewerTab {
  if (row.magicType && IMAGE_MAGIC_TYPES.has(row.magicType)) return 'preview';
  return getDefaultFilePreviewTab(file);
}

export function useFileClassificationBoardModel() {
  const [selected, setSelected] = useState<ClassifiedFileRow>();
  const [viewerTab, setViewerTab] = useState<PreviewViewerTab>('hex');
  const selectedFile = useMemo(
    () => (selected ? toFileEntryRow(selected) : undefined),
    [selected],
  );
  const preview = useFilePreview({
    selectedFile,
    viewerTab,
  });

  const selectRow = useCallback((row: ClassifiedFileRow) => {
    const file = toFileEntryRow(row);
    setViewerTab(defaultPreviewTab(row, file));
    setSelected(row);
  }, []);

  return {
    ...preview,
    open: Boolean(selectedFile),
    onOpenChange(open: boolean) {
      if (!open) setSelected(undefined);
    },
    selectRow,
    selectedFile,
    selectedFileId: selected?.fileId,
    setViewerTab,
    viewerTab,
  };
}

export type FileClassificationBoardModel = ReturnType<typeof useFileClassificationBoardModel>;
