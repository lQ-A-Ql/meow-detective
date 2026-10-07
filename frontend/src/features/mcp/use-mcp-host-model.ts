import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { getMcpHostStatus, setMcpHostSettings, listMcpHostTools, callMcpHostTool } from '@/lib/api/mcp-host';
import type { McpHostSettings } from '@/types/models';

const statusKey = ['mcp-host', 'status'] as const;

export function useMcpHostModel() {
  const client = useQueryClient();
  const status = useQuery({ queryKey: statusKey, queryFn: getMcpHostStatus, retry: false, refetchInterval: 3000 });
  const tools = useQuery({ queryKey: ['mcp-host', 'tools'], queryFn: listMcpHostTools, retry: false, staleTime: Infinity });
  const settings = useMutation({
    mutationFn: setMcpHostSettings,
    onSuccess: (result) => client.setQueryData(statusKey, result),
    onSettled: () => { void client.invalidateQueries({ queryKey: statusKey }); },
  });
  const save = async (value: McpHostSettings) => {
    try { await settings.mutateAsync(value); } catch { /* Error remains visible in the panel. */ }
  };
  return {
    status: status.data,
    tools: tools.data ?? [],
    loading: status.isLoading || tools.isLoading || settings.isPending,
    error: settings.error ?? status.error ?? tools.error,
    refresh: () => { void status.refetch(); void tools.refetch(); },
    save,
    callTool: callMcpHostTool,
  };
}
