import { useState } from 'react';
import { Button } from '@/app/components/ui/button';
import { Checkbox } from '@/app/components/ui/checkbox';
import { useMcpHostModel } from '../use-mcp-host-model';
import { McpToolList } from './McpToolList';

export function McpHostToolsPanel() {
  const model = useMcpHostModel();
  const [copied, setCopied] = useState(false);
  const [copyError, setCopyError] = useState(false);
  const settings = model.status?.settings;
  const setDisabled = (name: string, disabled: boolean) => {
    if (!settings) return;
    const names = new Set(settings.disabledTools);
    if (disabled) names.add(name); else names.delete(name);
    void model.save({ ...settings, disabledTools: [...names] });
  };
  const copyEndpoint = async () => {
    if (!model.status) return;
    try {
      await navigator.clipboard.writeText(model.status.endpoint);
      setCopied(true);
      setCopyError(false);
    } catch { setCopyError(true); }
  };
  return <div className="space-y-2">
    <div className="flex flex-wrap items-center gap-3 border border-forensics-border bg-forensics-panel p-3">
      <label className="flex items-center gap-2 text-[11px] text-forensics-text">
        <Checkbox aria-label="启用本机 MCP 服务" checked={settings?.enabled ?? false} disabled={model.loading || !settings}
          onCheckedChange={(checked) => { if (settings) void model.save({ ...settings, enabled: checked === true }); }} variant="forensics" />
        启用本机 MCP 服务
      </label>
      <span className="text-[10px] text-forensics-muted" role="status">{model.status?.running ? '运行中' : settings?.enabled ? '未启动' : '已停止'}</span>
      <code className="min-w-0 break-all text-[10px] text-forensics-muted">{model.status?.endpoint ?? 'http://127.0.0.1:3001/mcp'}</code>
      <Button type="button" variant="forensicsGhost" size="compact" disabled={!model.status} onClick={() => void copyEndpoint()}>{copied ? '已复制' : '复制连接地址'}</Button>
      {model.status && settings?.enabled && !model.status.running ? <Button type="button" variant="forensicsOutline" size="compact" disabled={model.loading} onClick={() => void model.save(settings)}>重试启动</Button> : null}
      {copyError ? <span role="alert" className="text-[10px] text-forensics-error-text">复制失败，可选中地址复制。</span> : null}
    </div>
    <McpToolList title="内置只读工具" tools={model.tools} loading={model.loading}
      error={model.error instanceof Error ? model.error.message : model.status?.lastError}
      onRefresh={model.refresh} onTestTool={model.callTool}
      disabledTools={settings?.disabledTools ?? []} toolAccess={settings?.enabled ? 'allowAll' : 'disabled'}
      onToggleTool={setDisabled}
      onToggleAll={(disabled) => { if (settings) void model.save({ ...settings, disabledTools: disabled ? model.tools.map((tool) => tool.name) : [] }); }} />
  </div>;
}
