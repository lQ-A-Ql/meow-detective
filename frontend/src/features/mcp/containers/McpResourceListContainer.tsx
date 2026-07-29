import { useEffect } from 'react';
import { McpResourceList } from '@/features/mcp/components/McpResourceList';
import { useMcpStore } from '@/stores/mcp-store';

export function McpResourceListContainer({ serverId }: { serverId: string }) {
  const { resources, loading, refreshResources } = useMcpStore();

  useEffect(() => {
    void refreshResources(serverId);
  }, [refreshResources, serverId]);

  return (
    <McpResourceList
      resources={resources}
      loading={loading}
      onRefresh={() => void refreshResources(serverId)}
    />
  );
}
