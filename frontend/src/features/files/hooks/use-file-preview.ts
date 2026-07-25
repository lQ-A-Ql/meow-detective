import { useMemo } from 'react';
import { useNavigate } from 'react-router';
import {
  useExtractFile, useFileHandle, useFileViewer, useImagePreview, useMediaUrl,
  useTextPreview, useDocumentPreview,
} from '@/features/files/hooks';
import type { FileEntryRow } from '@/types/models';
import type { FilePreviewKind } from '@/components/preview/FilePreviewTabs';

const IMAGE_EXTENSIONS = new Set(['jpg', 'jpeg', 'png', 'gif', 'bmp', 'webp', 'ico']);
const VIDEO_EXTENSIONS = new Set(['mp4', 'webm', 'avi', 'mkv']);
const AUDIO_EXTENSIONS = new Set(['mp3', 'wav', 'flac', 'aac', 'ogg']);
const DOCUMENT_EXTENSIONS = new Set([
  'pdf', 'docx', 'xlsx', 'pptx', 'sqlite', 'sqlite3', 'db', 'db3',
  // Legacy OLE Office formats are routed to the document chain so the
  // backend's typed "not document-like" error is surfaced instead of a
  // silent placeholder.
  'doc', 'xls', 'ppt',
]);

function getPreviewKindFromExtension(ext?: string): FilePreviewKind | undefined {
  const normalized = ext?.toLowerCase().replace(/^\./, '');
  if (!normalized) return undefined;
  if (IMAGE_EXTENSIONS.has(normalized)) return 'image';
  if (VIDEO_EXTENSIONS.has(normalized)) return 'video';
  if (AUDIO_EXTENSIONS.has(normalized)) return 'audio';
  if (DOCUMENT_EXTENSIONS.has(normalized)) return 'document';
  return undefined;
}

function getPreviewKindFromMime(mime?: string): FilePreviewKind | undefined {
  const normalized = mime?.toLowerCase() ?? '';
  if (normalized.startsWith('image/')) return 'image';
  if (normalized.startsWith('video/')) return 'video';
  if (normalized.startsWith('audio/')) return 'audio';
  return undefined;
}

interface UseFilePreviewOptions {
  selectedFile: FileEntryRow | undefined;
  viewerTab: string;
  setSelectedTimelineId: (id?: string) => void;
}

export function useFilePreview({
  selectedFile,
  viewerTab,
  setSelectedTimelineId,
}: UseFilePreviewOptions) {
  const navigate = useNavigate();
  const selectedFilePreviewKind = useMemo(() => {
    const ext = selectedFile?.ext ?? selectedFile?.name.split('.').pop();
    return getPreviewKindFromExtension(ext);
  }, [selectedFile?.ext, selectedFile?.name]);
  const needsPreviewHandle =
    viewerTab === 'preview' &&
    Boolean(selectedFile?.id) &&
    selectedFilePreviewKind === undefined;
  const needsMetadataHandle =
    viewerTab === 'metadata' && Boolean(selectedFile?.id);
  const { data: fileHandle } = useFileHandle(
    selectedFile?.id,
    needsPreviewHandle || needsMetadataHandle
  );
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
  const {
    data: viewer,
    setJumpOffsetInput,
    jumpToOffset,
    loadNextRange,
    loadPreviousRange,
  } = useFileViewer(selectedFile?.id, hexPreviewEnabled);
  const { data: textPreview } = useTextPreview(selectedFile?.id, textPreviewEnabled);
  const { data: imagePreview } = useImagePreview(selectedFile?.id, imagePreviewEnabled);
  const { data: mediaUrl } = useMediaUrl(selectedFile?.id, mediaPreviewEnabled);
  const { data: documentPreview } = useDocumentPreview(selectedFile?.id, documentPreviewEnabled);
  const extractFile = useExtractFile();

  const onViewTimeline = () => {
    if (selectedFile) {
      setSelectedTimelineId(selectedFile.id);
      navigate('/timeline');
    }
  };

  return {
    fileHandle,
    previewKind,
    hexPreviewEnabled,
    viewer,
    setJumpOffsetInput,
    jumpToOffset,
    loadNextRange,
    loadPreviousRange,
    textPreview,
    imagePreview,
    mediaUrl,
    documentPreview,
    extractFile,
    onViewTimeline,
  };
}
