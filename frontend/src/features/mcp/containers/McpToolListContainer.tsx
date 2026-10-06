import { useEffect } from 'react';
import { McpToolList } from '@/features/mcp/components/McpToolList';
import { useMcpStore } from '@/stores/mcp-store';

export function McpToolListContainer({ serverId }: { serverId: string }) {
  const tools = useMcpStore((state) => state.tools);
  const loading = useMcpStore((state) => state.loading);
  const refreshTools = useMcpStore((state) => state.refreshTools);
  const callTool = useMcpStore((state) => state.callTool);

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
