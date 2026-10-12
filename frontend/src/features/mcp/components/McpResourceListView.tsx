import { ActionIcon } from '@/components/controls/ActionIcon';
import { IconButton } from '@/components/controls/IconButton';
import type { TFunction } from 'i18next';
import type { McpResourceListProps } from '../model/mcp-resource-list';

export function McpResourceListView({ resources, loading, error, onRefresh, t, copiedUri, onCopy }: McpResourceListProps & {
  t: TFunction; copiedUri?: string; onCopy: (uri: string) => void;
}) {
  return (
    <div className="min-w-0 space-y-3">
      <div className="flex items-center justify-between gap-3">
        <h3 className="text-[12px] text-forensics-ink">{t('mcpUi.resources.title')} <span className="text-forensics-muted">({resources.length})</span></h3>
        <IconButton label={t('mcpUi.resources.refresh')} onClick={onRefresh} disabled={loading}>
          <ActionIcon name="refresh" className={loading ? 'size-[15px] animate-spin' : 'size-[15px]'} />
        </IconButton>
      </div>
      {error ? <div role="alert" className="text-[11px] text-forensics-error-text">{error}</div> : null}
      {resources.length === 0 ? (
        <div className="py-3 text-[12px] text-forensics-muted">{t(loading ? 'mcpUi.resources.loading' : 'mcpUi.resources.empty')}</div>
      ) : <div className="space-y-1">{resources.map((resource) => (
        <div key={resource.uri} className="flex min-w-0 items-start gap-3 px-2 py-3 hover:bg-forensics-hover">
          <ActionIcon name="file" className="mt-1 size-[15px] text-forensics-muted" />
          <div className="min-w-0 flex-1">
            <div className="truncate text-[12px] text-forensics-ink">{resource.name}</div>
            <div className="truncate font-mono text-[11px] text-forensics-muted" title={resource.uri}>{resource.uri}</div>
            {resource.description ? <div className="mt-1 text-[11px] text-forensics-muted">{resource.description}</div> : null}
          </div>
          <IconButton label={t('mcpUi.resources.copyNamed', { name: resource.name })} title={t('mcpUi.resources.copy')}
            onClick={() => onCopy(resource.uri)}>
            <ActionIcon name={copiedUri === resource.uri ? 'check' : 'copy'} />
          </IconButton>
        </div>
      ))}</div>}
    </div>
  );
}
