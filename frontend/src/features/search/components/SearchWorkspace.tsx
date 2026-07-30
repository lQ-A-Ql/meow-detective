import { AlertTriangle, File, Folder, Search, X } from 'lucide-react';
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
import { PageSubbar } from '@/components/layout/PageSubbar';
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

const SEARCH_FILE_COLUMNS: DenseColumn<SearchFileHit>[] = [
  {
    key: 'name',
    title: '名称',
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
    title: '路径',
    sortable: true,
    sortKey: 'path',
    className: 'w-[38%] text-forensics-text-secondary',
    render: (row) => <span className="block truncate">{row.path}</span>,
  },
  {
    key: 'size',
    title: '大小',
    sortable: true,
    sortKey: 'size',
    className: 'w-28 text-right text-forensics-muted',
    render: (row) => row.size === undefined ? '-' : formatBytes(row.size),
  },
  {
    key: 'modifiedAt',
    title: '修改时间',
    sortable: true,
    sortKey: 'modifiedAt',
    className: 'w-44 text-forensics-muted',
    render: (row) => formatModifiedAt(row.modifiedAt),
  },
  {
    key: 'source',
    title: '数据源',
    className: 'w-36 text-forensics-muted',
    render: (row) => <span className="block truncate" title={row.dataSourceName}>{row.dataSourceName}</span>,
  },
];

export function SearchWorkspace({ model }: SearchWorkspaceProps) {
  const selectedSource = model.options.dataSourceIds[0] ?? '__all__';
  const coverage = model.coverage;
  const resultMeta = model.activeQuery
    ? `已载入 ${model.searchHits.length}/${model.totalHits} 项`
    : '输入文件名或路径开始搜索';

  return (
    <div className="flex h-full min-h-0 w-full min-w-0 flex-1 flex-col bg-forensics-surface">
      <PageSubbar title="文件搜索" meta={resultMeta}>
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
              placeholder="输入文件名、目录名或路径"
              aria-label="文件名搜索"
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
            路径
          </Button>
          <Select value={model.options.entryType} onValueChange={(value) => model.setOption('entryType', value as SearchWorkspaceModel['options']['entryType'])}>
            <SelectTrigger size="xs" variant="forensics" className="w-24"><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="any">全部类型</SelectItem>
              <SelectItem value="file">文件</SelectItem>
              <SelectItem value="directory">目录</SelectItem>
            </SelectContent>
          </Select>
          <Input
            value={model.extensionInput}
            onChange={(event) => model.setExtensionInput(event.target.value)}
            variant="forensics"
            inputSize="compact"
            className="w-24 font-mono"
            placeholder="扩展名"
            aria-label="扩展名筛选"
          />
          <Select value={selectedSource} onValueChange={(value) => model.setOption('dataSourceIds', value === '__all__' ? [] : [value])}>
            <SelectTrigger size="xs" variant="forensics" className="w-32"><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="__all__">全部数据源</SelectItem>
              {model.dataSources.map((source) => <SelectItem key={source.id} value={source.id}>{source.name}</SelectItem>)}
            </SelectContent>
          </Select>
        </div>
      </PageSubbar>

      <div className="flex min-h-0 flex-1 flex-col">
        <div className="flex shrink-0 items-center gap-3 border-b border-forensics-border bg-forensics-panel px-4 py-2 font-mono text-[10px] text-forensics-muted">
          <span>{model.activeQuery ? `找到 ${model.totalHits} 个结果` : '等待输入'}</span>
          {model.activeQuery ? <span>查询 {model.searchTookMs} ms</span> : null}
          {coverage && !coverage.complete ? (
            <span className="flex items-center gap-1 text-forensics-error-text" title={`未就绪数据源: ${coverage.missingSourceIds.join(', ')}`}>
              <AlertTriangle size={12} />索引覆盖不完整 {coverage.indexedEntryCount}/{coverage.expectedEntryCount}
            </span>
          ) : coverage ? <span>索引覆盖 {coverage.indexedEntryCount} 项 / {coverage.readySourceCount} 个数据源</span> : null}
          {model.truncated ? <span className="text-forensics-error-text">结果超过浏览上限，仅显示前 {model.searchHits.length > 0 ? model.searchHits.length : model.totalHits} 项窗口</span> : null}
        </div>

        <DenseDataTable<SearchFileHit>
          rows={model.searchHits}
          columns={SEARCH_FILE_COLUMNS}
          getRowKey={(row) => row.fileId}
          selectedRowKey={model.selectedHit?.fileId}
          onRowClick={model.onHitRowClick}
          onRowDoubleClick={model.openHitInFiles}
          emptyTitle={model.activeQuery ? '没有匹配的文件' : '输入文件名开始搜索'}
          emptyDescription={model.activeQuery ? '尝试修改文件名、路径或筛选条件。' : '搜索仅定位已导入数据源中的文件和目录。'}
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
      </div>
    </div>
  );
}
