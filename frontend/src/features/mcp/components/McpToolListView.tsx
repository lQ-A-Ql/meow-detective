import { Checkbox } from '@/app/components/ui/checkbox';
import { Input } from '@/app/components/ui/input';
import { Textarea } from '@/app/components/ui/textarea';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { ListRow } from '@/components/lists/ListRow';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { IconButton } from '@/components/controls/IconButton';
import { Tooltip } from '@/app/components/ui/tooltip';
import { VirtualList } from '@/components/lists/VirtualList';
import type { TFunction } from 'i18next';
import type { McpToolListProps } from '../model/mcp-tool-list';
import { formatMcpArguments, formatMcpResult, type McpToolResult } from '../tool-format';

interface ViewProps extends McpToolListProps {
  t: TFunction;
  query: string;
  onQuery: (value: string) => void;
  filteredTools: McpToolListProps['tools'];
  selectedTool?: McpToolListProps['tools'][number];
  argumentsText: string;
  onArguments: (value: string) => void;
  argumentError?: string;
  testingTool?: string;
  testResult?: McpToolResult;
  disabledCount: number;
  isToolDisabled: (name: string) => boolean;
  selectTool: (tool: McpToolListProps['tools'][number]) => void;
  runTool: (tool: McpToolListProps['tools'][number]) => void;
}

export function McpToolListView(p: ViewProps) {
  const { t, tools, loading, error, onRefresh, filteredTools, selectedTool, argumentsText, onArguments, argumentError,
    testingTool, testResult, disabledCount, isToolDisabled, selectTool, runTool, toolAccess = 'allowAll',
    onToggleTool, onToggleAll, onEnableAll } = p;
  return (
    <div className="min-w-0 space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <h3 className="mr-auto text-[12px] text-forensics-ink">
          {p.title ?? t('mcpUi.tools.title')} <span className="text-forensics-muted">({tools.length})</span>
        </h3>
        <span className="text-[11px] text-forensics-muted">
          {t('mcpUi.tools.enabled', { enabled: Math.max(0, tools.length - disabledCount), total: tools.length })}
        </span>
        {toolAccess === 'allowAll' && onToggleAll && tools.length > 0 ? <>
          <IconButton label={t('mcpUi.tools.enableAll')} disabled={loading} onClick={() => onToggleAll(false)}><ActionIcon name="check" /></IconButton>
          <IconButton label={t('mcpUi.tools.disableAll')} disabled={loading} onClick={() => onToggleAll(true)}><ActionIcon name="pause" /></IconButton>
        </> : null}
        {toolAccess !== 'allowAll' && onEnableAll ? (
          <IconButton label={t(toolAccess === 'disabled' ? 'mcpUi.tools.enable' : 'mcpUi.tools.allTools')} onClick={onEnableAll}>
            <ActionIcon name="play" />
          </IconButton>
        ) : null}
        <IconButton label={t('mcpUi.tools.refresh')} onClick={onRefresh} disabled={loading}>
          <ActionIcon name="refresh" className={loading ? 'size-[15px] animate-spin' : 'size-[15px]'} />
        </IconButton>
      </div>
      {error ? <div role="alert" className="text-[11px] text-forensics-error-text">{error}</div> : null}
      {tools.length > 0 ? <div className="relative">
        <ActionIcon name="search" className="pointer-events-none absolute left-0 top-1/2 size-3 -translate-y-1/2 text-forensics-muted" />
        <Input value={p.query} onChange={(event) => p.onQuery(event.target.value)} placeholder={t('mcpUi.tools.search')}
          aria-label={t('mcpUi.tools.search')} inputSize="compact" className="pl-6" />
      </div> : null}
      {filteredTools.length === 0 ? (
        <div className="py-3 text-[12px] text-forensics-muted">{loading ? t('mcpUi.tools.loading') : tools.length ? t('mcpUi.tools.noMatch') : t('mcpUi.tools.empty')}</div>
      ) : <VirtualList items={filteredTools} getItemKey={(tool) => tool.name} estimateSize={58}
        style={{ height: 'min(32vh, 260px)' }} ariaLabel={t('mcpUi.tools.listLabel')}
        renderItem={(tool) => <div className="flex w-full min-w-0 items-center gap-2 py-1">
          <ListRow selected={selectedTool?.name === tool.name} wrapLabel onActivate={() => selectTool(tool)}
            ariaLabel={tool.name} className="min-w-0 flex-1"
            label={<>
              <span className="block truncate text-[12px] text-forensics-ink">{tool.name}</span>
              <span className="block truncate text-[11px] text-forensics-muted" title={tool.description}>
                {tool.description || t('mcpUi.tools.noDescription')}
              </span>
            </>} />
          <IconButton label={t('mcpUi.tools.testWithCurrent')} onClick={() => void runTool(tool)}
            disabled={Boolean(testingTool) || loading || isToolDisabled(tool.name)}>
            <ActionIcon name="play" />
          </IconButton>
          {onToggleTool ? <Tooltip content={t(isToolDisabled(tool.name) ? 'mcpUi.tools.enable' : 'mcpUi.tools.disable')}>
            <Checkbox aria-label={`${t('mcpUi.tools.enable')} ${tool.name}`} checked={!isToolDisabled(tool.name)} disabled={loading}
              onCheckedChange={(checked) => onToggleTool(tool.name, checked !== true)} />
          </Tooltip> : null}
        </div>} />}
      {selectedTool ? <div className="space-y-2 pt-2">
        <div className="flex items-center justify-between gap-2">
          <div className="truncate text-[12px] text-forensics-ink">{selectedTool.name} {t('mcpUi.tools.parameter')}</div>
          <span className="text-[11px] text-forensics-muted">{t('mcpUi.tools.json')}</span>
        </div>
        <Textarea value={argumentsText || formatMcpArguments(selectedTool.inputSchema)} onChange={(event) => onArguments(event.target.value)}
          variant="mono" textareaSize="compact" rows={5} spellCheck={false}
          aria-label={`${selectedTool.name} ${t('mcpUi.tools.parameter')}`} aria-invalid={Boolean(argumentError)} />
        {argumentError ? <div role="alert" className="text-[11px] text-forensics-error-text">{t(argumentError)}</div> : null}
        <div className="flex justify-end">
          <IconButton label={t('mcpUi.tools.test')} variant="solid" onClick={() => void runTool(selectedTool)}
            disabled={Boolean(testingTool) || loading || isToolDisabled(selectedTool.name)}><ActionIcon name="play" /></IconButton>
        </div>
      </div> : null}
      {testResult ? <div role="status" className="space-y-2 py-2 text-[11px]">
        <div className={`flex items-center gap-2 ${testResult.success ? 'text-forensics-success-text' : 'text-forensics-error-text'}`}>
          <ActionIcon name={testResult.success ? 'check' : 'warning'} className="size-3" />
          <span>{testResult.toolName}</span>
        </div>
        {testResult.error ? <div className="text-forensics-error-text">{testResult.error}</div> : null}
        {testResult.data !== undefined ? <ScrollArea className="max-h-28" showHorizontalScrollbar>
          <pre className="text-[11px] text-forensics-muted">{formatMcpResult(testResult.data)}</pre>
        </ScrollArea> : null}
      </div> : null}
    </div>
  );
}
