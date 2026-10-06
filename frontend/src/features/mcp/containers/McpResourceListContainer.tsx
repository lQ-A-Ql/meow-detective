import { useEffect } from 'react';
import { McpResourceList } from '@/features/mcp/components/McpResourceList';
import { useMcpStore } from '@/stores/mcp-store';

export function McpResourceListContainer({ serverId }: { serverId: string }) {
  const resources = useMcpStore((state) => state.resourcesByServer[serverId] ?? []);
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
