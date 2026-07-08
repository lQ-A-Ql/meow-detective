import { useMutation, useQuery } from '@tanstack/react-query';
import {
  classifyFiles,
  generateAnalysisSummary,
  getBrowserHistorySummary,
  getCorrelationSnapshot,
  getEmailExtractionSummary,
  getEvtxEventSummary,
  getEvidenceClassificationSummary,
  getLinuxArtifactSummary,
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

type OptionalAnalysisPageRequest = Partial<AnalysisExtractionPageRequest>;

const ANALYSIS_QUERY_OPTIONS = {
  staleTime: Infinity,
  gcTime: 30 * 60 * 1000,
  refetchOnMount: false,
  refetchOnWindowFocus: false,
  refetchOnReconnect: false,
} as const;

export function useAnalysisSystemInfo(dataSourceId?: string) {
  const currentCase = useCurrentCase();
  return useQuery({
    queryKey: ['analysis', 'system-info', currentCase.data?.id ?? null, dataSourceId ?? null],
    queryFn: () => getSystemInfo(dataSourceId ?? ''),
    enabled: currentCase.isSuccess && Boolean(currentCase.data) && Boolean(dataSourceId),
    retry: false,
    ...ANALYSIS_QUERY_OPTIONS,
  });
}

export function useAnalysisClassifications(dataSourceId?: string, sampleSize = 1000) {
  const currentCase = useCurrentCase();
  return useQuery({
    queryKey: ['analysis', 'classifications', currentCase.data?.id ?? null, dataSourceId ?? null, sampleSize],
    queryFn: () => classifyFiles(dataSourceId ?? '', sampleSize),
    enabled: currentCase.isSuccess && Boolean(currentCase.data) && Boolean(dataSourceId),
    retry: false,
    ...ANALYSIS_QUERY_OPTIONS,
  });
}

export function useEvidenceClassificationSummary(dataSourceId?: string) {
  const currentCase = useCurrentCase();
  return useQuery({
    queryKey: ['analysis', 'evidence-classification', currentCase.data?.id ?? null, dataSourceId ?? null],
    queryFn: () => getEvidenceClassificationSummary(dataSourceId ?? ''),
    enabled: currentCase.isSuccess && Boolean(currentCase.data) && Boolean(dataSourceId),
    retry: false,
    ...ANALYSIS_QUERY_OPTIONS,
  });
}

export function useRegistryExtractionSummary(request: OptionalAnalysisPageRequest = {}) {
  const currentCase = useCurrentCase();
  const dataSourceId = request.dataSourceId;
  const offset = request.offset ?? 0;
  const limit = request.limit ?? 200;
  return useQuery({
    queryKey: ['analysis', 'registry-extraction', currentCase.data?.id ?? null, dataSourceId ?? null, offset, limit],
    queryFn: () => getRegistryExtractionSummary({ dataSourceId: dataSourceId ?? '', offset, limit }),
    enabled: currentCase.isSuccess && Boolean(currentCase.data) && Boolean(dataSourceId),
    retry: false,
    ...ANALYSIS_QUERY_OPTIONS,
  });
}

export function useRegistryStructuredSummary(dataSourceId?: string) {
  const currentCase = useCurrentCase();
  return useQuery({
    queryKey: ['analysis', 'registry-structured', currentCase.data?.id ?? null, dataSourceId ?? null],
    queryFn: () => getRegistryStructuredSummary(dataSourceId ?? ''),
    enabled: currentCase.isSuccess && Boolean(currentCase.data) && Boolean(dataSourceId),
    retry: false,
    ...ANALYSIS_QUERY_OPTIONS,
  });
}

export function useBrowserHistorySummary(request: OptionalAnalysisPageRequest = {}) {
  const currentCase = useCurrentCase();
  const dataSourceId = request.dataSourceId;
  const offset = request.offset ?? 0;
  const limit = request.limit ?? 200;
  return useQuery({
    queryKey: ['analysis', 'browser-history', currentCase.data?.id ?? null, dataSourceId ?? null, offset, limit],
    queryFn: () => getBrowserHistorySummary({ dataSourceId: dataSourceId ?? '', offset, limit }),
    enabled: currentCase.isSuccess && Boolean(currentCase.data) && Boolean(dataSourceId),
    retry: false,
    ...ANALYSIS_QUERY_OPTIONS,
  });
}

export function useEmailExtractionSummary(request: OptionalAnalysisPageRequest = {}) {
  const currentCase = useCurrentCase();
  const dataSourceId = request.dataSourceId;
  const offset = request.offset ?? 0;
  const limit = request.limit ?? 200;
  return useQuery({
    queryKey: ['analysis', 'email-extraction', currentCase.data?.id ?? null, dataSourceId ?? null, offset, limit],
    queryFn: () => getEmailExtractionSummary({ dataSourceId: dataSourceId ?? '', offset, limit }),
    enabled: currentCase.isSuccess && Boolean(currentCase.data) && Boolean(dataSourceId),
    retry: false,
    ...ANALYSIS_QUERY_OPTIONS,
  });
}

export function useEvtxEventSummary(request: OptionalAnalysisPageRequest = {}) {
  const currentCase = useCurrentCase();
  const dataSourceId = request.dataSourceId;
  const offset = request.offset ?? 0;
  const limit = request.limit ?? 200;
  return useQuery({
    queryKey: ['analysis', 'evtx-events', currentCase.data?.id ?? null, dataSourceId ?? null, offset, limit],
    queryFn: () => getEvtxEventSummary({ dataSourceId: dataSourceId ?? '', offset, limit }),
    enabled: currentCase.isSuccess && Boolean(currentCase.data) && Boolean(dataSourceId),
    retry: false,
    ...ANALYSIS_QUERY_OPTIONS,
  });
}

export function useLinuxArtifactSummary(request: OptionalAnalysisPageRequest = {}) {
  const currentCase = useCurrentCase();
  const dataSourceId = request.dataSourceId;
  const offset = request.offset ?? 0;
  const limit = request.limit ?? 200;
  return useQuery({
    queryKey: ['analysis', 'linux-artifacts', currentCase.data?.id ?? null, dataSourceId ?? null, offset, limit],
    queryFn: () => getLinuxArtifactSummary({ dataSourceId: dataSourceId ?? '', offset, limit }),
    enabled: currentCase.isSuccess && Boolean(currentCase.data) && Boolean(dataSourceId),
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
    mutationFn: (request: { dataSourceId: string; categories?: string[] }) =>
      runEvidenceClassification(request.dataSourceId, request.categories ?? []),
    onSuccess: (_data, variables) => {
      qc.invalidateQueries({
        queryKey: ['analysis', 'evidence-classification', currentCase.data?.id ?? null, variables.dataSourceId],
      });
      qc.invalidateQueries({ queryKey: ['artifacts'] });
    },
  });
}

export function useRunAnalysisExtraction() {
  const qc = useQueryClient();
  const currentCase = useCurrentCase();
  return useMutation({
    mutationFn: (request: AnalysisExtractionRequest) => runAnalysisExtraction(request),
    onSuccess: (_data, variables) => {
      qc.invalidateQueries({ queryKey: ['analysis', 'evidence-classification', currentCase.data?.id ?? null, variables.dataSourceId] });
      qc.invalidateQueries({ queryKey: ['analysis', 'system-info', currentCase.data?.id ?? null, variables.dataSourceId] });
      qc.invalidateQueries({ queryKey: ['analysis', 'registry-extraction', currentCase.data?.id ?? null, variables.dataSourceId] });
      qc.invalidateQueries({ queryKey: ['analysis', 'registry-structured', currentCase.data?.id ?? null, variables.dataSourceId] });
      qc.invalidateQueries({ queryKey: ['analysis', 'browser-history', currentCase.data?.id ?? null, variables.dataSourceId] });
      qc.invalidateQueries({ queryKey: ['analysis', 'email-extraction', currentCase.data?.id ?? null, variables.dataSourceId] });
      qc.invalidateQueries({ queryKey: ['analysis', 'evtx-events', currentCase.data?.id ?? null, variables.dataSourceId] });
      qc.invalidateQueries({ queryKey: ['analysis', 'linux-artifacts', currentCase.data?.id ?? null, variables.dataSourceId] });
      qc.invalidateQueries({ queryKey: ['artifacts'] });
      qc.invalidateQueries({ queryKey: ['timeline'] });
      qc.invalidateQueries({ queryKey: ['graph', 'snapshot', currentCase.data?.id ?? null] });
    },
  });
}

export function useGenerateAnalysisSummary(dataSourceId?: string) {
  return useMutation({
    mutationFn: () => generateAnalysisSummary(dataSourceId ?? ''),
  });
}
