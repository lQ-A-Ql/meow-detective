import { Clock, ZoomIn, ZoomOut, ChevronRight } from 'lucide-react';
import { PageSubbar } from '@/components/layout/PageSubbar';
import { DenseDataTable } from '@/components/tables/DenseDataTable';
import { InspectorPane, InspectorSection, InspectorValue } from '@/components/layout/InspectorPane';
import { useTimelineEvents } from '@/features/timeline/hooks';
import { useSelectionStore } from '@/stores/selection-store';
import { TimelineEventDto } from '@/types/models';

export function Timeline() {
  const { data: events } = useTimelineEvents();
  const selectedTimelineId = useSelectionStore((state) => state.selectedTimelineId);
  const setSelectedTimelineId = useSelectionStore((state) => state.setSelectedTimelineId);
  const selectedEvent = events?.find((event) => event.id === selectedTimelineId) ?? events?.[0];
  const sourceCount = new Set((events ?? []).map((event) => String(event.attrs.source ?? '-'))).size;

  return (
    <div className="flex-1 flex flex-col w-full h-full bg-white min-w-0">
      <PageSubbar title="时间线控制带" meta={`事件 ${events?.length ?? 0} 条 / 数据源 ${sourceCount} 个`}>
        <div className="h-10 shrink-0 flex items-center px-4 justify-between">
          <div className="flex items-center gap-4 flex-wrap">
            <div className="flex items-center gap-2 text-[#666] text-[11px] font-mono">
              <Clock size={12} className="text-[#888]" />
              <span className="text-[#111]">2026-05-10 00:00:00</span>
              <span className="text-[#aaa]">至</span>
              <span className="text-[#111]">2026-05-12 23:59:59</span>
            </div>
            <div className="border-l border-[#e0e0e0] h-4"></div>
            <div className="flex items-center gap-2 text-[#888] text-[11px]">
              粒度:
              <span className="text-[#111] bg-white px-1.5 py-0.5 border border-[#ccc]">小时</span>
            </div>
            <div className="flex items-center gap-2 text-[#888] text-[11px]">
              类型:
              <span className="text-[#111] bg-white px-1.5 py-0.5 border border-[#ccc]">全部事件</span>
            </div>
            <div className="flex items-center gap-2 text-[#888] text-[11px]">
              来源:
              <span className="text-[#111] bg-white px-1.5 py-0.5 border border-[#ccc]">文件系统 + 事件日志</span>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <button className="p-1 hover:bg-[#f0f0f0] text-[#666] hover:text-[#111] rounded"><ZoomOut size={14} /></button>
            <button className="p-1 hover:bg-[#f0f0f0] text-[#666] hover:text-[#111] rounded"><ZoomIn size={14} /></button>
          </div>
        </div>
      </PageSubbar>

      <div className="h-24 border-b border-[#e0e0e0] bg-[#fcfcfc] shrink-0 flex flex-col p-2">
        <div className="flex-1 flex items-end gap-[1px] px-2">
          {Array.from({ length: 60 }).map((_, i) => {
            const height = i === 45 ? 100 : i === 46 ? 80 : 8 + ((i * 17) % 26);
            const isHighlight = i === 45 || i === 46;
            return (
              <div
                key={i}
                className={`flex-1 ${isHighlight ? 'bg-[#111]' : 'bg-[#e0e0e0] hover:bg-[#ccc]'}`}
                style={{ height: `${height}%` }}
              />
            );
          })}
        </div>
        <div className="flex justify-between px-2 pt-1 text-[9px] font-mono text-[#888] mt-1">
          <span>05-10 00:00</span>
          <span>05-11 00:00</span>
          <span className="text-[#333] font-medium">05-11 14:00</span>
          <span>05-12 00:00</span>
        </div>
      </div>

      <div className="flex-1 flex overflow-hidden min-h-0">
        <div className="flex-1 min-w-0 border-r border-[#e0e0e0]">
          <DenseDataTable<TimelineEventDto>
            rows={events ?? []}
            getRowKey={(row) => row.id}
            selectedRowKey={selectedEvent?.id}
            onRowClick={(row) => setSelectedTimelineId(row.id)}
            emptyTitle="当前时间范围无事件"
            emptyDescription="请扩大时间范围或调整事件过滤条件。"
            columns={[
              { key: 'ts', title: '时间戳', className: 'w-36 text-[#666]', render: (row) => row.ts },
              { key: 'source', title: '数据源', className: 'w-28 text-[#888]', render: (row) => String(row.attrs.source ?? '-') },
              { key: 'eventType', title: '类型', className: 'w-28 text-[#666]', render: (row) => row.eventType },
              { key: 'title', title: '描述', className: 'text-[#333]', render: (row) => row.title },
            ]}
          />
        </div>

        <InspectorPane
          title="事件检查器"
          subtitle={selectedEvent ? `当前事件 ${selectedEvent.id}` : '未选择事件'}
          widthClassName="w-80"
        >
          <div className="space-y-5">
            <InspectorSection title="时间戳">
              <InspectorValue value={selectedEvent?.ts ?? '-'} mono strong />
            </InspectorSection>

            <InspectorSection title="事件类型">
              <InspectorValue value={selectedEvent?.eventType ?? '-'} />
            </InspectorSection>

            <InspectorSection title="源活动">
              <InspectorValue value={selectedEvent?.description ?? '-'} />
            </InspectorSection>

            <InspectorSection title="来源对象">
              <InspectorValue value={selectedEvent?.title ?? '-'} mono />
            </InspectorSection>

            <InspectorSection title="时间上下文">
              <div className="text-[10px] font-mono text-[#666] space-y-1">
                <div>source: {String(selectedEvent?.attrs.source ?? '-')}</div>
                <div>window: 2026-05-11 14:00 ± 10m</div>
              </div>
            </InspectorSection>

            <InspectorSection title="关联动作">
              <div className="font-mono text-[#555] text-[11px] break-all border border-[#ccc] p-2 flex items-center justify-between hover:bg-[#f0f0f0] bg-white cursor-pointer transition-colors">
                <span className="font-medium">跳转到来源对象</span>
                <ChevronRight size={12} className="text-[#888]" />
              </div>
            </InspectorSection>
          </div>
        </InspectorPane>
      </div>
    </div>
  );
}
