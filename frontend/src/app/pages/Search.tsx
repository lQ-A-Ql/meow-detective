import { Filter, ChevronRight } from 'lucide-react';
import { PageSubbar } from '@/components/layout/PageSubbar';
import { DenseDataTable } from '@/components/tables/DenseDataTable';
import { InspectorPane, InspectorSection, InspectorValue } from '@/components/layout/InspectorPane';
import { useSearchResults } from '@/features/search/hooks';
import { useSelectionStore } from '@/stores/selection-store';
import { SearchHit } from '@/types/models';

const defaultQuery = "files WHERE extension IN ('.doc', '.xls') AND size > 10MB";

export function Search() {
  const { data } = useSearchResults(defaultQuery);
  const selectedSearchHitId = useSelectionStore((state) => state.selectedSearchHitId);
  const setSelectedSearchHitId = useSelectionStore((state) => state.setSelectedSearchHitId);
  const selectedHit = data?.items.find((item) => item.fileId === selectedSearchHitId) ?? data?.items[0];
  const highScoreHits = data?.items.filter((item) => item.score >= 0.8).length ?? 0;

  return (
    <div className="flex-1 flex flex-col w-full h-full bg-white min-w-0">
      <PageSubbar title="搜索控制台" meta={`共 ${data?.total ?? 0} 项命中 / 高置信 ${highScoreHits} 项`}>
        <div className="shrink-0 p-3 flex flex-col gap-3">
          <div className="flex items-center gap-3">
            <div className="flex items-center bg-white border border-[#ccc] px-3 py-1.5 flex-1 focus-within:border-[#111] transition-colors">
              <span className="text-[#888] font-mono text-[11px] mr-2 shrink-0">SELECT * FROM</span>
              <input
                type="text"
                className="bg-transparent border-none outline-none text-[#111] font-mono text-[13px] w-full placeholder-[#aaa]"
                defaultValue={defaultQuery}
              />
              <button className="bg-[#111] text-white font-semibold text-[11px] px-3 py-0.5 ml-2 uppercase tracking-wider shrink-0 hover:bg-[#333]">
                执行
              </button>
            </div>
            <div className="flex items-center gap-2 border border-[#e0e0e0] bg-white px-3 py-1.5 text-[11px] text-[#666] cursor-pointer hover:text-[#111]">
              <Filter size={12} />
              <span>已保存查询</span>
            </div>
          </div>
          <div className="flex items-center gap-4 text-[11px] text-[#888] font-mono flex-wrap">
            <div className="flex items-center gap-1.5 cursor-pointer hover:text-[#111]">
              <span className="text-[#111]">范围:</span> 全局
            </div>
            <div className="flex items-center gap-1.5 cursor-pointer hover:text-[#111]">
              <span className="text-[#111]">模式:</span> SQL
            </div>
            <div className="flex items-center gap-1.5 cursor-pointer hover:text-[#111]">
              <span className="text-[#111]">过滤:</span> 文档 / 表格 / 大文件
            </div>
            <div className="ml-auto text-[#666]">找到 {data?.total ?? 0} 个结果 ({data?.tookMs ?? 0}ms)</div>
          </div>
        </div>
      </PageSubbar>

      <div className="flex-1 flex overflow-hidden min-h-0">
        <div className="flex-1 flex flex-col min-w-0 min-h-0">
          <div className="shrink-0 border-b border-[#e0e0e0] bg-[#fcfcfc] px-4 py-2 flex gap-2 text-[10px] uppercase tracking-wider text-[#666]">
            <span className="border border-[#d9d9d9] bg-white px-2 py-0.5">高分命中 {highScoreHits}</span>
            <span className="border border-[#d9d9d9] bg-white px-2 py-0.5">文档对象</span>
            <span className="border border-[#d9d9d9] bg-white px-2 py-0.5">上下文已提取</span>
          </div>

          <div className="flex-1 flex flex-col border-b border-[#e0e0e0] min-h-0">
            <DenseDataTable<SearchHit>
              rows={data?.items ?? []}
              getRowKey={(row) => row.fileId}
              selectedRowKey={selectedHit?.fileId}
              onRowClick={(row) => setSelectedSearchHitId(row.fileId)}
              emptyTitle="无搜索命中"
              emptyDescription="请调整检索语句、范围或过滤条件。"
              columns={[
                { key: 'path', title: '路径', className: 'w-[32%]', render: (row) => <span className="truncate block text-[#333]">{row.path}</span> },
                { key: 'score', title: '评分', className: 'w-24 text-[#666]', render: (row) => row.score.toFixed(2) },
                {
                  key: 'snippet',
                  title: '内容预览',
                  className: 'text-[#555]',
                  render: (row) => row.snippets[0]?.text ?? '-',
                },
              ]}
            />
          </div>

          <div className="h-56 bg-[#fcfcfc] flex flex-col shrink-0 min-h-0">
            <div className="h-7 border-b border-[#e0e0e0] flex items-center px-4 text-[10px] font-semibold uppercase text-[#555] tracking-wider shrink-0 bg-[#fafafa]">
              上下文预览
            </div>
            <div className="flex-1 overflow-auto p-4 font-mono text-[11px] text-[#444] leading-[1.6]">
              <div className="text-[#888] mb-2">在偏移 0x00A145 处找到匹配项</div>
              <div className="bg-white border border-[#e0e0e0] p-3 text-[#333] whitespace-pre-wrap">
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
              <div className="space-y-2 text-[10px] font-mono text-[#666]">
                <div className="flex items-center justify-between border border-[#e0e0e0] bg-white p-2">
                  <span>score</span>
                  <span className="text-[#111]">{selectedHit?.score.toFixed(2) ?? '-'}</span>
                </div>
                <div className="flex items-center justify-between border border-[#e0e0e0] bg-white p-2">
                  <span>snippet_count</span>
                  <span className="text-[#111]">{selectedHit?.snippets.length ?? 0}</span>
                </div>
              </div>
            </InspectorSection>

            <InspectorSection title="命中片段">
              <InspectorValue value={selectedHit?.snippets[0]?.text ?? '-'} mono />
            </InspectorSection>

            <InspectorSection title="关联动作">
              <button className="w-full border border-[#ccc] bg-white text-[#111] hover:bg-[#f0f0f0] py-1.5 text-center font-sans text-[11px] transition-colors rounded-[2px] cursor-pointer shadow-sm flex items-center justify-center gap-1.5">
                在文件浏览中打开 <ChevronRight size={12} />
              </button>
            </InspectorSection>
          </div>
        </InspectorPane>
      </div>
    </div>
  );
}
