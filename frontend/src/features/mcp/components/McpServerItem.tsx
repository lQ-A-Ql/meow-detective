import { useState } from 'react';
import { Loader2, Trash2, Wifi, WifiOff } from 'lucide-react';
import { Button } from '@/app/components/ui/button';

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
    if (server.connected) return 'bg-forensics-success-bg';
    if (server.lastError) return 'bg-forensics-error-bg';
    return 'bg-forensics-panel';
  };

  const getStatusText = () => {
    if (server.connected) return '已连接';
    if (server.lastError) return '错误';
    return '未连接';
  };

  return (
    <div
      className={`flex items-center gap-3 p-2 rounded-none cursor-pointer transition-colors ${
        isSelected
          ? 'bg-forensics-info-bg border border-forensics-info-border'
          : 'hover:bg-forensics-panel border border-transparent'
      }`}
      onClick={onSelect}
    >
      <div className={`w-2 h-2 rounded-none ${getStatusColor()}`} />

      <div className="flex-1 min-w-0">
        <div className="text-[12px] font-light text-forensics-muted truncate">
          {server.name}
        </div>
        <div className="text-[10px] text-forensics-muted truncate">
          {server.transportType === 'sse' ? server.url : server.command}
        </div>
      </div>

      <div className="flex gap-1">
        {server.hasResources && (
          <span className="px-1 py-0.5 text-[9px] bg-forensics-info-bg text-forensics-info-text rounded-none">
            R
          </span>
        )}
        {server.hasTools && (
          <span className="px-1 py-0.5 text-[9px] bg-forensics-success-bg text-forensics-success-text rounded-none">
            T
          </span>
        )}
        {server.hasPrompts && (
          <span className="px-1 py-0.5 text-[9px] bg-forensics-info-bg text-forensics-info-text rounded-none">
            P
          </span>
        )}
      </div>

      <span className="text-[10px] text-forensics-muted w-12 text-center">
        {getStatusText()}
      </span>

      <div className="flex gap-1">
        <Button
          type="button"
          variant="forensicsGhost"
          size="iconSm"
          onClick={(e) => {
            e.stopPropagation();
            handleConnect();
          }}
          disabled={loading}
          title={server.connected ? '断开' : '连接'}
        >
          {loading ? (
            <Loader2 size={14} className="opacity-70 text-forensics-muted" />
          ) : server.connected ? (
            <WifiOff size={14} className="text-forensics-muted" />
          ) : (
            <Wifi size={14} className="text-forensics-muted" />
          )}
        </Button>

        <Button
          type="button"
          variant="forensicsDangerGhost"
          size="iconSm"
          onClick={(e) => {
            e.stopPropagation();
            onRemove();
          }}
          title="删除"
        >
          <Trash2 size={14} className="text-forensics-muted hover:text-forensics-error-text" />
        </Button>
      </div>
    </div>
  );
}
