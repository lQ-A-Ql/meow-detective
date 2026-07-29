import { useEffect } from 'react';
import { McpToolList } from '@/features/mcp/components/McpToolList';
import { useMcpStore } from '@/stores/mcp-store';

export function McpToolListContainer({ serverId }: { serverId: string }) {
  const { tools, loading, refreshTools, callTool } = useMcpStore();

  useEffect(() => {
    void refreshTools(serverId);
  }, [refreshTools, serverId]);

  return (
    <McpToolList
      tools={tools}
      loading={loading}
      onRefresh={() => void refreshTools(serverId)}
      onTestTool={(toolName) => callTool(serverId, toolName, {})}
    />
  );
}
