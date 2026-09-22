import { AlertTriangle, File, Folder, Search, X } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/app/components/ui/button';
import { Input } from '@/app/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/app/components/ui/select';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import { DenseDataTableFrame } from '@/components/tables/DenseDataTableFrame';
import { PageSubbar } from '@/components/layout/PageSubbar';
import { SearchFilePreviewDialog } from '@/features/search/components/SearchFilePreviewDialog';
import type { SearchWorkspaceModel } from '@/features/search/use-search-workspace-model';
import { formatBytes } from '@/lib/format-bytes';
import type { SearchFileHit } from '@/types/search';

interface SearchWorkspaceProps {
  model: SearchWorkspaceModel;
}

function formatModifiedAt(value?: string) {
  if (!value) return '-';
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

function entryIcon(entryType: string) {
  return entryType === 'directory'
    ? <Folder size={13} className="text-forensics-sakura-500" />
    : <File size={13} className="text-forensics-muted" />;
}

function searchFileColumns(t: ReturnType<typeof useTranslation>['t']): DenseColumn<SearchFileHit>[] {
return [
  {
    key: 'name',
    title: t('search.columns.name'),
    sortable: true,
    sortKey: 'name',
    className: 'w-[27%]',
    render: (row) => (
      <span className="flex min-w-0 items-center gap-2 text-forensics-text">
        {entryIcon(row.entryType)}
        <span className="truncate">{row.name || row.path}</span>
        {row.deleted ? <span className="text-[10px] text-forensics-error-text">已删除</span> : null}
      </span>
    ),
  },
  {
    key: 'path',
    title: t('search.columns.path'),
    sortable: true,
    sortKey: 'path',
    className: 'w-[38%] text-forensics-text-secondary',
    render: (row) => <span className="block truncate">{row.path}</span>,
  },
  {
    key: 'size',
    title: t('search.columns.size'),
    sortable: true,
    sortKey: 'size',
    className: 'w-28 text-right text-forensics-muted',
    render: (row) => row.size === undefined ? '-' : formatBytes(row.size),
  },
  {
    key: 'modifiedAt',
    title: t('search.columns.modifiedAt'),
    sortable: true,
    sortKey: 'modifiedAt',
    className: 'w-44 text-forensics-muted',
    render: (row) => formatModifiedAt(row.modifiedAt),
  },
  {
    key: 'source',
    title: t('search.columns.source'),
    className: 'w-36 text-forensics-muted',
    render: (row) => <span className="block truncate" title={row.dataSourceName}>{row.dataSourceName}</span>,
  },
];
}

export function SearchWorkspace({ model }: SearchWorkspaceProps) {
  const { t } = useTranslation();
  const searchColumns = searchFileColumns(t);
  const selectedSource = model.options.dataSourceIds[0] ?? '__all__';
  const coverage = model.coverage;
  const resultMeta = model.activeQuery
    ? t('search.meta.loaded', { shown: model.searchHits.length, total: model.totalHits })
    : t('search.meta.waiting');

  return (
    <div className="flex h-full min-h-0 w-full min-w-0 flex-1 flex-col bg-forensics-surface">
      <PageSubbar title={t('search.title')} meta={resultMeta}>
        <div className="flex min-w-0 flex-1 items-center gap-2 p-3">
          <div className="flex min-w-0 flex-1 items-center border border-forensics-border-strong bg-forensics-surface px-3 py-1.5 focus-within:border-forensics-sakura-500">
            <Search size={14} className="mr-2 shrink-0 text-forensics-muted" />
            <Input
              autoFocus
              value={model.queryInput}
              onChange={(event) => model.setQueryInput(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Escape') model.clearQuery();
              }}
              variant="search"
              inputSize="compact"
              className="w-full font-mono text-[13px] text-forensics-text"
              placeholder={t('search.inputPlaceholder')}
              aria-label={t('search.inputLabel')}
            />
            {model.queryInput ? (
              <Button type="button" variant="forensicsGhost" size="iconXs" onClick={model.clearQuery} title="清除搜索">
                <X size={13} />
              </Button>
            ) : null}
          </div>
          <Button
            type="button"
            variant={model.options.matchPath ? 'forensicsPrimary' : 'forensicsOutline'}
            size="compact"
            onClick={() => model.setOption('matchPath', !model.options.matchPath)}
          >
            {t('search.pathToggle')}
          </Button>
          <Select value={model.options.entryType} onValueChange={(value) => model.setOption('entryType', value as SearchWorkspaceModel['options']['entryType'])}>
            <SelectTrigger size="xs" variant="forensics" className="w-24"><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="any">{t('search.allTypes')}</SelectItem>
              <SelectItem value="file">{t('search.file')}</SelectItem>
              <SelectItem value="directory">{t('search.directory')}</SelectItem>
            </SelectContent>
          </Select>
          <Input
            value={model.extensionInput}
            onChange={(event) => model.setExtensionInput(event.target.value)}
            variant="forensics"
            inputSize="compact"
            className="w-24 font-mono"
            placeholder={t('search.extensionPlaceholder')}
            aria-label={t('search.extensionLabel')}
          />
          <Select value={selectedSource} onValueChange={(value) => model.setOption('dataSourceIds', value === '__all__' ? [] : [value])}>
            <SelectTrigger size="xs" variant="forensics" className="w-32"><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="__all__">{t('search.allSources')}</SelectItem>
              {model.dataSources.map((source) => <SelectItem key={source.id} value={source.id}>{source.name}</SelectItem>)}
            </SelectContent>
          </Select>
        </div>
      </PageSubbar>

      <div className="flex min-h-0 flex-1 flex-col">
        <div className="flex shrink-0 items-center gap-3 border-b border-forensics-border bg-forensics-panel px-4 py-2 font-mono text-[10px] text-forensics-muted">
          <span>{model.activeQuery ? t('search.meta.found', { count: model.totalHits }) : t('search.meta.idle')}</span>
          {model.activeQuery ? <span>{t('search.meta.took', { ms: model.searchTookMs })}</span> : null}
          {coverage && !coverage.complete ? (
            <span className="flex items-center gap-1 text-forensics-error-text" title={`未就绪数据源: ${coverage.missingSourceIds.join(', ')}`}>
              <AlertTriangle size={12} />{t('search.coverage.incomplete', { indexed: coverage.indexedEntryCount, expected: coverage.expectedEntryCount })}
            </span>
          ) : coverage ? <span>{t('search.coverage.complete', { indexed: coverage.indexedEntryCount, sources: coverage.readySourceCount })}</span> : null}
          {model.truncated ? <span className="text-forensics-error-text">{t('search.truncated', { count: model.searchHits.length > 0 ? model.searchHits.length : model.totalHits })}</span> : null}
        </div>

        <DenseDataTableFrame layout="fill" variant="plain">
          <DenseDataTable<SearchFileHit>
          rows={model.searchHits}
          columns={searchColumns}
          getRowKey={(row) => row.fileId}
          selectedRowKey={model.selectedHit?.fileId}
          onRowClick={model.onHitRowClick}
          emptyTitle={model.activeQuery ? t('search.empty.noMatch') : t('search.empty.start')}
          emptyDescription={model.activeQuery ? t('search.empty.noMatchDescription') : t('search.empty.startDescription')}
          sortKey={model.sortKey}
          sortDirection={model.sortDirection}
          onSort={model.toggleSort}
          loadContextKey={model.loadContextKey}
          loadStateKey={model.searchQueryStateKey}
          onReachEnd={model.loadNextPage}
          onRetryLoadMore={model.retry}
          hasMore={model.hasMore}
          loadingMore={model.loadingMore}
          loadMoreFailed={model.loadMoreFailed}
          initialLoadFailed={model.initialLoadFailed}
          onRetryInitialLoad={model.retry}
          />
        </DenseDataTableFrame>
      </div>
      <SearchFilePreviewDialog model={model.preview} />
    </div>
  );
}
