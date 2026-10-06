import { McpSection } from '@/features/mcp/components/McpSection';
import { McpResourceListContainer } from '@/features/mcp/containers/McpResourceListContainer';
import { McpToolListContainer } from '@/features/mcp/containers/McpToolListContainer';
import { McpHostToolsPanel } from '@/features/mcp/components/McpHostToolsPanel';
import { useMcpStore } from '@/stores/mcp-store';
import { useMemo } from 'react';

export function McpSectionContainer() {
  const servers = useMcpStore((state) => state.servers);
  const selectedServerId = useMcpStore((state) => state.selectedServerId);
  const loading = useMcpStore((state) => state.loading);
  const error = useMcpStore((state) => state.error);
  const addServer = useMcpStore((state) => state.addServer);
  const removeServer = useMcpStore((state) => state.removeServer);
  const connectServer = useMcpStore((state) => state.connectServer);
  const disconnectServer = useMcpStore((state) => state.disconnectServer);
  const testConnection = useMcpStore((state) => state.testConnection);
  const selectServer = useMcpStore((state) => state.selectServer);
  const selectedServer = useMemo(() => servers.find((server) => server.id === selectedServerId), [servers, selectedServerId]);

  return (
    <McpSection
      servers={servers}
      selectedServerId={selectedServerId}
      loading={loading}
      error={error}
      onAdd={addServer}
      onConnect={(serverId) => void connectServer(serverId)}
      onDisconnect={(serverId) => void disconnectServer(serverId)}
      onRemove={(serverId) => void removeServer(serverId)}
      onSelect={selectServer}
      testConnection={testConnection}
      hostToolList={<McpHostToolsPanel />}
      resourceList={selectedServer ? <McpResourceListContainer serverId={selectedServer.id} /> : undefined}
      toolList={selectedServer ? <McpToolListContainer serverId={selectedServer.id} /> : undefined}
    />
  );
}
