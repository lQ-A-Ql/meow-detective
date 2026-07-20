import { useEffect } from 'react';
import { FileText, Loader2, RefreshCw } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
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
  }, [refreshResources, serverId]);

  return (
    <div className="bg-forensics-panel border border-forensics-border p-3">
      <div className="flex items-center justify-between mb-2">
        <div className="text-[11px] font-light text-forensics-muted">
          暴露的 Resources
        </div>
        <Button
          type="button"
          variant="forensicsGhost"
          size="iconSm"
          onClick={() => refreshResources(serverId)}
          disabled={loading}
          title="刷新"
        >
          {loading ? (
            <Loader2 size={12} className="opacity-70 text-forensics-muted" />
          ) : (
            <RefreshCw size={12} className="text-forensics-muted" />
          )}
        </Button>
      </div>

      {resources.length === 0 ? (
        <div className="text-[11px] text-forensics-muted py-2">
          {loading ? '加载中...' : '暂无资源'}
        </div>
      ) : (
        <div className="space-y-1">
          {resources.map((resource) => (
            <div
              key={resource.uri}
              className="flex items-start gap-2 p-2 rounded-none hover:bg-forensics-surface transition-colors"
            >
              <FileText size={12} className="text-forensics-info-text mt-0.5 shrink-0" />
              <div className="min-w-0">
                <div className="text-[11px] font-light text-forensics-muted truncate">
                  {resource.name}
                </div>
                <div className="text-[10px] text-forensics-muted font-mono truncate">
                  {resource.uri}
                </div>
                {resource.description && (
                  <div className="text-[10px] text-forensics-muted mt-0.5">
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
