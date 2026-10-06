import { useEffect, useMemo, useState } from 'react';
import { CheckCircle, Loader2, Play, RefreshCw, Search, Wrench, XCircle } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { Input } from '@/app/components/ui/input';
import { Textarea } from '@/app/components/ui/textarea';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { VirtualList } from '@/components/lists/VirtualList';
import type { JsonSchema, McpTool } from '@/lib/api/mcp';

export interface McpToolListProps {
  tools: McpTool[];
  loading: boolean;
  error?: string | null;
  onRefresh: () => void;
  onTestTool: (toolName: string, args: unknown) => Promise<{
    success: boolean;
    data?: unknown;
    error?: string;
  }>;
  disabledTools?: string[];
  onToggleTool?: (toolName: string, disabled: boolean) => void;
  toolAccess?: 'allowAll' | 'disabled' | 'allowList';
  onEnableAll?: () => void;
  onToggleAll?: (disabled: boolean) => void;
}

export function McpToolList({ tools, loading, error, onRefresh, onTestTool, disabledTools = [], onToggleTool, toolAccess = 'allowAll', onEnableAll, onToggleAll }: McpToolListProps) {
  const [query, setQuery] = useState('');
  const [selectedToolName, setSelectedToolName] = useState<string>();
  const [argumentsText, setArgumentsText] = useState('');
  const [argumentError, setArgumentError] = useState<string>();
  const [testingTool, setTestingTool] = useState<string>();
  const [testResult, setTestResult] = useState<{
    toolName: string;
    success: boolean;
    data?: unknown;
    error?: string;
  }>();
  const filteredTools = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    if (!normalized) return tools;
    return tools.filter((tool) => `${tool.name} ${tool.description}`.toLowerCase().includes(normalized));
  }, [query, tools]);
  const selectedTool = tools.find((tool) => tool.name === selectedToolName) ?? filteredTools[0];
  const isToolDisabled = (toolName: string) => disabledTools.some((name) => name.toLowerCase() === toolName.toLowerCase());
  const disabledCount = tools.filter((tool) => isToolDisabled(tool.name)).length;

  useEffect(() => {
    if (!selectedToolName || tools.some((tool) => tool.name === selectedToolName)) return;
    setSelectedToolName(undefined);
    setArgumentsText('');
    setArgumentError(undefined);
  }, [selectedToolName, tools]);

  const selectTool = (tool: McpTool) => {
    setSelectedToolName(tool.name);
    setArgumentsText(formatArguments(tool.inputSchema));
    setArgumentError(undefined);
    setTestResult(undefined);
  };

  const runTool = async (tool: McpTool, rawArguments = tool.name === selectedToolName ? argumentsText : formatArguments(tool.inputSchema)) => {
    let args: unknown;
    try {
      args = JSON.parse(rawArguments || '{}');
    } catch {
      setSelectedToolName(tool.name);
      setArgumentsText(rawArguments);
      setArgumentError('参数必须是有效的 JSON。');
      return;
    }
    setSelectedToolName(tool.name);
    setArgumentsText(JSON.stringify(args, null, 2));
    setArgumentError(undefined);
    setTestingTool(tool.name);
    setTestResult(undefined);
    try {
      const result = await onTestTool(tool.name, args);
      setTestResult({ toolName: tool.name, ...result });
    } finally {
      setTestingTool(undefined);
    }
  };

  return (
    <div className="min-w-0 border border-forensics-border bg-forensics-panel p-3">
      <div className="mb-2 flex items-center justify-between gap-2">
        <div className="text-[11px] font-light text-forensics-muted">可用 Tools ({tools.length})</div>
        <span className="text-[10px] text-forensics-muted">已启用 {Math.max(0, tools.length - disabledCount)}/{tools.length}</span>
        {toolAccess === 'allowAll' && onToggleAll && tools.length > 0 ? <>
          <Button type="button" variant="forensicsGhost" size="compact" onClick={() => onToggleAll(false)} className="h-6 px-2 text-[10px]">全部启用</Button>
          <Button type="button" variant="forensicsGhost" size="compact" onClick={() => onToggleAll(true)} className="h-6 px-2 text-[10px]">全部禁用</Button>
        </> : null}
        {toolAccess !== 'allowAll' && onEnableAll ? <Button type="button" variant="forensicsOutline" size="compact" onClick={onEnableAll} className="mr-auto ml-2 h-6 px-2 text-[10px]">{toolAccess === 'disabled' ? '启用工具' : '切换为全部工具'}</Button> : null}
        <Button type="button" variant="forensicsGhost" size="iconSm" onClick={onRefresh} disabled={loading} title="刷新工具">
          {loading ? <Loader2 size={12} className="opacity-70 text-forensics-muted" /> : <RefreshCw size={12} className="text-forensics-muted" />}
        </Button>
      </div>

      {error ? <div className="mb-2 border border-forensics-error-border bg-forensics-error-bg p-2 text-[10px] text-forensics-error-text">{error}</div> : null}
      {tools.length > 0 ? <div className="relative mb-2"><Search size={12} className="pointer-events-none absolute left-2 top-1/2 -translate-y-1/2 text-forensics-muted" /><Input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索工具名称或描述" variant="forensics" inputSize="compact" className="pl-7" /></div> : null}

      {filteredTools.length === 0 ? (
        <div className="py-2 text-[11px] text-forensics-muted">{loading ? '加载中...' : tools.length ? '没有匹配的工具' : '暂无工具'}</div>
      ) : (
        <VirtualList items={filteredTools} getItemKey={(tool) => tool.name} estimateSize={58} style={{ height: 'min(32vh, 260px)' }} ariaLabel="MCP 工具列表"
          renderItem={(tool) => (
            <div className={`flex w-full items-start gap-2 border-b border-forensics-border-light p-2 text-left transition-colors hover:bg-forensics-surface ${selectedTool?.name === tool.name ? 'bg-forensics-surface' : ''}`}>
              <Wrench size={12} className="mt-0.5 shrink-0 text-forensics-success-text" />
              <button type="button" className="min-w-0 flex-1 text-left" onClick={() => selectTool(tool)}>
                <span className="block truncate text-[11px] font-light text-forensics-muted">{tool.name}</span>
                <span className="block truncate text-[10px] text-forensics-muted" title={tool.description}>{tool.description || '无描述'}</span>
              </button>
              <Button type="button" variant="forensicsGhost" size="compact" onClick={(event) => { event.stopPropagation(); void runTool(tool); }} disabled={testingTool === tool.name || isToolDisabled(tool.name)} className="h-5 shrink-0 px-1.5 py-0.5 text-[9px]" title="使用当前参数测试">
                {testingTool === tool.name ? <Loader2 size={10} className="opacity-70" /> : <Play size={10} />}
              </Button>
              {onToggleTool ? <Button type="button" variant="forensicsGhost" size="compact" aria-pressed={isToolDisabled(tool.name)} onClick={(event) => { event.stopPropagation(); onToggleTool(tool.name, !isToolDisabled(tool.name)); }} className="h-5 shrink-0 px-1.5 py-0.5 text-[9px]" title={isToolDisabled(tool.name) ? '启用工具' : '禁用工具'}>{isToolDisabled(tool.name) ? '启用' : '禁用'}</Button> : null}
            </div>
          )} />
      )}

      {selectedTool ? <div className="mt-3 border-t border-forensics-border pt-3">
        <div className="mb-1 flex items-center justify-between gap-2"><div className="truncate text-[11px] font-light text-forensics-text">{selectedTool.name} 参数</div><span className="text-[10px] text-forensics-muted">JSON</span></div>
        <Textarea value={argumentsText || formatArguments(selectedTool.inputSchema)} onChange={(event) => { setArgumentsText(event.target.value); setArgumentError(undefined); }} variant="mono" textareaSize="compact" rows={5} spellCheck={false} aria-label={`${selectedTool.name} 参数`} />
        {argumentError ? <div className="mt-1 text-[10px] text-forensics-error-text">{argumentError}</div> : null}
        <Button type="button" variant="forensicsOutline" size="compact" onClick={() => void runTool(selectedTool)} disabled={testingTool === selectedTool.name} className="mt-2 text-[10px]">
          {testingTool === selectedTool.name ? <Loader2 size={11} className="mr-1 animate-spin" /> : <Play size={11} className="mr-1" />}测试工具
        </Button>
      </div> : null}

      {testResult ? <div className={`mt-3 rounded-none p-2 text-[11px] ${testResult.success ? 'border border-forensics-success-border bg-forensics-success-bg' : 'border border-forensics-error-border bg-forensics-error-bg'}`}>
        <div className="mb-1 flex items-center gap-1">{testResult.success ? <CheckCircle size={12} className="text-forensics-success-text" /> : <XCircle size={12} className="text-forensics-error-text" />}<span className={testResult.success ? 'text-forensics-success-text' : 'text-forensics-error-text'}>{testResult.toolName}</span></div>
        {testResult.error ? <div className="text-[10px] text-forensics-error-text">{testResult.error}</div> : null}
        {testResult.data !== undefined ? <ScrollArea className="mt-1 max-h-28" showHorizontalScrollbar><pre className="p-1 text-[10px] text-forensics-muted">{formatResult(testResult.data)}</pre></ScrollArea> : null}
      </div> : null}
    </div>
  );
}

function formatArguments(schema: JsonSchema): string {
  return JSON.stringify(defaultArguments(schema), null, 2);
}

function defaultArguments(schema: JsonSchema): unknown {
  if (schema.type === 'object') {
    const properties = schema.properties ?? {};
    const required = new Set(schema.required ?? Object.keys(properties));
    return Object.fromEntries(Object.entries(properties).filter(([name]) => required.has(name)).map(([name, value]) => [name, defaultArguments(value)]));
  }
  if (schema.type === 'array') return [];
  if (schema.type === 'boolean') return false;
  if (schema.type === 'number' || schema.type === 'integer') return 0;
  return '';
}

function formatResult(value: unknown): string {
  if (typeof value === 'string') return value;
  try { return JSON.stringify(value, null, 2); } catch { return String(value); }
}
