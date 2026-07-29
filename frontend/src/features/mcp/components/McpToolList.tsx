import { useState } from 'react';
import { CheckCircle, Loader2, Play, RefreshCw, Wrench, XCircle } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { ScrollArea } from '@/app/components/ui/scroll-area';

export interface McpToolListProps {
  tools: Array<{ name: string; description: string }>;
  loading: boolean;
  onRefresh: () => void;
  onTestTool: (toolName: string) => Promise<{
    success: boolean;
    data?: unknown;
    error?: string;
  }>;
}

export function McpToolList({ tools, loading, onRefresh, onTestTool }: McpToolListProps) {
  const [testingTool, setTestingTool] = useState<string | null>(null);
  const [testResult, setTestResult] = useState<{
    toolName: string;
    success: boolean;
    data?: unknown;
    error?: string;
  } | null>(null);

  const handleTestTool = async (toolName: string) => {
    setTestingTool(toolName);
    setTestResult(null);
    try {
      const result = await onTestTool(toolName);
      setTestResult({ toolName, ...result });
    } finally {
      setTestingTool(null);
    }
  };

  return (
    <div className="bg-forensics-panel border border-forensics-border p-3">
      <div className="flex items-center justify-between mb-2">
        <div className="text-[11px] font-light text-forensics-muted">可用 Tools</div>
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

      {tools.length === 0 ? (
        <div className="text-[11px] text-forensics-muted py-2">
          {loading ? '加载中...' : '暂无工具'}
        </div>
      ) : (
        <div className="space-y-1">
          {tools.map((tool) => (
            <div
              key={tool.name}
              className="flex items-start gap-2 p-2 rounded-none hover:bg-forensics-surface transition-colors"
            >
              <Wrench size={12} className="text-forensics-success-text mt-0.5 shrink-0" />
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2">
                  <div className="text-[11px] font-light text-forensics-muted">{tool.name}</div>
                  <Button
                    type="button"
                    variant="forensicsGhost"
                    size="compact"
                    onClick={() => void handleTestTool(tool.name)}
                    disabled={testingTool === tool.name}
                    className="h-5 bg-forensics-panel px-1.5 py-0.5 text-[9px] hover:bg-forensics-panel"
                    title="测试调用"
                  >
                    {testingTool === tool.name ? <Loader2 size={10} className="opacity-70" /> : <Play size={10} />}
                  </Button>
                </div>
                <div className="text-[10px] text-forensics-muted">{tool.description}</div>
              </div>
            </div>
          ))}
        </div>
      )}

      {testResult && (
        <div
          className={`mt-3 p-2 rounded-none text-[11px] ${
            testResult.success
              ? 'bg-forensics-success-bg border border-forensics-success-border'
              : 'bg-forensics-error-bg border border-forensics-error-border'
          }`}
        >
          <div className="flex items-center gap-1 mb-1">
            {testResult.success ? (
              <CheckCircle size={12} className="text-forensics-success-text" />
            ) : (
              <XCircle size={12} className="text-forensics-error-text" />
            )}
            <span className={testResult.success ? 'text-forensics-success-text' : 'text-forensics-error-text'}>
              {testResult.toolName}
            </span>
          </div>
          {testResult.error && <div className="text-[10px] text-forensics-error-text">{testResult.error}</div>}
          {testResult.data !== undefined && testResult.data !== null && (
            <ScrollArea className="mt-1 max-h-20" showHorizontalScrollbar>
              <pre className="p-1 text-[10px] text-forensics-muted">
                {JSON.stringify(testResult.data, null, 2)}
              </pre>
            </ScrollArea>
          )}
        </div>
      )}
    </div>
  );
}
