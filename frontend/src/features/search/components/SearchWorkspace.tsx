import { AlertTriangle, File, Folder, Search } from 'lucide-react';
import { ActionIcon } from '@/components/controls/ActionIcon';
import { Tooltip } from '@/app/components/ui/tooltip';
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
import { ContextBar } from '@/components/layout/ContextBar';
import { SearchFilePreviewDialog } from '@/features/search/components/SearchFilePreviewDialog';
import type { SearchWorkspaceModel } from '@/features/search/use-search-workspace-model';
import { formatBytes } from '@/lib/format-bytes';
import type { SearchFileHit } from '@/types/search';
import { WorkspaceFrame } from '@/components/layout/WorkspaceFrame';
import { WorkspaceCard } from '@/components/layout/WorkspaceCard';
import { BrandWatermark } from '@/components/brand';
import { ContentIndexStatus } from './ContentIndexStatus';
import { IconButton } from '@/components/controls/IconButton';

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
        {row.deleted ? <span className="text-[10px] text-forensics-error-text">{t('wechatWorkspace.deleted')}</span> : null}
      </span>
    ),
  },
  {
    key: 'path',
    title: t('search.columns.path'),
    sortable: true,
    sortKey: 'path',
    className: 'w-[38%] text-forensics-text-secondary',
    render: (row) => <div><span className="block truncate">{row.path}</span>{row.snippet ? <span className="block truncate text-[10px] text-forensics-muted" title={row.snippet}>{row.snippet}</span> : null}</div>,
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
    render: (row) => <span className="block truncate" title={row.dataSourceName}>{(row.matchedSources?.length ? row.matchedSources : [row.matchSource ?? 'metadata']).map((source) => t(`contentIndex.sources.${source}`)).join(' / ')} · {row.dataSourceName}</span>,
  },
];
}

export function SearchWorkspace({ model }: SearchWorkspaceProps) {
  const { t } = useTranslation();
  const searchColumns = searchFileColumns(t);
  const selectedSource = model.options.dataSourceIds[0] ?? '__all__';
  const coverage = model.coverage;
  return (
    <WorkspaceFrame>
      <WorkspaceCard className="relative isolate flex flex-1 flex-col">
      <ContextBar className="h-auto min-h-9 overflow-visible py-1">
        <div className="flex min-w-0 flex-1 flex-wrap items-center gap-2">
          <div className="flex min-w-0 flex-1 basis-72 items-center gap-2 px-3 py-1.5">
            <Search size={14} className="shrink-0 text-forensics-ink-3" />
            {model.queryInput ? null : (
              <span aria-hidden="true" className="forensics-caret h-4 w-[7px] shrink-0 bg-forensics-red" />
            )}
            <Input
              autoFocus
              disabled={!model.available}
              value={model.queryInput}
              onChange={(event) => model.setQueryInput(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Escape') model.clearQuery();
              }}
              variant="search"
              inputSize="compact"
              className={`h-6 min-w-0 flex-1 py-0 font-mono text-[13px] leading-6 text-forensics-ink ${model.queryInput ? '' : 'caret-transparent'}`}
              placeholder={t('search.inputPlaceholder')}
              aria-label={t('search.inputLabel')}
            />
            {model.queryInput ? (
              <Tooltip content={t('search.clearQuery')}>
                <Button type="button" variant="forensicsGhost" aria-label={t('search.clearQuery')} onClick={model.clearQuery}>
                  <ActionIcon name="close" />
                </Button>
              </Tooltip>
            ) : null}
          </div>
          <Tooltip content={t('search.pathToggle')}><Button aria-label={t('search.pathToggle')}
            type="button"
            variant={model.options.matchPath ? 'forensicsPrimary' : 'forensicsOutline'}
            size="compact"
            aria-pressed={model.options.matchPath}
            onClick={() => model.setOption('matchPath', !model.options.matchPath)}
          ><ActionIcon name="chevron-down" /></Button></Tooltip>
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
        <IconButton label={t('search.previewSelected')} disabled={!model.selectedHit} onClick={model.previewSelected}><ActionIcon name="eye" /></IconButton>
        <IconButton label={t('search.locateSelected')} disabled={!model.selectedHit || model.navigating} onClick={() => { if (model.selectedHit) model.jumpToFile(model.selectedHit); }}><ActionIcon name="folder" /></IconButton>
        <IconButton label={t('search.refresh')} disabled={!model.available || model.updating} onClick={model.refresh}><ActionIcon name="refresh" /></IconButton>
      </ContextBar>

      <div className="flex min-h-0 flex-1 flex-col">
        <ContentIndexStatus model={model.contentIndex} sources={model.indexSources} sourceId={model.indexSourceId} onSourceChange={model.setIndexSourceId} />
        <div className="flex shrink-0 flex-wrap items-center gap-3 border-b border-forensics-divider bg-forensics-panel px-4 py-2 font-mono text-[10px] text-forensics-muted">
          <span>{!model.available ? t('search.noCase') : model.updating ? t('search.loading') : t('search.meta.found', { count: model.totalHits })}</span>
          <span>{t('search.meta.loaded', { shown: model.searchHits.length, total: model.totalHits })}</span>
          <span>{t('search.meta.took', { ms: model.searchTookMs })}</span>
          <span>{t('search.doubleClickHint')}</span>
          {model.navigating ? <span role="status">{t('search.locating')}</span> : null}
          {coverage && !coverage.complete ? (
            <span className="flex items-center gap-1 text-forensics-error-text" title={t('search.coverage.missingTitle', { sources: coverage.missingSourceIds.join(', ') })}>
              <AlertTriangle size={12} />{t('search.coverage.incomplete', { indexed: coverage.indexedEntryCount, expected: coverage.expectedEntryCount })}
            </span>
          ) : coverage ? <span>{t('search.coverage.complete', { indexed: coverage.indexedEntryCount, sources: coverage.readySourceCount })}</span> : null}
          {model.truncated ? <span className="text-forensics-error-text">{t('search.truncated', { count: model.searchHits.length > 0 ? model.searchHits.length : model.totalHits })}</span> : null}
        </div>

        {model.navigationError ? <div role="alert" className="px-4 py-2 text-[11px] text-forensics-error-text">{model.navigationError}</div> : null}
        <DenseDataTableFrame layout="fill" variant="plain">
          <DenseDataTable<SearchFileHit>
          rows={model.searchHits}
          columns={searchColumns}
          getRowKey={(row) => row.fileId}
          selectedRowKey={model.selectedHit?.fileId}
          onRowClick={model.onHitRowClick}
          onRowDoubleClick={model.jumpToFile}
          emptyTitle={!model.available ? t('search.noCase') : model.updating ? t('search.loading') : model.activeQuery ? t('search.empty.noMatch') : t('search.empty.indexEmpty')}
          emptyDescription={model.activeQuery ? t('search.empty.noMatchDescription') : t('search.empty.indexEmptyDescription')}
            emptyAction={model.activeQuery ? (
              <Tooltip content={t('search.clear')}><Button aria-label={t('search.clear')} type="button" variant="outline" size="sm" onClick={model.clearQuery}><ActionIcon name="close" /></Button></Tooltip>
            ) : undefined}
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
        {!model.activeQuery ? <BrandWatermark motif="search" className="watermark-search" /> : null}
      </WorkspaceCard>
      <SearchFilePreviewDialog model={model.preview} />
    </WorkspaceFrame>
  );
}
