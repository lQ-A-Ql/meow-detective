import { useMemo } from 'react';
import {
  useFileHandle, useFileViewer, useImagePreview, useMediaUrl,
  useTextPreview, useDocumentPreview,
} from '@/features/files/hooks';
import type { ApiErrorDto, FileEntryRow } from '@/types/models';
import type { FilePreviewKind, PreviewViewerTab } from '@/components/preview/FilePreviewTabs';
import { errorMessage, isApiErrorDto } from '@/lib/errors';
import { getFilePreviewKind } from '@/features/files/preview-file-kind';

function getPreviewKindFromMime(mime?: string): FilePreviewKind | undefined {
  const normalized = mime?.toLowerCase() ?? '';
  if (normalized.startsWith('image/')) return 'image';
  if (normalized.startsWith('video/')) return 'video';
  if (normalized.startsWith('audio/')) return 'audio';
  return undefined;
}

interface UseFilePreviewOptions {
  selectedFile: FileEntryRow | undefined;
  viewerTab: PreviewViewerTab;
}

function normalizePreviewError(error: unknown): ApiErrorDto | null {
  if (!error) return null;
  if (isApiErrorDto(error)) return error;
  return {
    code: 'FILE_PREVIEW_FAILED',
    message: errorMessage(error, '文件预览失败'),
    category: 'internal',
    recoverable: true,
  };
}

export function useFilePreview({
  selectedFile,
  viewerTab,
}: UseFilePreviewOptions) {
  const selectedFilePreviewKind = useMemo(() => {
    return selectedFile ? getFilePreviewKind(selectedFile) : undefined;
  }, [selectedFile]);
  const needsPreviewHandle =
    viewerTab === 'preview' &&
    Boolean(selectedFile?.id) &&
    selectedFilePreviewKind === undefined;
  const needsMetadataHandle =
    viewerTab === 'metadata' && Boolean(selectedFile?.id);
  const handleQuery = useFileHandle(
    selectedFile?.id,
    needsPreviewHandle || needsMetadataHandle
  );
  const fileHandle = handleQuery.data;
  const previewKind =
    viewerTab === 'preview'
      ? selectedFilePreviewKind ?? getPreviewKindFromMime(fileHandle?.mime)
      : undefined;
  const hexPreviewEnabled = viewerTab === 'hex' && Boolean(selectedFile?.id);
  const textPreviewEnabled = viewerTab === 'text' && Boolean(selectedFile?.id);
  const imagePreviewEnabled =
    viewerTab === 'preview' && previewKind === 'image' && Boolean(selectedFile?.id);
  const mediaPreviewEnabled =
    viewerTab === 'preview' &&
    (previewKind === 'video' || previewKind === 'audio') &&
    Boolean(selectedFile?.id);
  const documentPreviewEnabled =
    viewerTab === 'preview' && previewKind === 'document' && Boolean(selectedFile?.id);
  const viewerQuery = useFileViewer(selectedFile?.id, hexPreviewEnabled);
  const {
    data: viewer,
    setJumpOffsetInput,
    jumpToOffset,
    loadNextRange,
    loadPreviousRange,
  } = viewerQuery;
  const textQuery = useTextPreview(selectedFile?.id, textPreviewEnabled);
  const imageQuery = useImagePreview(selectedFile?.id, imagePreviewEnabled);
  const mediaQuery = useMediaUrl(selectedFile?.id, mediaPreviewEnabled);
  const documentQuery = useDocumentPreview(selectedFile?.id, documentPreviewEnabled);
  const activePreviewQuery =
    viewerTab === 'hex'
      ? viewerQuery
      : viewerTab === 'text'
        ? textQuery
        : viewerTab === 'metadata' || (viewerTab === 'preview' && !previewKind)
          ? handleQuery
          : previewKind === 'image'
            ? imageQuery
            : previewKind === 'video' || previewKind === 'audio'
              ? mediaQuery
              : documentQuery;
  const previewError = normalizePreviewError(activePreviewQuery.error);
  const onRetryPreview = activePreviewQuery.isError
    ? () => { void activePreviewQuery.refetch(); }
    : undefined;

  return {
    fileHandle,
    previewKind,
    hexPreviewEnabled,
    viewer,
    setJumpOffsetInput,
    jumpToOffset,
    loadNextRange,
    loadPreviousRange,
    textPreview: textQuery.data,
    imagePreview: imageQuery.data,
    mediaUrl: mediaQuery.data,
    documentPreview: documentQuery.data,
    previewError,
    onRetryPreview,
  };
}
