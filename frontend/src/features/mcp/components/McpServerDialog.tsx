import { useState } from 'react';
import { CheckCircle, Loader2, X, XCircle } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { Checkbox } from '@/app/components/ui/checkbox';
import { Field, FieldLabel } from '@/app/components/ui/field';
import { Input } from '@/app/components/ui/input';
import type { McpPermissionProfile } from '@/lib/api/mcp';

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
    permissions?: McpPermissionProfile;
  }) => Promise<void>;
  testConnection: (
    transportType: string,
    url?: string,
    command?: string,
    args?: string[],
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
        <div className="flex items-center justify-between p-4 border-b">
          <h2 className="text-[14px] font-semibold text-gray-900">添加 MCP 服务器</h2>
          <Button
            type="button"
            variant="forensicsGhost"
            size="iconSm"
            onClick={onClose}
          >
            <X size={16} className="text-gray-500" />
          </Button>
        </div>

        <div className="p-4 space-y-4">
          <Field>
            <FieldLabel>
              服务器名称 <span className="text-red-500">*</span>
            </FieldLabel>
            <Input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="例如: Claude Desktop"
              variant="forensics"
              inputSize="compact"
            />
          </Field>

          <div>
            <label className="block text-[12px] font-medium text-gray-700 mb-1">
              传输类型
            </label>
            <div className="flex gap-2">
              <Button
                type="button"
                variant={transportType === 'sse' ? 'forensicsSurface' : 'forensicsOutline'}
                size="sm"
                onClick={() => setTransportType('sse')}
                className={transportType === 'sse' ? 'flex-1 border-blue-300 bg-blue-50 text-blue-700' : 'flex-1'}
              >
                HTTP/SSE
              </Button>
              <Button
                type="button"
                variant={transportType === 'stdio' ? 'forensicsSurface' : 'forensicsOutline'}
                size="sm"
                onClick={() => setTransportType('stdio')}
                className={transportType === 'stdio' ? 'flex-1 border-blue-300 bg-blue-50 text-blue-700' : 'flex-1'}
              >
                Stdio
              </Button>
            </div>
          </div>

          {transportType === 'sse' && (
            <Field>
              <FieldLabel>
                SSE URL <span className="text-red-500">*</span>
              </FieldLabel>
              <Input
                type="text"
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                placeholder="http://localhost:3001"
                variant="path"
                inputSize="compact"
              />
            </Field>
          )}

          {transportType === 'stdio' && (
            <>
              <Field>
                <FieldLabel>
                  命令 <span className="text-red-500">*</span>
                </FieldLabel>
                <Input
                  type="text"
                  value={command}
                  onChange={(e) => setCommand(e.target.value)}
                  placeholder="python -m forensics_mcp"
                  variant="mono"
                  inputSize="compact"
                />
              </Field>
              <Field>
                <FieldLabel>
                  参数 (空格分隔)
                </FieldLabel>
                <Input
                  type="text"
                  value={argsStr}
                  onChange={(e) => setArgsStr(e.target.value)}
                  placeholder="--port 3001 --verbose"
                  variant="mono"
                  inputSize="compact"
                />
              </Field>
            </>
          )}

          <div className="flex gap-4">
            <label className="flex items-center gap-2">
              <Checkbox
                checked={enabled}
                onCheckedChange={(checked) => setEnabled(checked === true)}
                variant="forensics"
              />
              <span className="text-[12px] text-gray-700">启用</span>
            </label>
            <label className="flex items-center gap-2">
              <Checkbox
                checked={autoConnect}
                onCheckedChange={(checked) => setAutoConnect(checked === true)}
                variant="forensics"
              />
              <span className="text-[12px] text-gray-700">自动连接</span>
            </label>
          </div>

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

          {error && (
            <div className="p-3 rounded text-[12px] bg-red-50 text-red-700 border border-red-200">
              {error}
            </div>
          )}
        </div>

        <div className="flex items-center justify-between p-4 border-t bg-gray-50">
          <Button
            type="button"
            variant="forensicsOutline"
            size="sm"
            onClick={handleTest}
            disabled={testing}
          >
            {testing ? (
              <span className="flex items-center gap-2">
                <Loader2 size={12} className="animate-spin" />
                测试中...
              </span>
            ) : (
              '测试连接'
            )}
          </Button>

          <div className="flex gap-2">
            <Button
              type="button"
              variant="forensicsOutline"
              size="sm"
              onClick={onClose}
            >
              取消
            </Button>
            <Button
              type="button"
              variant="forensicsPrimary"
              size="sm"
              onClick={handleSave}
              disabled={saving}
              className="bg-blue-600 hover:bg-blue-700"
            >
              {saving ? (
                <span className="flex items-center gap-2">
                  <Loader2 size={12} className="animate-spin" />
                  保存中...
                </span>
              ) : (
                '添加'
              )}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
