import { useEffect } from 'react';
import { McpResourceList } from '@/features/mcp/components/McpResourceList';
import { useMcpStore } from '@/stores/mcp-store';
import type { McpResource } from '@/lib/api/mcp';

const emptyResources: McpResource[] = [];

export function McpResourceListContainer({ serverId }: { serverId: string }) {
  const resources = useMcpStore((state) => state.resourcesByServer[serverId] ?? emptyResources);
  const loading = useMcpStore((state) => state.resourceLoadingByServer[serverId] ?? false);
  const error = useMcpStore((state) => state.resourceErrorsByServer[serverId] ?? null);
  const refreshResources = useMcpStore((state) => state.refreshResources);

  useEffect(() => {
    void refreshResources(serverId);
  }, [refreshResources, serverId]);

  return (
    <McpResourceList
      resources={resources}
      loading={loading}
      error={error}
      onRefresh={() => void refreshResources(serverId)}
    />
  );
}
