import { useQuery } from '@tanstack/react-query';
import { callMcpHostTool } from '@/lib/api/mcp-host';
import { useCurrentCase } from '@/features/case/hooks';
import type { JobSnapshot } from '@/types/jobs';
import type { McpHostMetrics } from '@/types/mcpHost';

async function readTool<T>(name: string): Promise<T> {
  const result = await callMcpHostTool(name, {});
  if (!result.success || result.data === undefined) {
    throw new Error(result.error ?? 'MCP request failed');
  }
  return result.data as T;
}

export function useMcpActivityModel(enabled: boolean) {
  const currentCase = useCurrentCase();
  const caseId = currentCase.data?.id;
  const available = enabled && currentCase.isSuccess && Boolean(caseId);
  const metrics = useQuery({
    queryKey: ['mcp-host', 'activity', caseId ?? null, 'metrics'],
    queryFn: () => readTool<McpHostMetrics>('forensics.get_mcp_metrics'),
    enabled: available,
    retry: false,
  });
  const jobs = useQuery({
    queryKey: ['mcp-host', 'activity', caseId ?? null, 'jobs'],
    queryFn: () => readTool<JobSnapshot[]>('forensics.list_jobs'),
    enabled: available,
    retry: false,
  });
  return {
    available,
    metrics: available ? metrics.data : undefined,
    jobs: available ? jobs.data : undefined,
    error: available && (metrics.isError || jobs.isError),
    loading: metrics.isFetching || jobs.isFetching,
    refresh: () => { if (available) { void metrics.refetch(); void jobs.refetch(); } },
  };
}
