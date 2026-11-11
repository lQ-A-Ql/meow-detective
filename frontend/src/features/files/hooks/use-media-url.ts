import { useQuery } from '@tanstack/react-query';
import { useEffect, useMemo } from 'react';
import { getMediaUrl, readMediaRange } from '@/lib/api/files';
import type { MediaPreview } from '@/types/models';
import { getPreviewSettings } from '@/lib/settings';
import {
  adoptPreviewHandle,
  ensurePreviewRequestActive,
  releasePreviewHandle,
  usePreviewHandleOwner,
} from '../previewHandleOwner';
import { PREVIEW_STALE_TIME } from './preview-constants';

/**
 * Hook to get media URL for video/audio playback.
 * Returns an inline data URL for bounded media previews.
 */
export function useMediaUrl(fileId?: string, enabled = true, caseId?: string) {
  const preview = useMemo(() => getPreviewSettings(), []);
  const owner = usePreviewHandleOwner(`${fileId ?? ''}:media`);
  const query = useQuery<MediaPreview | null, Error>({
    queryKey: ['files', 'media', caseId ?? null, fileId],
    enabled: Boolean(fileId) && enabled,
    retry: false,
    staleTime: PREVIEW_STALE_TIME,
    gcTime: 0,
    queryFn: async ({ signal }): Promise<MediaPreview | null> => {
      if (!fileId) return null;
      const media = await getMediaUrl(fileId);
      if (media.handleId) {
        await adoptPreviewHandle(owner, media.handleId, signal);
      }
      try {
        if (media.url && media.mode === 'protocol') {
          return {
            ...media,
            previewMode: 'protocol' as const,
          };
        }
        if (media.url || !media.handleId || !media.canReadRanges) {
          return {
            ...media,
            previewMode: media.mode,
          };
        }

        const range = await readMediaRange({
          handleId: media.handleId,
          offset: 0,
          length: preview.maxViewerRangeLength,
        });
        await ensurePreviewRequestActive(owner, media.handleId, signal);
        if (!range.bytesRead) {
          return {
            ...media,
            previewMode: 'rangeFallback' as const,
            previewBytes: 0,
          };
        }
        const byteCharacters = atob(range.bytesBase64);
        const byteNumbers = new Array(byteCharacters.length);
        for (let index = 0; index < byteCharacters.length; index += 1) {
          byteNumbers[index] = byteCharacters.charCodeAt(index);
        }
        const blob = new Blob([new Uint8Array(byteNumbers)], { type: media.mimeType });
        const objectUrl = URL.createObjectURL(blob);
        if (signal.aborted) {
          // React Query discards an aborted result, so the revoke effect keyed on the query
          // data would never run and the object URL would leak for the whole session.
          URL.revokeObjectURL(objectUrl);
          throw new DOMException('Media preview was aborted', 'AbortError');
        }
        return {
          ...media,
          mode: media.mode ?? 'rangeFallback',
          previewMode: 'rangeFallback' as const,
          previewBytes: range.bytesRead,
          url: objectUrl,
        };
      } catch (error) {
        await releasePreviewHandle(owner, media.handleId);
        throw error;
      }
    },
  });

  const blobUrl = query.data?.url?.startsWith('blob:') ? query.data.url : undefined;
  useEffect(() => {
    return () => {
      if (blobUrl) {
        URL.revokeObjectURL(blobUrl);
      }
    };
  }, [blobUrl]);

  return query;
}
