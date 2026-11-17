import { useQuery } from '@tanstack/react-query';
import { analyzeImageSteg } from '@/lib/api/files';
import { errorMessage } from '@/lib/errors';
import type { ImageStegReport } from '@/types/viewer';

const IMAGE_STEG_GC_TIME = 30 * 60_000;

export interface ImageStegModel {
  report?: ImageStegReport;
  running: boolean;
  errorText?: string;
  run: () => void;
}

/**
 * Session-only steganalysis. The analysis runs only when the investigator
 * presses the button; results are never persisted to the case.
 */
export function useImageSteg(fileId?: string, caseId?: string): ImageStegModel {
  const query = useQuery({
    queryKey: ['files', 'image-steg', caseId ?? null, fileId ?? null],
    enabled: false,
    retry: false,
    staleTime: Infinity,
    gcTime: IMAGE_STEG_GC_TIME,
    queryFn: async () => {
      if (!fileId) return null;
      return await analyzeImageSteg(fileId);
    },
  });

  return {
    report: query.data ?? undefined,
    running: query.isFetching,
    errorText: query.error ? errorMessage(query.error) : undefined,
    run: () => {
      if (!fileId || query.isFetching) return;
      void query.refetch();
    },
  };
}
