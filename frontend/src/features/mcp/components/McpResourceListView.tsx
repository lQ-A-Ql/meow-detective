import { Button } from '@/app/components/ui/button';
import type { TFunction } from 'i18next';
import { mcpIcons } from '../mcp-icons';
import type { McpResourceListProps } from '../model/mcp-resource-list';

export function McpResourceListView({ resources, loading, error, onRefresh, t, copiedUri, onCopy }: McpResourceListProps & { t: TFunction; copiedUri?: string; onCopy: (uri: string) => void }) {
  const { Check, Copy, FileText, Loader2, RefreshCw } = mcpIcons;
  return <div className="bg-forensics-panel border border-forensics-border p-3"><div className="flex items-center justify-between mb-2"><div className="text-[11px] font-light text-forensics-muted">{t('mcpUi.resources.title')}</div><Button type="button" variant="forensicsGhost" size="iconSm" onClick={onRefresh} disabled={loading} title={t('mcpUi.resources.refresh')}>{loading ? <Loader2 size={12} className="opacity-70 text-forensics-muted" /> : <RefreshCw size={12} className="text-forensics-muted" />}</Button></div>
    {error ? <div className="mb-2 border border-forensics-error-border bg-forensics-error-bg p-2 text-[10px] text-forensics-error-text">{error}</div> : null}
    {resources.length === 0 ? <div className="text-[11px] text-forensics-muted py-2">{t(loading ? 'mcpUi.resources.loading' : 'mcpUi.resources.empty')}</div> : <div className="space-y-1">{resources.map((resource) => <div key={resource.uri} className="flex items-start gap-2 p-2 rounded-none hover:bg-forensics-surface"><FileText size={12} className="text-forensics-info-text mt-0.5 shrink-0" /><div className="min-w-0 flex-1"><div className="text-[11px] font-light text-forensics-muted truncate">{resource.name}</div><div className="text-[10px] text-forensics-muted font-mono truncate">{resource.uri}</div>{resource.description ? <div className="text-[10px] text-forensics-muted mt-0.5">{resource.description}</div> : null}</div><Button type="button" variant="forensicsGhost" size="iconSm" onClick={() => onCopy(resource.uri)} title={t('mcpUi.resources.copy')} aria-label={t('mcpUi.resources.copyNamed', { name: resource.name })}>{copiedUri === resource.uri ? <Check size={12} className="text-forensics-success-text" /> : <Copy size={12} className="text-forensics-muted" />}</Button></div>)}</div>}
  </div>;
}
