import { useInfiniteQuery, useMutation, useQuery, useQueryClient, type QueryClient } from '@tanstack/react-query';
import {
  generateAnalysisSummary,
  getBrowserHistorySummary,
  getCorrelationSnapshot,
  getEmailExtractionSummary,
  getEvtxEventSummary,
  getEvidenceClassificationSummary,
  getFileClassificationBoard,
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
import { AnalysisExtractionPageRequest, AnalysisExtractionRequest } from '@/types/models';
import type { DataSourceSummary } from '@/types/models';
import type { EvtxEventSummary, EvtxEventView } from '@/types/models';

type AnalysisSource = Pick<DataSourceSummary, 'id' | 'platform'>;
type OptionalAnalysisPageRequest = Omit<Partial<AnalysisExtractionPageRequest>, 'dataSourceId'> & {
  source?: AnalysisSource;
};

const ANALYSIS_QUERY_OPTIONS = {
  staleTime: Infinity,
  // Keep analysis projections cached indefinitely: they are expensive to
  // rebuild from evidence and must survive long idle periods without the
  // UI dropping into a loading state on the next render.
  gcTime: Infinity,
  refetchOnMount: false,
  refetchOnWindowFocus: false,
  refetchOnReconnect: false,
} as const;

const EVTX_PAGE_SIZE = 500;

function mergeEvtxPages(pages: EvtxEventSummary[]): EvtxEventSummary | undefined {
  const first = pages[0];
  if (!first) return undefined;
  return {
    ...first,
    bootEvents: pages.flatMap((page) => page.bootEvents),
    securityEvents: pages.flatMap((page) => page.securityEvents),
    applicationEvents: pages.flatMap((page) => page.applicationEvents),
    warnings: [...new Set(pages.flatMap((page) => page.warnings))],
  };
}

const ALL_EXTRACTION_QUERY_FAMILIES = [
  'evidence-classification',
  'system-info',
  'registry-extraction',
  'registry-structured',
  'browser-history',
  'email-extraction',
  'evtx-events',
  'linux-artifacts',
] as const;

const EXTRACTION_QUERY_FAMILIES: Readonly<Record<string, readonly string[]>> = {
  SystemInformation: ['evidence-classification', 'system-info'],
  Registry: [
    'evidence-classification',
    'system-info',
    'registry-extraction',
    'registry-structured',
  ],
  BrowserHistory: ['evidence-classification', 'browser-history'],
  Email: ['evidence-classification', 'email-extraction'],
  EventLogs: ['evidence-classification', 'evtx-events'],
  LinuxArtifacts: ['linux-artifacts'],
  LinuxJournal: ['linux-artifacts'],
  LinuxLogin: ['linux-artifacts'],
  LinuxCommands: ['linux-artifacts'],
  LinuxPackages: ['linux-artifacts'],
  LinuxCron: ['linux-artifacts'],
  LinuxSudo: ['linux-artifacts'],
  LinuxSystemConfig: ['linux-artifacts'],
  LinuxWebServices: ['linux-artifacts'],
  LinuxMysqlServices: ['linux-artifacts'],
};

function extractionQueryFamilies(categories: readonly string[]): readonly string[] {
  if (categories.length === 0) {
    return ALL_EXTRACTION_QUERY_FAMILIES;
  }

  return [...new Set(categories.flatMap((category) => EXTRACTION_QUERY_FAMILIES[category] ?? []))];
}

async function refreshExtractionQueries(
  queryClient: QueryClient,
  caseId: string | null,
  request: AnalysisExtractionRequest,
): Promise<void> {
  const sourceQueries = extractionQueryFamilies(request.categories).map((family) =>
    queryClient.invalidateQueries({
      queryKey: ['analysis', family, caseId, request.dataSourceId],
      refetchType: 'active',
    }));

  await Promise.all([
    ...sourceQueries,
    queryClient.invalidateQueries({ queryKey: ['artifacts'], refetchType: 'active' }),
    queryClient.invalidateQueries({ queryKey: ['timeline'], refetchType: 'active' }),
    queryClient.invalidateQueries({
      queryKey: ['graph', 'snapshot', caseId],
      refetchType: 'active',
    }),
  ]);
}

export function useAnalysisSystemInfo(source?: AnalysisSource) {
  const currentCase = useCurrentCase();
  const dataSourceId = source?.id;
  return useQuery({
    queryKey: ['analysis', 'system-info', currentCase.data?.id ?? null, dataSourceId ?? null, source?.platform ?? null],
    queryFn: () => getSystemInfo(dataSourceId ?? ''),
    enabled: currentCase.isSuccess
      && Boolean(currentCase.data)
      && Boolean(dataSourceId)
      && source?.platform === 'windows',
    retry: false,
    ...ANALYSIS_QUERY_OPTIONS,
  });
}

export function useFileClassificationBoard(source?: AnalysisSource, magicLimit = 300) {
  const currentCase = useCurrentCase();
  const dataSourceId = source?.id;
  return useQuery({
    queryKey: ['analysis', 'classification-board', currentCase.data?.id ?? null, dataSourceId ?? null, source?.platform ?? null, magicLimit],
    queryFn: () => getFileClassificationBoard(dataSourceId ?? '', magicLimit),
    enabled: currentCase.isSuccess
      && Boolean(currentCase.data)
      && Boolean(dataSourceId)
      && source?.platform === 'windows',
    retry: false,
    ...ANALYSIS_QUERY_OPTIONS,
  });
}

export function useEvidenceClassificationSummary(source?: AnalysisSource) {
  const currentCase = useCurrentCase();
  const dataSourceId = source?.id;
  return useQuery({
    queryKey: ['analysis', 'evidence-classification', currentCase.data?.id ?? null, dataSourceId ?? null, source?.platform ?? null],
    queryFn: () => getEvidenceClassificationSummary(dataSourceId ?? ''),
    enabled: currentCase.isSuccess
      && Boolean(currentCase.data)
      && Boolean(dataSourceId)
      && source?.platform === 'windows',
    retry: false,
    ...ANALYSIS_QUERY_OPTIONS,
  });
}

export function useRegistryExtractionSummary(request: OptionalAnalysisPageRequest = {}) {
  const currentCase = useCurrentCase();
  const dataSourceId = request.source?.id;
  const offset = request.offset ?? 0;
  const limit = request.limit ?? 200;
  return useQuery({
    queryKey: ['analysis', 'registry-extraction', currentCase.data?.id ?? null, dataSourceId ?? null, request.source?.platform ?? null, offset, limit],
    queryFn: () => getRegistryExtractionSummary({ dataSourceId: dataSourceId ?? '', offset, limit }),
    enabled: currentCase.isSuccess
      && Boolean(currentCase.data)
      && Boolean(dataSourceId)
      && request.source?.platform === 'windows',
    retry: false,
    ...ANALYSIS_QUERY_OPTIONS,
  });
}

export function useRegistryStructuredSummary(source?: AnalysisSource) {
  const currentCase = useCurrentCase();
  const dataSourceId = source?.id;
  return useQuery({
    queryKey: ['analysis', 'registry-structured', currentCase.data?.id ?? null, dataSourceId ?? null, source?.platform ?? null],
    queryFn: () => getRegistryStructuredSummary(dataSourceId ?? ''),
    enabled: currentCase.isSuccess
      && Boolean(currentCase.data)
      && Boolean(dataSourceId)
      && source?.platform === 'windows',
    retry: false,
    ...ANALYSIS_QUERY_OPTIONS,
  });
}

export function useBrowserHistorySummary(request: OptionalAnalysisPageRequest = {}) {
  const currentCase = useCurrentCase();
  const dataSourceId = request.source?.id;
  const offset = request.offset ?? 0;
  const limit = request.limit ?? 200;
  return useQuery({
    queryKey: ['analysis', 'browser-history', currentCase.data?.id ?? null, dataSourceId ?? null, request.source?.platform ?? null, offset, limit],
    queryFn: () => getBrowserHistorySummary({ dataSourceId: dataSourceId ?? '', offset, limit }),
    enabled: currentCase.isSuccess
      && Boolean(currentCase.data)
      && Boolean(dataSourceId)
      && request.source?.platform === 'windows',
    retry: false,
    ...ANALYSIS_QUERY_OPTIONS,
  });
}

export function useEmailExtractionSummary(request: OptionalAnalysisPageRequest = {}) {
  const currentCase = useCurrentCase();
  const dataSourceId = request.source?.id;
  const offset = request.offset ?? 0;
  const limit = request.limit ?? 200;
  return useQuery({
    queryKey: ['analysis', 'email-extraction', currentCase.data?.id ?? null, dataSourceId ?? null, request.source?.platform ?? null, offset, limit],
    queryFn: () => getEmailExtractionSummary({ dataSourceId: dataSourceId ?? '', offset, limit }),
    enabled: currentCase.isSuccess
      && Boolean(currentCase.data)
      && Boolean(dataSourceId)
      && request.source?.platform === 'windows',
    retry: false,
    ...ANALYSIS_QUERY_OPTIONS,
  });
}

export function useEvtxEventSummary(
  request: OptionalAnalysisPageRequest & { view?: EvtxEventView } = {},
) {
  const currentCase = useCurrentCase();
  const dataSourceId = request.source?.id;
  const view = request.view ?? 'boot';
  const limit = Math.min(request.limit ?? EVTX_PAGE_SIZE, EVTX_PAGE_SIZE);
  const query = useInfiniteQuery({
    queryKey: ['analysis', 'evtx-events', currentCase.data?.id ?? null, dataSourceId ?? null, request.source?.platform ?? null, view, limit],
    queryFn: ({ pageParam }) => getEvtxEventSummary({
      dataSourceId: dataSourceId ?? '',
      view,
      offset: pageParam,
      limit,
    }),
    initialPageParam: request.offset ?? 0,
    getNextPageParam: (lastPage, pages) => {
      const lastPageItemCount = lastPage.bootEvents.length
        + lastPage.securityEvents.length
        + lastPage.applicationEvents.length;
      if (lastPageItemCount === 0) return undefined;
      const loaded = pages.reduce(
        (total, page) => total
          + page.bootEvents.length
          + page.securityEvents.length
          + page.applicationEvents.length,
        0,
      );
      return loaded < lastPage.pageTotal ? (request.offset ?? 0) + loaded : undefined;
    },
    enabled: currentCase.isSuccess
      && Boolean(currentCase.data)
      && Boolean(dataSourceId)
      && request.source?.platform === 'windows',
    retry: false,
    ...ANALYSIS_QUERY_OPTIONS,
  });
  return {
    ...query,
    data: mergeEvtxPages(query.data?.pages ?? []),
  };
}

export function useLinuxArtifactSummary(request: OptionalAnalysisPageRequest = {}) {
  const currentCase = useCurrentCase();
  const dataSourceId = request.source?.id;
  const offset = request.offset ?? 0;
  const limit = request.limit ?? 200;
  return useQuery({
    queryKey: ['analysis', 'linux-artifacts', currentCase.data?.id ?? null, dataSourceId ?? null, request.source?.platform ?? null, offset, limit],
    queryFn: () => getLinuxArtifactSummary({ dataSourceId: dataSourceId ?? '', offset, limit }),
    enabled: currentCase.isSuccess
      && Boolean(currentCase.data)
      && Boolean(dataSourceId)
      && request.source?.platform === 'linux',
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
    onSuccess: async (_data, variables) => {
      await Promise.all([
        qc.invalidateQueries({
          queryKey: ['analysis', 'evidence-classification', currentCase.data?.id ?? null, variables.dataSourceId],
          refetchType: 'active',
        }),
        qc.invalidateQueries({ queryKey: ['artifacts'], refetchType: 'active' }),
      ]);
    },
  });
}

export function useRunAnalysisExtraction() {
  const qc = useQueryClient();
  const currentCase = useCurrentCase();
  return useMutation({
    mutationFn: (request: AnalysisExtractionRequest) => runAnalysisExtraction(request),
    onSuccess: (_data, variables) => refreshExtractionQueries(
      qc,
      currentCase.data?.id ?? null,
      variables,
    ),
  });
}

export function useGenerateAnalysisSummary(dataSourceId?: string) {
  return useMutation({
    mutationFn: () => generateAnalysisSummary(dataSourceId ?? ''),
  });
}
