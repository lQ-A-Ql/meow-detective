import { useCallback, useMemo, useState } from 'react';
import type { PreviewViewerTab } from '@/components/preview/FilePreviewTabs';
import { useFilePreview } from '@/features/files/hooks/use-file-preview';
import { getDefaultFilePreviewTab } from '@/features/files/preview-file-kind';
import type { FileEntryRow, SearchFileHit } from '@/types/models';

function toFileEntryRow(hit: SearchFileHit): FileEntryRow {
  return {
    id: hit.fileId,
    path: hit.path,
    name: hit.name,
    entryType: hit.entryType === 'directory' ? 'directory' : 'file',
    size: hit.size,
    ext: hit.extension,
    modifiedAt: hit.modifiedAt,
    deleted: hit.deleted,
    hidden: hit.hidden,
    system: hit.system,
    encrypted: hit.encrypted,
  };
}

export function useSearchPreviewModel() {
  const [previewHit, setPreviewHit] = useState<SearchFileHit>();
  const [viewerTab, setViewerTab] = useState<PreviewViewerTab>('metadata');
  const selectedFile = useMemo(
    () => (previewHit ? toFileEntryRow(previewHit) : undefined),
    [previewHit],
  );
  const previewableFile = selectedFile?.entryType === 'file' ? selectedFile : undefined;
  const preview = useFilePreview({
    selectedFile: previewableFile,
    viewerTab,
  });

  const openHit = useCallback((hit: SearchFileHit) => {
    const file = toFileEntryRow(hit);
    setViewerTab(getDefaultFilePreviewTab(file));
    setPreviewHit(hit);
  }, []);
  const onOpenChange = useCallback((open: boolean) => {
    if (!open) setPreviewHit(undefined);
  }, []);

  return {
    ...preview,
    open: Boolean(previewHit),
    onOpenChange,
    openHit,
    selectedFile,
    setViewerTab,
    viewerTab,
  };
}

export type SearchPreviewModel = ReturnType<typeof useSearchPreviewModel>;
