import { useMutation, useQuery } from '@tanstack/react-query';
import {
  classifyFiles,
  generateAnalysisSummary,
  getSystemInfo,
} from '@/lib/api/analysis';
import { useCurrentCase } from '@/features/case/hooks';

export function useAnalysisSystemInfo() {
  const currentCase = useCurrentCase();
  return useQuery({
    queryKey: ['analysis', 'system-info', currentCase.data?.id ?? null],
    queryFn: getSystemInfo,
    enabled: currentCase.isSuccess && Boolean(currentCase.data),
    retry: false,
  });
}

export function useAnalysisClassifications(sampleSize = 1000) {
  const currentCase = useCurrentCase();
  return useQuery({
    queryKey: ['analysis', 'classifications', currentCase.data?.id ?? null, sampleSize],
    queryFn: () => classifyFiles(sampleSize),
    enabled: currentCase.isSuccess && Boolean(currentCase.data),
    retry: false,
  });
}

export function useGenerateAnalysisSummary() {
  return useMutation({
    mutationFn: generateAnalysisSummary,
  });
}
