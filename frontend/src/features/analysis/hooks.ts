import { useMutation, useQuery } from '@tanstack/react-query';
import {
  classifyFiles,
  generateAnalysisSummary,
  getBrowserHistorySummary,
  getCorrelationSnapshot,
  getEmailExtractionSummary,
  getEvtxEventSummary,
  getEvidenceClassificationSummary,
  getRegistryExtractionSummary,
  getRegistryStructuredSummary,
  getSystemInfo,
  getV2GovernanceSnapshot,
  getV3GovernanceSnapshot,
  runAnalysisExtraction,
  runEvidenceClassification,
} from '@/lib/api/analysis';
import { useCurrentCase } from '@/features/case/hooks';
import { useQueryClient } from '@tanstack/react-query';
import { AnalysisExtractionPageRequest, AnalysisExtractionRequest } from '@/types/models';

const ANALYSIS_QUERY_OPTIONS = {
  staleTime: Infinity,
  gcTime: 30 * 60 * 1000,
  refetchOnMount: false,
  refetchOnWindowFocus: false,
  refetchOnReconnect: false,
} as const;

export function useAnalysisSystemInfo() {
  const currentCase = useCurrentCase();
  return useQuery({
    queryKey: ['analysis', 'system-info', currentCase.data?.id ?? null],
    queryFn: getSystemInfo,
    enabled: currentCase.isSuccess && Boolean(currentCase.data),
    retry: false,
    ...ANALYSIS_QUERY_OPTIONS,
  });
}

export function useAnalysisClassifications(sampleSize = 1000) {
  const currentCase = useCurrentCase();
  return useQuery({
    queryKey: ['analysis', 'classifications', currentCase.data?.id ?? null, sampleSize],
    queryFn: () => classifyFiles(sampleSize),
    enabled: currentCase.isSuccess && Boolean(currentCase.data),
    retry: false,
    ...ANALYSIS_QUERY_OPTIONS,
  });
}

export function useEvidenceClassificationSummary() {
  const currentCase = useCurrentCase();
  return useQuery({
    queryKey: ['analysis', 'evidence-classification', currentCase.data?.id ?? null],
    queryFn: getEvidenceClassificationSummary,
    enabled: currentCase.isSuccess && Boolean(currentCase.data),
    retry: false,
    ...ANALYSIS_QUERY_OPTIONS,
  });
}

export function useRegistryExtractionSummary(request: AnalysisExtractionPageRequest = {}) {
  const currentCase = useCurrentCase();
  const offset = request.offset ?? 0;
  const limit = request.limit ?? 200;
  return useQuery({
    queryKey: ['analysis', 'registry-extraction', currentCase.data?.id ?? null, offset, limit],
    queryFn: () => getRegistryExtractionSummary({ offset, limit }),
    enabled: currentCase.isSuccess && Boolean(currentCase.data),
    retry: false,
    ...ANALYSIS_QUERY_OPTIONS,
  });
}

export function useRegistryStructuredSummary() {
  const currentCase = useCurrentCase();
  return useQuery({
    queryKey: ['analysis', 'registry-structured', currentCase.data?.id ?? null],
    queryFn: getRegistryStructuredSummary,
    enabled: currentCase.isSuccess && Boolean(currentCase.data),
    retry: false,
    ...ANALYSIS_QUERY_OPTIONS,
  });
}

export function useBrowserHistorySummary(request: AnalysisExtractionPageRequest = {}) {
  const currentCase = useCurrentCase();
  const offset = request.offset ?? 0;
  const limit = request.limit ?? 200;
  return useQuery({
    queryKey: ['analysis', 'browser-history', currentCase.data?.id ?? null, offset, limit],
    queryFn: () => getBrowserHistorySummary({ offset, limit }),
    enabled: currentCase.isSuccess && Boolean(currentCase.data),
    retry: false,
    ...ANALYSIS_QUERY_OPTIONS,
  });
}

export function useEmailExtractionSummary(request: AnalysisExtractionPageRequest = {}) {
  const currentCase = useCurrentCase();
  const offset = request.offset ?? 0;
  const limit = request.limit ?? 200;
  return useQuery({
    queryKey: ['analysis', 'email-extraction', currentCase.data?.id ?? null, offset, limit],
    queryFn: () => getEmailExtractionSummary({ offset, limit }),
    enabled: currentCase.isSuccess && Boolean(currentCase.data),
    retry: false,
    ...ANALYSIS_QUERY_OPTIONS,
  });
}

export function useEvtxEventSummary(request: AnalysisExtractionPageRequest = {}) {
  const currentCase = useCurrentCase();
  const offset = request.offset ?? 0;
  const limit = request.limit ?? 200;
  return useQuery({
    queryKey: ['analysis', 'evtx-events', currentCase.data?.id ?? null, offset, limit],
    queryFn: () => getEvtxEventSummary({ offset, limit }),
    enabled: currentCase.isSuccess && Boolean(currentCase.data),
    retry: false,
    ...ANALYSIS_QUERY_OPTIONS,
  });
}

export function useV2GovernanceSnapshot() {
  const currentCase = useCurrentCase();
  return useQuery({
    queryKey: ['analysis', 'v2-governance', currentCase.data?.id ?? null],
    queryFn: getV2GovernanceSnapshot,
    enabled: currentCase.isSuccess && Boolean(currentCase.data),
    retry: false,
    ...ANALYSIS_QUERY_OPTIONS,
  });
}

export function useCorrelationSnapshot() {
  const currentCase = useCurrentCase();
  return useQuery({
    queryKey: ['analysis', 'correlation', currentCase.data?.id ?? null],
    queryFn: getCorrelationSnapshot,
    enabled: currentCase.isSuccess && Boolean(currentCase.data),
    retry: false,
    ...ANALYSIS_QUERY_OPTIONS,
  });
}

export function useV3GovernanceSnapshot() {
  const currentCase = useCurrentCase();
  return useQuery({
    queryKey: ['analysis', 'v3-governance', currentCase.data?.id ?? null],
    queryFn: getV3GovernanceSnapshot,
    enabled: currentCase.isSuccess && Boolean(currentCase.data),
    retry: false,
    ...ANALYSIS_QUERY_OPTIONS,
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

export function useRunAnalysisExtraction() {
  const qc = useQueryClient();
  const currentCase = useCurrentCase();
  return useMutation({
    mutationFn: (request?: AnalysisExtractionRequest) => runAnalysisExtraction(request ?? { categories: [] }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['analysis', 'evidence-classification', currentCase.data?.id ?? null] });
      qc.invalidateQueries({ queryKey: ['analysis', 'registry-extraction', currentCase.data?.id ?? null] });
      qc.invalidateQueries({ queryKey: ['analysis', 'registry-structured', currentCase.data?.id ?? null] });
      qc.invalidateQueries({ queryKey: ['analysis', 'browser-history', currentCase.data?.id ?? null] });
      qc.invalidateQueries({ queryKey: ['analysis', 'email-extraction', currentCase.data?.id ?? null] });
      qc.invalidateQueries({ queryKey: ['analysis', 'evtx-events', currentCase.data?.id ?? null] });
      qc.invalidateQueries({ queryKey: ['artifacts'] });
      qc.invalidateQueries({ queryKey: ['timeline'] });
    },
  });
}

export function useGenerateAnalysisSummary() {
  return useMutation({
    mutationFn: generateAnalysisSummary,
  });
}
