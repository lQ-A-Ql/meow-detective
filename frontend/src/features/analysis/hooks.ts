import { useMutation, useQuery } from '@tanstack/react-query';
import {
  classifyFiles,
  generateAnalysisSummary,
  getEvidenceClassificationSummary,
  getSystemInfo,
  runEvidenceClassification,
} from '@/lib/api/analysis';
import { useCurrentCase } from '@/features/case/hooks';
import { useQueryClient } from '@tanstack/react-query';

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

export function useEvidenceClassificationSummary() {
  const currentCase = useCurrentCase();
  return useQuery({
    queryKey: ['analysis', 'evidence-classification', currentCase.data?.id ?? null],
    queryFn: getEvidenceClassificationSummary,
    enabled: currentCase.isSuccess && Boolean(currentCase.data),
    retry: false,
  });
}

export function useRunEvidenceClassification() {
  const qc = useQueryClient();
  const currentCase = useCurrentCase();
  return useMutation({
    mutationFn: (categories?: string[]) => runEvidenceClassification(categories ?? []),
    onSuccess: () => {
      qc.invalidateQueries({
        queryKey: ['analysis', 'evidence-classification', currentCase.data?.id ?? null],
      });
      qc.invalidateQueries({ queryKey: ['artifacts'] });
    },
  });
}

export function useGenerateAnalysisSummary() {
  return useMutation({
    mutationFn: generateAnalysisSummary,
  });
}
