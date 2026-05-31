import { useState, useEffect } from 'react';
import { HardDrive, FolderOpen, Bot, ChevronDown, ChevronRight, Plus } from 'lucide-react';
import { useMcpStore } from '@/stores/mcp-store';
import { McpServerItem } from '@/components/mcp/McpServerItem';
import { McpServerDialog } from '@/components/mcp/McpServerDialog';
import { McpResourceList } from '@/components/mcp/McpResourceList';
import { McpToolList } from '@/components/mcp/McpToolList';

export function Settings() {
  const [mcpExpanded, setMcpExpanded] = useState(false);
  const [showAddDialog, setShowAddDialog] = useState(false);

  const {
    servers,
    selectedServerId,
    loading,
    error,
    loadConfig,
    addServer,
    removeServer,
    connectServer,
    disconnectServer,
    testConnection,
    selectServer,
  } = useMcpStore();

  // Load MCP config on mount
  useEffect(() => {
    loadConfig();
  }, []);

  const selectedServer = servers.find((s) => s.id === selectedServerId);

  return (
    <div className="flex-1 flex flex-col w-full h-full bg-white overflow-auto">
      <div className="border-b border-[#e0e0e0] bg-[#fafafa] p-6 shrink-0">
        <div className="font-serif text-xl text-[#111] tracking-tight">设置</div>
        <div className="text-[#666] text-[11px] font-mono mt-1">应用配置与数据目录</div>
      </div>

      <div className="p-6 space-y-8">
        {/* 案件目录 */}
        <section>
          <div className="flex items-center gap-2 mb-3">
            <FolderOpen size={14} className="text-[#888]" />
            <span className="text-[13px] font-semibold text-[#333]">案件目录</span>
          </div>
          <div className="bg-[#f8f8f8] border border-[#e0e0e0] p-3 font-mono text-[12px] text-[#555]">
            C:\Cases
          </div>
          <div className="mt-1 text-[10px] text-[#999]">所有案件数据将存储在此目录下</div>
        </section>

        {/* 镜像搜索路径 */}
        <section>
          <div className="flex items-center gap-2 mb-3">
            <HardDrive size={14} className="text-[#888]" />
            <span className="text-[13px] font-semibold text-[#333]">镜像搜索路径</span>
          </div>
          <div className="bg-[#f8f8f8] border border-[#e0e0e0] p-3 font-mono text-[12px] text-[#555]">
            E:\cases\; D:\images\
          </div>
          <div className="mt-1 text-[10px] text-[#999]">导入数据源时自动搜索的镜像目录（分号分隔）</div>
        </section>

        {/* MCP 配置 */}
        <section>
          <div
            className="flex items-center gap-2 mb-3 cursor-pointer select-none"
            onClick={() => setMcpExpanded(!mcpExpanded)}
          >
            <Bot size={14} className="text-[#888]" />
            <span className="text-[13px] font-semibold text-[#333]">AI 助手 (MCP)</span>
            {mcpExpanded ? (
              <ChevronDown size={14} className="text-[#888]" />
            ) : (
              <ChevronRight size={14} className="text-[#888]" />
            )}
            {loading && (
              <span className="text-[10px] text-blue-500">加载中...</span>
            )}
          </div>

          {mcpExpanded && (
            <div className="space-y-4">
              {/* 服务器列表 */}
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

              {/* Resources 和 Tools */}
              {selectedServer && (
                <div className="grid grid-cols-2 gap-4">
                  <McpResourceList serverId={selectedServer.id} />
                  <McpToolList serverId={selectedServer.id} />
                </div>
              )}

              {/* 连接状态 */}
              <div className="text-[11px] text-[#666]">
                连接状态:{' '}
                <span className="font-medium">
                  {servers.filter((s) => s.connected).length}
                </span>{' '}
                个服务器已连接
              </div>

              {/* Error */}
              {error && (
                <div className="p-3 rounded text-[12px] bg-red-50 text-red-700 border border-red-200">
                  {error}
                </div>
              )}
            </div>
          )}
        </section>

        {/* 系统信息 */}
        <section>
          <div className="flex items-center gap-2 mb-3">
            <span className="text-[13px] font-semibold text-[#333]">系统信息</span>
          </div>
          <div className="space-y-2 text-[12px] font-mono text-[#666]">
            <div className="flex justify-between border-b border-[#eee] pb-1">
              <span>版本</span>
              <span>Forensics Workbench 0.1.0</span>
            </div>
            <div className="flex justify-between border-b border-[#eee] pb-1">
              <span>平台</span>
              <span>{navigator.platform || 'Windows'}</span>
            </div>
            <div className="flex justify-between border-b border-[#eee] pb-1">
              <span>数据库</span>
              <span>SQLite (每个案件独立)</span>
            </div>
            <div className="flex justify-between border-b border-[#eee] pb-1">
              <span>搜索引擎</span>
              <span>Tantivy (全文索引)</span>
            </div>
            <div className="flex justify-between border-b border-[#eee] pb-1">
              <span>MCP 协议</span>
              <span>v1.0 (Resources/Tools/Prompts)</span>
            </div>
          </div>
        </section>
      </div>

      {/* 添加服务器对话框 */}
      {showAddDialog && (
        <McpServerDialog
          onClose={() => setShowAddDialog(false)}
          onAdd={addServer}
          testConnection={testConnection}
        />
      )}
    </div>
  );
}
