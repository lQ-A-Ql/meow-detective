import { Check, Copy, FileText, Loader2, RefreshCw } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/app/components/ui/button';

export interface McpResourceListProps {
  resources: Array<{
    uri: string;
    name: string;
    description?: string;
  }>;
  loading: boolean;
  error?: string | null;
  onRefresh: () => void;
}

export function McpResourceList({ resources, loading, error, onRefresh }: McpResourceListProps) {
  const [copiedUri, setCopiedUri] = useState<string>();
  const copyUri = async (uri: string) => {
    try {
      await navigator.clipboard?.writeText(uri);
      setCopiedUri(uri);
      window.setTimeout(() => setCopiedUri((current) => current === uri ? undefined : current), 1500);
    } catch {
      setCopiedUri(undefined);
    }
  };
  return (
    <div className="bg-forensics-panel border border-forensics-border p-3">
      <div className="flex items-center justify-between mb-2">
        <div className="text-[11px] font-light text-forensics-muted">暴露的 Resources</div>
        <Button
          type="button"
          variant="forensicsGhost"
          size="iconSm"
          onClick={onRefresh}
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

      {error ? <div className="mb-2 border border-forensics-error-border bg-forensics-error-bg p-2 text-[10px] text-forensics-error-text">{error}</div> : null}

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
              <div className="min-w-0 flex-1">
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
              <Button type="button" variant="forensicsGhost" size="iconSm" onClick={() => void copyUri(resource.uri)} title="复制资源 URI" aria-label={`复制 ${resource.name} URI`}>
                {copiedUri === resource.uri ? <Check size={12} className="text-forensics-success-text" /> : <Copy size={12} className="text-forensics-muted" />}
              </Button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
