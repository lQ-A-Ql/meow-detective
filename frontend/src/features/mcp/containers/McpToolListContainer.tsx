import { useEffect } from 'react';
import { McpToolList } from '@/features/mcp/components/McpToolList';
import { useMcpStore } from '@/stores/mcp-store';

export function McpToolListContainer({ serverId }: { serverId: string }) {
  const tools = useMcpStore((state) => state.toolsByServer[serverId] ?? []);
  const server = useMcpStore((state) => state.servers.find((item) => item.id === serverId));
  const loading = useMcpStore((state) => state.toolLoadingByServer[serverId] ?? false);
  const error = useMcpStore((state) => state.toolErrorsByServer[serverId] ?? null);
  const refreshTools = useMcpStore((state) => state.refreshTools);
  const callTool = useMcpStore((state) => state.callTool);
  const setToolDisabled = useMcpStore((state) => state.setToolDisabled);
  const setAllToolsDisabled = useMcpStore((state) => state.setAllToolsDisabled);
  const setToolAccessMode = useMcpStore((state) => state.setToolAccessMode);

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
      disabledTools={server?.permissions.deniedTools ?? []}
      onToggleTool={server?.permissions.toolAccess === 'allowAll' ? (toolName, disabled) => void setToolDisabled(serverId, toolName, disabled) : undefined}
      toolAccess={server?.permissions.toolAccess}
      onEnableAll={() => void setToolAccessMode(serverId, 'allowAll')}
      onToggleAll={server?.permissions.toolAccess === 'allowAll' ? (disabled) => void setAllToolsDisabled(serverId, tools.map((tool) => tool.name), disabled) : undefined}
    />
  );
}
