import { useEffect } from 'react';
import { RefreshCw, FileText, Loader2 } from 'lucide-react';
import { useMcpStore } from '@/stores/mcp-store';

interface McpResourceListProps {
  serverId: string;
}

export function McpResourceList({ serverId }: McpResourceListProps) {
  const { resources, loading, refreshResources } = useMcpStore();

  useEffect(() => {
    if (serverId) {
      refreshResources(serverId);
    }
  }, [serverId]);

  return (
    <div className="bg-[#f8f8f8] border border-[#e0e0e0] p-3">
      <div className="flex items-center justify-between mb-2">
        <div className="text-[11px] font-semibold text-[#666]">
          暴露的 Resources
        </div>
        <button
          onClick={() => refreshResources(serverId)}
          disabled={loading}
          className="p-1 rounded hover:bg-gray-200 transition-colors disabled:opacity-50"
          title="刷新"
        >
          {loading ? (
            <Loader2 size={12} className="animate-spin text-gray-500" />
          ) : (
            <RefreshCw size={12} className="text-gray-500" />
          )}
        </button>
      </div>

      {resources.length === 0 ? (
        <div className="text-[11px] text-gray-500 py-2">
          {loading ? '加载中...' : '暂无资源'}
        </div>
      ) : (
        <div className="space-y-1">
          {resources.map((resource) => (
            <div
              key={resource.uri}
              className="flex items-start gap-2 p-2 rounded hover:bg-white transition-colors"
            >
              <FileText size={12} className="text-blue-500 mt-0.5 shrink-0" />
              <div className="min-w-0">
                <div className="text-[11px] font-medium text-gray-900 truncate">
                  {resource.name}
                </div>
                <div className="text-[10px] text-gray-500 font-mono truncate">
                  {resource.uri}
                </div>
                {resource.description && (
                  <div className="text-[10px] text-gray-400 mt-0.5">
                    {resource.description}
                  </div>
                )}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
