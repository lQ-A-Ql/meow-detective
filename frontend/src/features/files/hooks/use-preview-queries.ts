import { useQuery } from '@tanstack/react-query';
import {
  getDocumentPreview,
  getImageMetadata,
  getImagePreview,
  getTextPreview,
} from '@/lib/api/files';
import { PREVIEW_STALE_TIME } from './preview-constants';

/**
 * Hook to get text preview for a file.
 * Returns text content with encoding detection.
 */
export function useTextPreview(fileId?: string, enabled = true, caseId?: string) {
  return useQuery({
    queryKey: ['files', 'text', caseId ?? null, fileId],
    enabled: Boolean(fileId) && enabled,
    retry: false,
    staleTime: PREVIEW_STALE_TIME,
    gcTime: 60_000,
    queryFn: async () => {
      if (!fileId) return null;
      return await getTextPreview(fileId, 1024 * 1024); // 1MB limit
    },
  });
}

/**
 * Hook to get image preview for a file.
 * Returns an inline data URL for bounded image previews.
 */
export function useImagePreview(fileId?: string, enabled = true, caseId?: string) {
  return useQuery({
    queryKey: ['files', 'image', caseId ?? null, fileId],
    enabled: Boolean(fileId) && enabled,
    retry: false,
    staleTime: PREVIEW_STALE_TIME,
    gcTime: 60_000,
    queryFn: async () => {
      if (!fileId) return null;
      return await getImagePreview(fileId);
    },
  });
}

/** Load bounded EXIF/GPS metadata through the source-bound backend reader. */
export function useImageMetadata(fileId?: string, enabled = true, caseId?: string) {
  return useQuery({
    queryKey: ['files', 'image-metadata', caseId ?? null, fileId],
    enabled: Boolean(fileId) && enabled,
    retry: false,
    staleTime: PREVIEW_STALE_TIME,
    gcTime: 60_000,
    queryFn: async () => {
      if (!fileId) return null;
      return await getImageMetadata(fileId);
    },
  });
}

/**
 * Hook for structured document preview (PDF, Office Open XML, SQLite).
 */
export function useDocumentPreview(fileId?: string, enabled = true, caseId?: string) {
  return useQuery({
    queryKey: ['files', 'document', caseId ?? null, fileId],
    enabled: Boolean(fileId) && enabled,
    retry: false,
    staleTime: PREVIEW_STALE_TIME,
    gcTime: 60_000,
    queryFn: async () => {
      if (!fileId) return null;
      return await getDocumentPreview(fileId);
    },
  });
}
