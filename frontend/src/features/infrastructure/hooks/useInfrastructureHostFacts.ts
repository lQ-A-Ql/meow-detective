import { useMemo } from 'react';
import { useQueries } from '@tanstack/react-query';
import { useCurrentCase, useDataSources } from '@/features/case/hooks';
import { getLinuxArtifactSummary } from '@/lib/api/analysis';
import { ANALYSIS_QUERY_OPTIONS } from '@/features/analysis/query-options';

export interface InfrastructureHostFact {
  hostname?: string;
  operatingSystem?: string;
  operatingSystemVersion?: string;
  kernelVersion?: string;
}

/** Reads existing Linux summary evidence for host labels and version facts. */
export function useInfrastructureHostFacts() {
  const currentCase = useCurrentCase();
  const dataSources = useDataSources();
  const linuxSources = useMemo(
    () => (dataSources.data ?? []).filter((source) => source.platform === 'linux' && source.importState === 'ready'),
    [dataSources.data],
  );
  const queries = useQueries({
    queries: linuxSources.map((source) => ({
      queryKey: ['analysis', 'linux-artifacts', currentCase.data?.id ?? null, source.id, 'linux', 1],
      queryFn: () => getLinuxArtifactSummary({ dataSourceId: source.id, offset: 0, limit: 1 }),
      enabled: Boolean(currentCase.data),
      retry: false,
      ...ANALYSIS_QUERY_OPTIONS,
    })),
  });
  return useMemo(() => new Map(linuxSources.map((source, index) => {
    const info = queries[index]?.data?.systemInfo;
    return [source.id, {
      hostname: info?.hostname,
      operatingSystem: info?.osPrettyName ?? info?.osId,
      operatingSystemVersion: info?.osVersionId,
      kernelVersion: info?.kernelVersions?.[0],
    } satisfies InfrastructureHostFact];
  })), [linuxSources, queries]);
}
