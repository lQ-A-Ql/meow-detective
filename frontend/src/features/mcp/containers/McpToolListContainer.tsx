import { useEffect } from 'react';
import { McpToolList } from '@/features/mcp/components/McpToolList';
import { useMcpStore } from '@/stores/mcp-store';

export function McpToolListContainer({ serverId }: { serverId: string }) {
  const tools = useMcpStore((state) => state.toolsByServer[serverId] ?? []);
  const loading = useMcpStore((state) => state.toolLoadingByServer[serverId] ?? false);
  const error = useMcpStore((state) => state.toolErrorsByServer[serverId] ?? null);
  const refreshTools = useMcpStore((state) => state.refreshTools);
  const callTool = useMcpStore((state) => state.callTool);

  useEffect(() => {
    void refreshTools(serverId);
  }, [refreshTools, serverId]);

  return (
    <McpToolList
      tools={tools}
      loading={loading}
      error={error}
      onRefresh={() => void refreshTools(serverId)}
      onTestTool={(toolName, args) => callTool(serverId, toolName, args)}
    />
  );
}
