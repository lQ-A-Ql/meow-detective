import { useState } from 'react';
import { X, Loader2, CheckCircle, XCircle } from 'lucide-react';

interface McpServerDialogProps {
  onClose: () => void;
  onAdd: (server: {
    name: string;
    transportType: 'sse' | 'stdio';
    url?: string;
    command?: string;
    args?: string[];
    enabled: boolean;
    autoConnect: boolean;
  }) => Promise<void>;
  testConnection: (
    transportType: string,
    url?: string,
    command?: string,
    args?: string[]
  ) => Promise<{ success: boolean; error?: string }>;
}

export function McpServerDialog({ onClose, onAdd, testConnection }: McpServerDialogProps) {
  const [name, setName] = useState('');
  const [transportType, setTransportType] = useState<'sse' | 'stdio'>('sse');
  const [url, setUrl] = useState('http://localhost:3001');
  const [command, setCommand] = useState('');
  const [argsStr, setArgsStr] = useState('');
  const [enabled, setEnabled] = useState(true);
  const [autoConnect, setAutoConnect] = useState(false);

  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ success: boolean; error?: string } | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleTest = async () => {
    setTesting(true);
    setTestResult(null);
    try {
      const args = argsStr ? argsStr.split(' ').filter(Boolean) : undefined;
      const result = await testConnection(transportType, url, command, args);
      setTestResult(result);
    } finally {
      setTesting(false);
    }
  };

  const handleSave = async () => {
    if (!name.trim()) {
      setError('请输入服务器名称');
      return;
    }

    if (transportType === 'sse' && !url.trim()) {
      setError('请输入 SSE URL');
      return;
    }

    if (transportType === 'stdio' && !command.trim()) {
      setError('请输入命令');
      return;
    }

    setSaving(true);
    setError(null);
    try {
      const args = argsStr ? argsStr.split(' ').filter(Boolean) : undefined;
      await onAdd({
        name: name.trim(),
        transportType,
        url: transportType === 'sse' ? url : undefined,
        command: transportType === 'stdio' ? command : undefined,
        args,
        enabled,
        autoConnect,
      });
      onClose();
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-white rounded-lg shadow-xl w-[480px] max-h-[90vh] overflow-auto">
        {/* Header */}
        <div className="flex items-center justify-between p-4 border-b">
          <h2 className="text-[14px] font-semibold text-gray-900">添加 MCP 服务器</h2>
          <button
            onClick={onClose}
            className="p-1 rounded hover:bg-gray-100 transition-colors"
          >
            <X size={16} className="text-gray-500" />
          </button>
        </div>

        {/* Content */}
        <div className="p-4 space-y-4">
          {/* Name */}
          <div>
            <label className="block text-[12px] font-medium text-gray-700 mb-1">
              服务器名称 <span className="text-red-500">*</span>
            </label>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="例如: Claude Desktop"
              className="w-full px-3 py-2 text-[12px] border rounded focus:outline-none focus:ring-1 focus:ring-blue-500"
            />
          </div>

          {/* Transport Type */}
          <div>
            <label className="block text-[12px] font-medium text-gray-700 mb-1">
              传输类型
            </label>
            <div className="flex gap-2">
              <button
                onClick={() => setTransportType('sse')}
                className={`flex-1 px-3 py-2 text-[12px] rounded border transition-colors ${
                  transportType === 'sse'
                    ? 'bg-blue-50 border-blue-300 text-blue-700'
                    : 'bg-white border-gray-300 text-gray-700 hover:bg-gray-50'
                }`}
              >
                HTTP/SSE
              </button>
              <button
                onClick={() => setTransportType('stdio')}
                className={`flex-1 px-3 py-2 text-[12px] rounded border transition-colors ${
                  transportType === 'stdio'
                    ? 'bg-blue-50 border-blue-300 text-blue-700'
                    : 'bg-white border-gray-300 text-gray-700 hover:bg-gray-50'
                }`}
              >
                Stdio
              </button>
            </div>
          </div>

          {/* SSE URL */}
          {transportType === 'sse' && (
            <div>
              <label className="block text-[12px] font-medium text-gray-700 mb-1">
                SSE URL <span className="text-red-500">*</span>
              </label>
              <input
                type="text"
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                placeholder="http://localhost:3001"
                className="w-full px-3 py-2 text-[12px] border rounded focus:outline-none focus:ring-1 focus:ring-blue-500"
              />
            </div>
          )}

          {/* Stdio Command */}
          {transportType === 'stdio' && (
            <>
              <div>
                <label className="block text-[12px] font-medium text-gray-700 mb-1">
                  命令 <span className="text-red-500">*</span>
                </label>
                <input
                  type="text"
                  value={command}
                  onChange={(e) => setCommand(e.target.value)}
                  placeholder="python -m forensics_mcp"
                  className="w-full px-3 py-2 text-[12px] border rounded focus:outline-none focus:ring-1 focus:ring-blue-500"
                />
              </div>
              <div>
                <label className="block text-[12px] font-medium text-gray-700 mb-1">
                  参数 (空格分隔)
                </label>
                <input
                  type="text"
                  value={argsStr}
                  onChange={(e) => setArgsStr(e.target.value)}
                  placeholder="--port 3001 --verbose"
                  className="w-full px-3 py-2 text-[12px] border rounded focus:outline-none focus:ring-1 focus:ring-blue-500"
                />
              </div>
            </>
          )}

          {/* Options */}
          <div className="flex gap-4">
            <label className="flex items-center gap-2">
              <input
                type="checkbox"
                checked={enabled}
                onChange={(e) => setEnabled(e.target.checked)}
                className="rounded"
              />
              <span className="text-[12px] text-gray-700">启用</span>
            </label>
            <label className="flex items-center gap-2">
              <input
                type="checkbox"
                checked={autoConnect}
                onChange={(e) => setAutoConnect(e.target.checked)}
                className="rounded"
              />
              <span className="text-[12px] text-gray-700">自动连接</span>
            </label>
          </div>

          {/* Test Result */}
          {testResult && (
            <div
              className={`p-3 rounded text-[12px] ${
                testResult.success
                  ? 'bg-green-50 text-green-700 border border-green-200'
                  : 'bg-red-50 text-red-700 border border-red-200'
              }`}
            >
              <div className="flex items-center gap-2">
                {testResult.success ? (
                  <CheckCircle size={14} />
                ) : (
                  <XCircle size={14} />
                )}
                <span>{testResult.success ? '连接成功' : '连接失败'}</span>
              </div>
              {testResult.error && (
                <div className="mt-1 text-[11px] opacity-80">{testResult.error}</div>
              )}
            </div>
          )}

          {/* Error */}
          {error && (
            <div className="p-3 rounded text-[12px] bg-red-50 text-red-700 border border-red-200">
              {error}
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="flex items-center justify-between p-4 border-t bg-gray-50">
          <button
            onClick={handleTest}
            disabled={testing}
            className="px-4 py-2 text-[12px] border rounded hover:bg-gray-100 transition-colors disabled:opacity-50"
          >
            {testing ? (
              <span className="flex items-center gap-2">
                <Loader2 size={12} className="animate-spin" />
                测试中...
              </span>
            ) : (
              '测试连接'
            )}
          </button>

          <div className="flex gap-2">
            <button
              onClick={onClose}
              className="px-4 py-2 text-[12px] border rounded hover:bg-gray-100 transition-colors"
            >
              取消
            </button>
            <button
              onClick={handleSave}
              disabled={saving}
              className="px-4 py-2 text-[12px] bg-blue-600 text-white rounded hover:bg-blue-700 transition-colors disabled:opacity-50"
            >
              {saving ? (
                <span className="flex items-center gap-2">
                  <Loader2 size={12} className="animate-spin" />
                  保存中...
                </span>
              ) : (
                '添加'
              )}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
