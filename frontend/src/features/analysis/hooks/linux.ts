import { useInfiniteQuery } from '@tanstack/react-query';
import { getLinuxArtifactSummary } from '@/lib/api/analysis';
import { useCurrentCase } from '@/features/case/hooks';
import type { LinuxArtifactSummary } from '@/types/models';
import { ANALYSIS_QUERY_OPTIONS } from '../query-options';
import type { OptionalAnalysisPageRequest } from './types';

const LINUX_PAGE_SIZE = 200;

const LINUX_ENTRY_FAMILIES: ReadonlyArray<{
  entries: (page: LinuxArtifactSummary) => readonly unknown[];
  count: (page: LinuxArtifactSummary) => number;
}> = [
  { entries: (page) => page.journalEntries, count: (page) => page.journalCount + page.textLogCount },
  { entries: (page) => page.loginRecords, count: (page) => page.loginCount },
  { entries: (page) => page.bashCommands, count: (page) => page.bashCommandCount },
  { entries: (page) => page.aptEvents, count: (page) => page.aptEventCount },
  { entries: (page) => page.cronJobs, count: (page) => page.cronJobCount },
  { entries: (page) => page.sudoEvents, count: (page) => page.sudoEventCount },
  { entries: (page) => page.systemConfigs, count: (page) => page.systemConfigCount },
  { entries: (page) => page.webSites, count: (page) => page.webSiteCount },
  { entries: (page) => page.webAccessLogs, count: (page) => page.webAccessLogCount },
  { entries: (page) => page.webErrorLogs, count: (page) => page.webErrorLogCount },
  { entries: (page) => page.webFindings, count: (page) => page.webFindingCount },
  { entries: (page) => page.mysqlConfigs, count: (page) => page.mysqlConfigCount },
  { entries: (page) => page.mysqlLogs, count: (page) => page.mysqlLogCount },
  { entries: (page) => page.mysqlFindings, count: (page) => page.mysqlFindingCount },
];

function mergeLinuxPages(pages: LinuxArtifactSummary[]): LinuxArtifactSummary | undefined {
  const first = pages[0];
  if (!first) return undefined;
  return {
    ...first,
    journalEntries: pages.flatMap((page) => page.journalEntries),
    loginRecords: pages.flatMap((page) => page.loginRecords),
    bashCommands: pages.flatMap((page) => page.bashCommands),
    aptEvents: pages.flatMap((page) => page.aptEvents),
    cronJobs: pages.flatMap((page) => page.cronJobs),
    sudoEvents: pages.flatMap((page) => page.sudoEvents),
    systemConfigs: pages.flatMap((page) => page.systemConfigs),
    webSites: pages.flatMap((page) => page.webSites),
    webAccessLogs: pages.flatMap((page) => page.webAccessLogs),
    webErrorLogs: pages.flatMap((page) => page.webErrorLogs),
    webFindings: pages.flatMap((page) => page.webFindings),
    mysqlConfigs: pages.flatMap((page) => page.mysqlConfigs),
    mysqlLogs: pages.flatMap((page) => page.mysqlLogs),
    mysqlFindings: pages.flatMap((page) => page.mysqlFindings),
    warnings: [...new Set(pages.flatMap((page) => page.warnings))],
    coverageNotes: [...new Set(pages.flatMap((page) => page.coverageNotes ?? []))],
  };
}

function hasUnloadedEntries(pages: LinuxArtifactSummary[]): boolean {
  const last = pages[pages.length - 1];
  if (!last) return false;
  return LINUX_ENTRY_FAMILIES.some((family) => {
    const loaded = pages.reduce((total, page) => total + family.entries(page).length, 0);
    return loaded < family.count(last);
  });
}

export function useLinuxArtifactSummary(request: OptionalAnalysisPageRequest = {}) {
  const currentCase = useCurrentCase();
  const dataSourceId = request.source?.id;
  const limit = Math.min(request.limit ?? LINUX_PAGE_SIZE, LINUX_PAGE_SIZE);
  const query = useInfiniteQuery({
    queryKey: [
      'analysis',
      'linux-artifacts',
      currentCase.data?.id ?? null,
      dataSourceId ?? null,
      request.source?.platform ?? null,
      limit,
    ],
    queryFn: ({ pageParam }) => getLinuxArtifactSummary({
      dataSourceId: dataSourceId ?? '',
      offset: pageParam,
      limit,
    }),
    initialPageParam: request.offset ?? 0,
    getNextPageParam: (lastPage, pages) => {
      const lastPageItemCount = LINUX_ENTRY_FAMILIES.reduce(
        (total, family) => total + family.entries(lastPage).length,
        0,
      );
      if (lastPageItemCount === 0) return undefined;
      return hasUnloadedEntries(pages)
        ? (request.offset ?? 0) + pages.length * limit
        : undefined;
    },
    enabled: currentCase.isSuccess
      && Boolean(currentCase.data)
      && Boolean(dataSourceId)
      && request.source?.platform === 'linux',
    retry: false,
    ...ANALYSIS_QUERY_OPTIONS,
  });
  return {
    ...query,
    data: mergeLinuxPages(query.data?.pages ?? []),
  };
}
