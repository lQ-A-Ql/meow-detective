import { Filter, ChevronRight, Save, Trash2 } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useNavigate, useSearchParams } from 'react-router';
import { PageSubbar } from '@/components/layout/PageSubbar';
import { DenseDataTable } from '@/components/tables/DenseDataTable';
import { InspectorPane, InspectorSection, InspectorValue } from '@/components/layout/InspectorPane';
import { useSearchResults } from '@/features/search/hooks';
import {
  readSavedSearchQueries,
  removeSavedSearchQuery,
  upsertSavedSearchQuery,
  writeSavedSearchQueries,
} from '@/lib/saved-queries';
import { useSelectionStore } from '@/stores/selection-store';
import { SearchHit } from '@/types/models';

const defaultQuery = 'content:password AND path:doc';

export function Search() {
  const [searchParams] = useSearchParams();
  const urlQuery = searchParams.get('q');
  const initialQuery = urlQuery?.trim() || defaultQuery;
  const [queryInput, setQueryInput] = useState(initialQuery);
  const [activeQuery, setActiveQuery] = useState(initialQuery);
  const [savedOpen, setSavedOpen] = useState(false);
  const [savedName, setSavedName] = useState('');
  const [savedQueries, setSavedQueries] = useState(() => readSavedSearchQueries());
  const { data } = useSearchResults(activeQuery);
  const selectedSearchHitId = useSelectionStore((state) => state.selectedSearchHitId);
  const setSelectedSearchHitId = useSelectionStore((state) => state.setSelectedSearchHitId);
  const navigate = useNavigate();
  const selectedHit = data?.items.find((item) => item.fileId === selectedSearchHitId) ?? data?.items[0];
  const highScoreHits = data?.items.filter((item) => item.score >= 0.8).length ?? 0;

  useEffect(() => {
    const nextQuery = urlQuery?.trim() || defaultQuery;
    setQueryInput(nextQuery);
    setActiveQuery(nextQuery);
  }, [urlQuery]);

  function persistSavedQueries(next: typeof savedQueries) {
    setSavedQueries(next);
    writeSavedSearchQueries(next);
  }

  function saveCurrentQuery() {
    const name = savedName.trim() || queryInput.slice(0, 48);
    const next = upsertSavedSearchQuery(savedQueries, name, queryInput);
    persistSavedQueries(next);
    setSavedName('');
    setSavedOpen(true);
  }

  function runSavedQuery(query: string) {
    setQueryInput(query);
    setActiveQuery(query);
    setSavedOpen(false);
  }

  return (
    <div className="flex-1 flex flex-col w-full h-full bg-white min-w-0">
      <PageSubbar title="搜索控制台" meta={`共 ${data?.total ?? 0} 项命中 / 高置信 ${highScoreHits} 项`}>
        <div className="shrink-0 p-3 flex flex-col gap-3">
          <div className="flex items-center gap-3">
            <div className="flex items-center bg-white border border-forensics-border-strong px-3 py-1.5 flex-1 focus-within:border-forensics-text transition-colors">
              <span className="text-forensics-muted-light font-mono text-[11px] mr-2 shrink-0">QUERY</span>
              <input
                type="text"
                className="bg-transparent border-none outline-none text-forensics-text font-mono text-[13px] w-full placeholder-forensics-500"
                value={queryInput}
                onChange={(e) => setQueryInput(e.target.value)}
                onKeyDown={(e) => { if (e.key === 'Enter') setActiveQuery(queryInput); }}
              />
              <button
                onClick={() => setActiveQuery(queryInput)}
                className="bg-forensics-text text-white font-semibold text-[11px] px-3 py-0.5 ml-2 uppercase tracking-wider shrink-0 hover:bg-forensics-text-secondary"
              >
                执行
              </button>
            </div>
            <div className="relative">
              <button
                type="button"
                onClick={() => setSavedOpen((open) => !open)}
                className="flex items-center gap-2 border border-forensics-border bg-white px-3 py-1.5 text-[11px] text-forensics-muted cursor-pointer hover:text-forensics-text"
              >
                <Filter size={12} />
                <span>已保存查询</span>
              </button>
              {savedOpen ? (
                <div className="absolute right-0 top-8 z-20 w-80 border border-forensics-border-strong bg-white shadow-lg">
                  <div className="border-b border-forensics-border p-2">
                    <input
                      value={savedName}
                      onChange={(event) => setSavedName(event.target.value)}
                      placeholder="查询名称"
                      className="mb-2 w-full border border-forensics-border-strong px-2 py-1 text-[11px] font-mono"
                    />
                    <button
                      type="button"
                      onClick={saveCurrentQuery}
                      className="flex w-full items-center justify-center gap-1 border border-forensics-text bg-forensics-text px-2 py-1 text-[11px] text-white hover:bg-forensics-text-secondary"
                    >
                      <Save size={12} />
                      保存当前查询
                    </button>
                  </div>
                  <div className="max-h-64 overflow-auto">
                    {savedQueries.length ? (
                      savedQueries.map((item) => (
                        <div
                          key={item.id}
                          className="flex items-start gap-2 border-b border-forensics-border-light p-2 last:border-b-0"
                        >
                          <button
                            type="button"
                            onClick={() => runSavedQuery(item.query)}
                            className="min-w-0 flex-1 text-left hover:text-forensics-text"
                          >
                            <div className="truncate text-[12px] font-medium text-forensics-text">
                              {item.name}
                            </div>
                            <div className="mt-0.5 line-clamp-2 font-mono text-[10px] text-forensics-muted">
                              {item.query}
                            </div>
                          </button>
                          <button
                            type="button"
                            onClick={() =>
                              persistSavedQueries(removeSavedSearchQuery(savedQueries, item.id))
                            }
                            className="p-1 text-forensics-muted-light hover:text-red-600"
                            title="删除保存的查询"
                          >
                            <Trash2 size={12} />
                          </button>
                        </div>
                      ))
                    ) : (
                      <div className="p-3 text-[11px] text-forensics-muted-light">暂无保存的查询。</div>
                    )}
                  </div>
                </div>
              ) : null}
            </div>
          </div>
          <div className="flex items-center gap-4 text-[11px] text-forensics-muted-light font-mono flex-wrap">
            <div className="flex items-center gap-1.5 cursor-pointer hover:text-forensics-text">
              <span className="text-forensics-text">范围:</span> 全局
            </div>
            <div className="flex items-center gap-1.5 cursor-pointer hover:text-forensics-text">
              <span className="text-forensics-text">模式:</span> Tantivy
            </div>
            <div className="flex items-center gap-1.5 cursor-pointer hover:text-forensics-text">
              <span className="text-forensics-text">过滤:</span> 文档 / 表格 / 大文件
            </div>
            <div className="ml-auto text-forensics-muted">找到 {data?.total ?? 0} 个结果 ({data?.tookMs ?? 0}ms)</div>
          </div>
        </div>
      </PageSubbar>

      <div className="flex-1 flex overflow-hidden min-h-0">
        <div className="flex-1 flex flex-col min-w-0 min-h-0">
          <div className="shrink-0 border-b border-forensics-border bg-forensics-panel px-4 py-2 flex gap-2 text-[10px] uppercase tracking-wider text-forensics-muted">
            <span className="border border-forensics-350 bg-white px-2 py-0.5">高分命中 {highScoreHits}</span>
            <span className="border border-forensics-350 bg-white px-2 py-0.5">文档对象</span>
            <span className="border border-forensics-350 bg-white px-2 py-0.5">上下文已提取</span>
          </div>

          <div className="flex-[2] flex flex-col border-b border-forensics-border min-h-0">
            <DenseDataTable<SearchHit>
              rows={data?.items ?? []}
              getRowKey={(row) => row.fileId}
              selectedRowKey={selectedHit?.fileId}
              onRowClick={(row) => setSelectedSearchHitId(row.fileId)}
              emptyTitle="无搜索命中"
              emptyDescription="请调整检索语句、范围或过滤条件。"
              columns={[
                { key: 'path', title: '路径', className: 'w-[32%]', render: (row) => <span className="truncate block text-forensics-text-secondary">{row.path}</span> },
                { key: 'score', title: '评分', className: 'w-24 text-forensics-muted', render: (row) => row.score.toFixed(2) },
                {
                  key: 'snippet',
                  title: '内容预览',
                  className: 'text-forensics-text-tertiary',
                  render: (row) => <span className="line-clamp-2 text-sm leading-tight">{row.snippets[0]?.text ?? '-'}</span>,
                },
              ]}
            />
          </div>

          <div className="flex-1 min-h-[8rem] bg-forensics-panel flex flex-col shrink-0">
            <div className="h-7 border-b border-forensics-border flex items-center px-4 text-[10px] font-semibold uppercase text-forensics-text-tertiary tracking-wider shrink-0 bg-forensics-panel">
              上下文预览
            </div>
            <div className="flex-1 overflow-auto p-4 font-mono text-[11px] text-forensics-text-secondary leading-[1.6]">
              <div className="text-forensics-muted-light mb-2">在偏移 0x00A145 处找到匹配项</div>
              <div className="bg-white border border-forensics-border p-3 text-forensics-text-secondary whitespace-pre-wrap">
                {selectedHit?.snippets[0]?.text ?? '无上下文预览'}
              </div>
            </div>
          </div>
        </div>

        <InspectorPane
          title="匹配详情"
          subtitle={selectedHit ? `当前命中 ${selectedHit.fileId}` : '未选择命中项'}
          widthClassName="w-80"
        >
          <div className="space-y-5">
            <InspectorSection title="对象定位">
              <InspectorValue value={selectedHit?.path.split('/').pop() ?? '-'} mono strong />
            </InspectorSection>

            <InspectorSection title="完整路径">
              <InspectorValue value={selectedHit?.path ?? '-'} mono />
            </InspectorSection>

            <InspectorSection title="命中字段">
              <div className="space-y-2 text-[10px] font-mono text-forensics-muted">
                <div className="flex items-center justify-between border border-forensics-border bg-white p-2">
                  <span>score</span>
                  <span className="text-forensics-text">{selectedHit?.score.toFixed(2) ?? '-'}</span>
                </div>
                <div className="flex items-center justify-between border border-forensics-border bg-white p-2">
                  <span>snippet_count</span>
                  <span className="text-forensics-text">{selectedHit?.snippets.length ?? 0}</span>
                </div>
              </div>
            </InspectorSection>

            <InspectorSection title="命中片段">
              <InspectorValue value={selectedHit?.snippets[0]?.text ?? '-'} mono />
            </InspectorSection>

            <InspectorSection title="关联动作">
              <button
                onClick={() => {
                  if (selectedHit) {
                    useSelectionStore.getState().setSelectedFileId(selectedHit.fileId);
                    navigate("/files");
                  }
                }}
                className="w-full border border-forensics-border-strong bg-white text-forensics-text hover:bg-forensics-hover py-1.5 text-center font-sans text-[11px] transition-colors rounded-[2px] cursor-pointer shadow-sm flex items-center justify-center gap-1.5"
              >
                在文件浏览中打开 <ChevronRight size={12} />
              </button>
            </InspectorSection>
          </div>
        </InspectorPane>
      </div>
    </div>
  );
}
