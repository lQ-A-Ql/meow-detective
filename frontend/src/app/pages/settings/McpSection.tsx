import { useState } from 'react';
import { Bot, ChevronDown, ChevronRight, Plus } from 'lucide-react';
import { useMcpStore } from '@/stores/mcp-store';
import { McpServerItem } from '@/components/mcp/McpServerItem';
import { McpServerDialog } from '@/components/mcp/McpServerDialog';
import { McpResourceList } from '@/components/mcp/McpResourceList';
import { McpToolList } from '@/components/mcp/McpToolList';

export function McpSection() {
  const [expanded, setExpanded] = useState(false);
  const [showAddDialog, setShowAddDialog] = useState(false);
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

  const selectedServer = servers.find((s) => s.id === selectedServerId);

  return (
    <section>
      <div
        className="flex items-center gap-2 mb-3 cursor-pointer select-none"
        onClick={() => setExpanded(!expanded)}
      >
        <Bot size={14} className="text-[#888]" />
        <span className="text-[13px] font-semibold text-[#333]">AI 助手 (MCP)</span>
        {expanded ? (
          <ChevronDown size={14} className="text-[#888]" />
        ) : (
          <ChevronRight size={14} className="text-[#888]" />
        )}
        {loading && (
          <span className="text-[10px] text-blue-500">加载中...</span>
        )}
      </div>

      {expanded && (
        <div className="space-y-4">
          <div className="bg-[#f8f8f8] border border-[#e0e0e0] p-3">
            <div className="text-[11px] font-semibold text-[#666] mb-2">
              MCP 服务器连接
            </div>
            <div className="space-y-1">
              {servers.length === 0 ? (
                <div className="text-[11px] text-gray-500 py-2">暂无服务器</div>
              ) : (
                servers.map((server) => (
                  <McpServerItem
                    key={server.id}
                    server={server}
                    isSelected={server.id === selectedServerId}
                    onConnect={() => connectServer(server.id)}
                    onDisconnect={() => disconnectServer(server.id)}
                    onRemove={() => removeServer(server.id)}
                    onSelect={() => selectServer(server.id)}
                  />
                ))
              )}
            </div>
            <button
              onClick={() => setShowAddDialog(true)}
              className="mt-2 flex items-center gap-1 text-[11px] text-blue-600 hover:text-blue-800 transition-colors"
            >
              <Plus size={12} />
              添加服务器
            </button>
          </div>

          {selectedServer && (
            <div className="grid grid-cols-2 gap-4">
              <McpResourceList serverId={selectedServer.id} />
              <McpToolList serverId={selectedServer.id} />
            </div>
          )}

          <div className="text-[11px] text-[#666]">
            连接状态:{' '}
            <span className="font-medium">
              {servers.filter((s) => s.connected).length}
            </span>{' '}
            个服务器已连接
          </div>

          {error && (
            <div className="p-3 rounded text-[12px] bg-red-50 text-red-700 border border-red-200">
              {error}
            </div>
          )}
        </div>
      )}

      {showAddDialog && (
        <McpServerDialog
          onClose={() => setShowAddDialog(false)}
          onAdd={addServer}
          testConnection={testConnection}
        />
      )}
    </section>
  );
}
