import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  getAndroidDeviceInfo,
  getAndroidPackageSummary,
  runAndroidAnalysis,
} from '@/lib/api/analysis';
import { useCurrentCase } from '@/features/case/hooks';
import type { AnalysisExtractionPageRequest, AndroidPackageSummary, DataSourceSummary } from '@/types/models';
import { ANALYSIS_QUERY_OPTIONS, ANDROID_PACKAGE_PAGE_SIZE } from '../query-options';

type AnalysisSource = Pick<DataSourceSummary, 'id' | 'platform'>;
type OptionalAnalysisPageRequest = Omit<Partial<AnalysisExtractionPageRequest>, 'dataSourceId'> & {
  source?: AnalysisSource;
};

function mergeAndroidPackagePages(
  pages: AndroidPackageSummary[],
): AndroidPackageSummary | undefined {
  const first = pages[0];
  if (!first) return undefined;
  return {
    ...first,
    packages: pages.flatMap((page) => page.packages),
    warnings: [...new Set(pages.flatMap((page) => page.warnings))],
  };
}

export function useAndroidDeviceInfo(source?: AnalysisSource) {
  const currentCase = useCurrentCase();
  const dataSourceId = source?.id;
  return useQuery({
    queryKey: ['analysis', 'android-device', currentCase.data?.id ?? null, dataSourceId ?? null],
    queryFn: () => getAndroidDeviceInfo(dataSourceId ?? ''),
    enabled: currentCase.isSuccess
      && Boolean(currentCase.data)
      && Boolean(dataSourceId)
      && source?.platform === 'android',
    retry: false,
    ...ANALYSIS_QUERY_OPTIONS,
  });
}

export function useAndroidPackageSummary(request: OptionalAnalysisPageRequest = {}) {
  const currentCase = useCurrentCase();
  const dataSourceId = request.source?.id;
  const limit = Math.min(request.limit ?? ANDROID_PACKAGE_PAGE_SIZE, ANDROID_PACKAGE_PAGE_SIZE);
  const query = useInfiniteQuery({
    queryKey: ['analysis', 'android-packages', currentCase.data?.id ?? null, dataSourceId ?? null, limit],
    queryFn: ({ pageParam }) => getAndroidPackageSummary({
      dataSourceId: dataSourceId ?? '',
      offset: pageParam,
      limit,
    }),
    initialPageParam: request.offset ?? 0,
    getNextPageParam: (lastPage, pages) => {
      if (lastPage.packages.length === 0) return undefined;
      const loaded = pages.reduce((total, page) => total + page.packages.length, 0);
      return loaded < lastPage.totalCount ? (request.offset ?? 0) + loaded : undefined;
    },
    enabled: currentCase.isSuccess
      && Boolean(currentCase.data)
      && Boolean(dataSourceId)
      && request.source?.platform === 'android',
    retry: false,
    ...ANALYSIS_QUERY_OPTIONS,
  });
  return {
    ...query,
    data: mergeAndroidPackagePages(query.data?.pages ?? []),
  };
}

export function useRunAndroidAnalysis() {
  const queryClient = useQueryClient();
  const currentCase = useCurrentCase();
  return useMutation({
    mutationFn: (dataSourceId: string) => runAndroidAnalysis(dataSourceId),
    onSuccess: async (_data, dataSourceId) => {
      await Promise.all([
        queryClient.invalidateQueries({
          queryKey: ['analysis', 'android-device', currentCase.data?.id ?? null, dataSourceId],
          refetchType: 'active',
        }),
        queryClient.invalidateQueries({
          queryKey: ['analysis', 'android-packages', currentCase.data?.id ?? null, dataSourceId],
          refetchType: 'active',
        }),
      ]);
    },
  });
}
