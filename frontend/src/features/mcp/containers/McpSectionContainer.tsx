import { McpSection } from '@/features/mcp/components/McpSection';
import { McpResourceListContainer } from '@/features/mcp/containers/McpResourceListContainer';
import { McpToolListContainer } from '@/features/mcp/containers/McpToolListContainer';
import { useMcpStore } from '@/stores/mcp-store';

export function McpSectionContainer() {
  const {
    servers,
    selectedServerId,
    loading,
    error,
    addServer,
    removeServer,
    connectServer,
    disconnectServer,
    testConnection,
    selectServer,
  } = useMcpStore();
  const selectedServer = servers.find((server) => server.id === selectedServerId);

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
      resourceList={selectedServer ? <McpResourceListContainer serverId={selectedServer.id} /> : undefined}
      toolList={selectedServer ? <McpToolListContainer serverId={selectedServer.id} /> : undefined}
    />
  );
}
