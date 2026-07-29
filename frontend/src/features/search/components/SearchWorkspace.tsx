import { ChevronRight, Filter, Save, Trash2 } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { Input } from '@/app/components/ui/input';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import {
  InspectorPane,
  InspectorSection,
  InspectorValue,
} from '@/components/layout/InspectorPane';
import { PageSubbar } from '@/components/layout/PageSubbar';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import type { SearchWorkspaceModel } from '@/features/search/use-search-workspace-model';
import type { SearchHit } from '@/types/models';

const SEARCH_HIT_COLUMNS: DenseColumn<SearchHit>[] = [
  { key: 'path', title: '路径', className: 'w-[32%]', render: (row) => <span className="block truncate text-forensics-text-secondary">{row.path}</span> },
  { key: 'score', title: '评分', className: 'w-24 text-forensics-muted', render: (row) => row.score.toFixed(2) },
  { key: 'snippet', title: '内容预览', className: 'text-forensics-text-tertiary', render: (row) => <span className="line-clamp-2 text-sm leading-tight">{row.snippets[0]?.text ?? '-'}</span> },
];

interface SearchWorkspaceProps {
  model: SearchWorkspaceModel;
}

/** Pure search presentation surface. Search execution and persistence belong to the workspace model. */
export function SearchWorkspace({ model }: SearchWorkspaceProps) {
  return (
    <div className="flex h-full w-full min-w-0 flex-1 flex-col bg-forensics-surface">
      <PageSubbar title="搜索控制台" meta={`已载入 ${model.searchHits.length}/${model.totalHits} 项命中 / 高置信 ${model.highScoreHits} 项`}>
        <div className="flex shrink-0 flex-col gap-3 p-3">
          <div className="flex items-center gap-3">
            <div className="flex flex-1 items-center border border-forensics-border-strong bg-forensics-surface px-3 py-1.5 transition-colors focus-within:border-forensics-text">
              <span className="mr-2 shrink-0 font-mono text-[11px] text-forensics-muted-light">QUERY</span>
              <Input type="text" variant="search" inputSize="compact" className="w-full font-mono text-[13px] text-forensics-text placeholder-forensics-500" value={model.queryInput} onChange={(event) => model.setQueryInput(event.target.value)} onKeyDown={(event) => { if (event.key === 'Enter') model.submitQuery(); }} />
              <Button type="button" variant="forensicsPrimary" size="compact" onClick={model.submitQuery} className="ml-2 shrink-0 font-light uppercase tracking-wider">执行</Button>
            </div>
            <div className="relative">
              <Button type="button" variant="forensicsOutline" size="xs" onClick={model.toggleSavedQueries}><Filter size={12} /><span>已保存查询</span></Button>
              {model.savedOpen ? (
                <div className="absolute right-0 top-8 z-20 w-80 border border-forensics-border-strong bg-forensics-surface shadow-none">
                  <div className="border-b border-forensics-border p-2">
                    <Input value={model.savedName} onChange={(event) => model.setSavedName(event.target.value)} placeholder="查询名称" variant="mono" inputSize="compact" className="mb-2" />
                    <Button type="button" variant="forensicsPrimary" size="xs" onClick={model.saveCurrentQuery} className="w-full"><Save size={12} />保存当前查询</Button>
                  </div>
                  <ScrollArea className="max-h-64">
                    {model.savedQueries.length ? model.savedQueries.map((item) => (
                      <div key={item.id} className="flex items-start gap-2 border-b border-forensics-border-light p-2 last:border-b-0">
                        <Button type="button" variant="forensicsGhost" size="inline" onClick={() => model.runSavedQuery(item.query)} className="min-w-0 flex-1 flex-col items-start justify-start gap-0 text-left">
                          <div className="truncate text-[12px] font-light text-forensics-text">{item.name}</div>
                          <div className="mt-0.5 line-clamp-2 font-mono text-[10px] text-forensics-muted">{item.query}</div>
                        </Button>
                        <Button type="button" variant="forensicsDangerGhost" size="iconSm" onClick={() => model.removeSavedQuery(item.id)} title="删除保存的查询"><Trash2 size={12} /></Button>
                      </div>
                    )) : <div className="p-3 text-[11px] text-forensics-muted-light">暂无保存的查询。</div>}
                  </ScrollArea>
                </div>
              ) : null}
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-4 font-mono text-[11px] text-forensics-muted-light">
            <div className="flex cursor-pointer items-center gap-1.5 hover:text-forensics-text"><span className="text-forensics-text">范围:</span> 全局</div>
            <div className="flex cursor-pointer items-center gap-1.5 hover:text-forensics-text"><span className="text-forensics-text">模式:</span> Tantivy</div>
            <div className="flex cursor-pointer items-center gap-1.5 hover:text-forensics-text"><span className="text-forensics-text">过滤:</span> 文档 / 表格 / 大文件</div>
            <div className="ml-auto text-forensics-muted">找到 {model.totalHits} 个结果 ({model.searchTookMs}ms)</div>
          </div>
        </div>
      </PageSubbar>

      <div className="flex min-h-0 flex-1 overflow-hidden">
        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
          <div className="flex shrink-0 gap-2 border-b border-forensics-border bg-forensics-panel px-4 py-2 text-[10px] uppercase tracking-wider text-forensics-muted"><span className="border border-forensics-350 bg-forensics-surface px-2 py-0.5">高分命中 {model.highScoreHits}</span><span className="border border-forensics-350 bg-forensics-surface px-2 py-0.5">文档对象</span><span className="border border-forensics-350 bg-forensics-surface px-2 py-0.5">上下文已提取</span></div>
          <div className="flex min-h-0 flex-[2] flex-col border-b border-forensics-border">
            <DenseDataTable<SearchHit> rows={model.searchHits} getRowKey={(row) => row.fileId} selectedRowKey={model.selectedHit?.fileId} onRowClick={model.onHitRowClick} emptyTitle="无搜索命中" emptyDescription="请调整检索语句、范围或过滤条件。" columns={SEARCH_HIT_COLUMNS} loadContextKey={model.loadContextKey} loadStateKey={model.loadStateKey} onReachEnd={model.loadNextPage} onRetryLoadMore={model.retry} hasMore={model.hasMore} loadingMore={model.loadingMore} loadMoreFailed={model.loadMoreFailed} initialLoadFailed={model.initialLoadFailed} onRetryInitialLoad={model.retry} />
          </div>
          <div className="flex min-h-[8rem] flex-1 shrink-0 flex-col bg-forensics-panel">
            <div className="h-7 shrink-0 border-b border-forensics-border bg-forensics-panel px-4 text-[10px] font-light uppercase tracking-wider text-forensics-text-tertiary">上下文预览</div>
            <ScrollArea className="min-h-0 flex-1" viewportClassName="p-4 font-mono text-[11px] leading-[1.6] text-forensics-text-secondary"><div className="mb-2 text-forensics-muted-light">在偏移 0x00A145 处找到匹配项</div><div className="whitespace-pre-wrap border border-forensics-border bg-forensics-surface p-3 text-forensics-text-secondary">{model.selectedHit?.snippets[0]?.text ?? '无上下文预览'}</div></ScrollArea>
          </div>
        </div>
        <InspectorPane title="匹配详情" subtitle={model.selectedHit ? `当前命中 ${model.selectedHit.fileId}` : '未选择命中项'} widthClassName="w-80">
          <div className="space-y-5">
            <InspectorSection title="对象定位"><InspectorValue value={model.selectedHit?.path.split('/').pop() ?? '-'} mono strong /></InspectorSection>
            <InspectorSection title="完整路径"><InspectorValue value={model.selectedHit?.path ?? '-'} mono /></InspectorSection>
            <InspectorSection title="命中字段"><div className="space-y-2 font-mono text-[10px] text-forensics-muted"><div className="flex items-center justify-between border border-forensics-border bg-forensics-surface p-2"><span>score</span><span className="text-forensics-text">{model.selectedHit?.score.toFixed(2) ?? '-'}</span></div><div className="flex items-center justify-between border border-forensics-border bg-forensics-surface p-2"><span>snippet_count</span><span className="text-forensics-text">{model.selectedHit?.snippets.length ?? 0}</span></div></div></InspectorSection>
            <InspectorSection title="命中片段"><InspectorValue value={model.selectedHit?.snippets[0]?.text ?? '-'} mono /></InspectorSection>
            <InspectorSection title="关联动作"><Button type="button" variant="forensicsSurface" size="xs" onClick={model.openSelectedHitInFiles} disabled={!model.selectedHit} className="w-full shadow-sm">在文件浏览中打开 <ChevronRight size={12} /></Button></InspectorSection>
          </div>
        </InspectorPane>
      </div>
    </div>
  );
}
