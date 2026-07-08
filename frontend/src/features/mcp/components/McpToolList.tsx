import { useEffect, useState } from 'react';
import { CheckCircle, Loader2, Play, RefreshCw, Wrench, XCircle } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { useMcpStore } from '@/stores/mcp-store';

interface McpToolListProps {
  serverId: string;
}

export function McpToolList({ serverId }: McpToolListProps) {
  const { tools, loading, refreshTools, callTool } = useMcpStore();
  const [testingTool, setTestingTool] = useState<string | null>(null);
  const [testResult, setTestResult] = useState<{
    toolName: string;
    success: boolean;
    data?: unknown;
    error?: string;
  } | null>(null);

  useEffect(() => {
    if (serverId) {
      refreshTools(serverId);
    }
  }, [refreshTools, serverId]);

  const handleTestTool = async (toolName: string) => {
    setTestingTool(toolName);
    setTestResult(null);
    try {
      const result = await callTool(serverId, toolName, {});
      setTestResult({
        toolName,
        success: result.success,
        data: result.data,
        error: result.error,
      });
    } finally {
      setTestingTool(null);
    }
  };

  return (
    <div className="bg-[#f8f8f8] border border-[#e0e0e0] p-3">
      <div className="flex items-center justify-between mb-2">
        <div className="text-[11px] font-semibold text-[#666]">可用 Tools</div>
        <Button
          type="button"
          variant="forensicsGhost"
          size="iconSm"
          onClick={() => refreshTools(serverId)}
          disabled={loading}
          title="刷新"
        >
          {loading ? (
            <Loader2 size={12} className="animate-spin text-gray-500" />
          ) : (
            <RefreshCw size={12} className="text-gray-500" />
          )}
        </Button>
      </div>

      {tools.length === 0 ? (
        <div className="text-[11px] text-gray-500 py-2">
          {loading ? '加载中...' : '暂无工具'}
        </div>
      ) : (
        <div className="space-y-1">
          {tools.map((tool) => (
            <div
              key={tool.name}
              className="flex items-start gap-2 p-2 rounded hover:bg-white transition-colors"
            >
              <Wrench size={12} className="text-green-500 mt-0.5 shrink-0" />
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2">
                  <div className="text-[11px] font-medium text-gray-900">
                    {tool.name}
                  </div>
                  <Button
                    type="button"
                    variant="forensicsGhost"
                    size="compact"
                    onClick={() => handleTestTool(tool.name)}
                    disabled={testingTool === tool.name}
                    className="h-5 bg-gray-100 px-1.5 py-0.5 text-[9px] hover:bg-gray-200"
                    title="测试调用"
                  >
                    {testingTool === tool.name ? (
                      <Loader2 size={10} className="animate-spin" />
                    ) : (
                      <Play size={10} />
                    )}
                  </Button>
                </div>
                <div className="text-[10px] text-gray-500">{tool.description}</div>
              </div>
            </div>
          ))}
        </div>
      )}

      {testResult && (
        <div
          className={`mt-3 p-2 rounded text-[11px] ${
            testResult.success
              ? 'bg-green-50 border border-green-200'
              : 'bg-red-50 border border-red-200'
          }`}
        >
          <div className="flex items-center gap-1 mb-1">
            {testResult.success ? (
              <CheckCircle size={12} className="text-green-600" />
            ) : (
              <XCircle size={12} className="text-red-600" />
            )}
            <span className={testResult.success ? 'text-green-700' : 'text-red-700'}>
              {testResult.toolName}
            </span>
          </div>
          {testResult.error && (
            <div className="text-[10px] text-red-600">{testResult.error}</div>
          )}
          {testResult.data !== undefined && testResult.data !== null && (
            <pre className="text-[10px] text-gray-600 mt-1 overflow-auto max-h-20">
              {JSON.stringify(testResult.data, null, 2)}
            </pre>
          )}
        </div>
      )}
    </div>
  );
}
