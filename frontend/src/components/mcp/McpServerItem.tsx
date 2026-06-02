import { useState } from 'react';
import { Wifi, WifiOff, Trash2, Loader2 } from 'lucide-react';

interface McpServer {
  id: string;
  name: string;
  transportType: 'sse' | 'stdio';
  url?: string;
  command?: string;
  enabled: boolean;
  connected: boolean;
  hasResources: boolean;
  hasTools: boolean;
  hasPrompts: boolean;
  lastError?: string;
}

interface McpServerItemProps {
  server: McpServer;
  isSelected: boolean;
  onConnect: () => void;
  onDisconnect: () => void;
  onRemove: () => void;
  onSelect: () => void;
}

export function McpServerItem({
  server,
  isSelected,
  onConnect,
  onDisconnect,
  onRemove,
  onSelect,
}: McpServerItemProps) {
  const [loading, setLoading] = useState(false);

  const handleConnect = async () => {
    setLoading(true);
    try {
      if (server.connected) {
        await onDisconnect();
      } else {
        await onConnect();
      }
    } finally {
      setLoading(false);
    }
  };

  const getStatusColor = () => {
    if (server.connected) return 'bg-green-500';
    if (server.lastError) return 'bg-red-500';
    return 'bg-gray-400';
  };

  const getStatusText = () => {
    if (server.connected) return '已连接';
    if (server.lastError) return '错误';
    return '未连接';
  };

  return (
    <div
      className={`flex items-center gap-3 p-2 rounded cursor-pointer transition-colors ${
        isSelected
          ? 'bg-blue-50 border border-blue-200'
          : 'hover:bg-gray-50 border border-transparent'
      }`}
      onClick={onSelect}
    >
      {/* Status indicator */}
      <div className={`w-2 h-2 rounded-full ${getStatusColor()}`} />

      {/* Server info */}
      <div className="flex-1 min-w-0">
        <div className="text-[12px] font-medium text-gray-900 truncate">
          {server.name}
        </div>
        <div className="text-[10px] text-gray-500 truncate">
          {server.transportType === 'sse' ? server.url : server.command}
        </div>
      </div>

      {/* Capabilities badges */}
      <div className="flex gap-1">
        {server.hasResources && (
          <span className="px-1 py-0.5 text-[9px] bg-blue-100 text-blue-700 rounded">
            R
          </span>
        )}
        {server.hasTools && (
          <span className="px-1 py-0.5 text-[9px] bg-green-100 text-green-700 rounded">
            T
          </span>
        )}
        {server.hasPrompts && (
          <span className="px-1 py-0.5 text-[9px] bg-purple-100 text-purple-700 rounded">
            P
          </span>
        )}
      </div>

      {/* Status text */}
      <span className="text-[10px] text-gray-500 w-12 text-center">
        {getStatusText()}
      </span>

      {/* Action buttons */}
      <div className="flex gap-1">
        <button
          onClick={(e) => {
            e.stopPropagation();
            handleConnect();
          }}
          disabled={loading}
          className="p-1 rounded hover:bg-gray-200 transition-colors disabled:opacity-50"
          title={server.connected ? '断开' : '连接'}
        >
          {loading ? (
            <Loader2 size={14} className="animate-spin text-gray-500" />
          ) : server.connected ? (
            <WifiOff size={14} className="text-gray-600" />
          ) : (
            <Wifi size={14} className="text-gray-600" />
          )}
        </button>

        <button
          onClick={(e) => {
            e.stopPropagation();
            onRemove();
          }}
          className="p-1 rounded hover:bg-red-100 transition-colors"
          title="删除"
        >
          <Trash2 size={14} className="text-gray-600 hover:text-red-600" />
        </button>
      </div>
    </div>
  );
}
