import {
  useFileHandle, useFileViewer, useImagePreview, useImageMetadata, useMediaUrl,
  useTextPreview, useDocumentPreview,
} from '@/features/files/hooks';
import i18n from '@/i18n';
import type { ApiErrorDto, FileEntryRow } from '@/types/models';
import type { FilePreviewKind, PreviewViewerTab } from '@/components/preview/FilePreviewTabs';
import { errorMessage, isApiErrorDto } from '@/lib/errors';

interface UseFilePreviewOptions {
  selectedFile: FileEntryRow | undefined;
  viewerTab: PreviewViewerTab;
  caseId?: string;
}

function getPreviewKindFromHandle(value: unknown): FilePreviewKind | undefined {
  return value === 'image' || value === 'video' || value === 'audio' || value === 'document'
    ? value
    : undefined;
}

function normalizePreviewError(error: unknown): ApiErrorDto | null {
  if (!error) return null;
  if (isApiErrorDto(error)) return error;
  return {
    code: 'FILE_PREVIEW_FAILED',
    message: errorMessage(error, i18n.t('viewer.error.title')),
    category: 'internal',
    recoverable: true,
  };
}

export function useFilePreview({
  selectedFile,
  viewerTab,
  caseId,
}: UseFilePreviewOptions) {
  const needsPreviewHandle =
    viewerTab === 'preview' &&
    Boolean(selectedFile?.id);
  const needsMetadataHandle =
    viewerTab === 'metadata' && Boolean(selectedFile?.id);
  const handleQuery = useFileHandle(
    selectedFile?.id,
    needsPreviewHandle || needsMetadataHandle,
    caseId,
  );
  const fileHandle = handleQuery.data;
  const detectedPreviewKind = getPreviewKindFromHandle(fileHandle?.previewKind);
  const previewKind =
    viewerTab === 'preview'
      ? detectedPreviewKind
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
  const viewerQuery = useFileViewer(selectedFile?.id, hexPreviewEnabled, caseId);
  const {
    data: viewer,
    setJumpOffsetInput,
    jumpToOffset,
    loadNextRange,
    loadPreviousRange,
  } = viewerQuery;
  const textQuery = useTextPreview(selectedFile?.id, textPreviewEnabled, caseId);
  const imageQuery = useImagePreview(selectedFile?.id, imagePreviewEnabled, caseId);
  const imageMetadataQuery = useImageMetadata(
    selectedFile?.id,
    Boolean(selectedFile?.id) && detectedPreviewKind === 'image',
    caseId,
  );
  const mediaQuery = useMediaUrl(selectedFile?.id, mediaPreviewEnabled, caseId);
  const documentQuery = useDocumentPreview(selectedFile?.id, documentPreviewEnabled, caseId);
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
    imageMetadata: imageMetadataQuery.data,
    imageMetadataLoading: imageMetadataQuery.isLoading,
    imageMetadataError: normalizePreviewError(imageMetadataQuery.error),
    onRetryImageMetadata: imageMetadataQuery.isError
      ? () => { void imageMetadataQuery.refetch(); }
      : undefined,
    mediaUrl: mediaQuery.data,
    documentPreview: documentQuery.data,
    previewError,
    onRetryPreview,
  };
}
