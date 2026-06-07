import { Clock, ZoomIn, ZoomOut, ChevronRight } from 'lucide-react';
import { useMemo, useState } from 'react';
import { useNavigate } from 'react-router';
import { PageSubbar } from '@/components/layout/PageSubbar';
import { DenseDataTable } from '@/components/tables/DenseDataTable';
import { InspectorPane, InspectorSection, InspectorValue } from '@/components/layout/InspectorPane';
import { useTimelineEvents } from '@/features/timeline/hooks';
import { useSelectionStore } from '@/stores/selection-store';
import { TimelineEventDto } from '@/types/models';

function buildTimelineBars(events: TimelineEventDto[], bucketCount: number): number[] {
  if (!events || events.length === 0) return Array(bucketCount).fill(0);
  const timestamps = events
    .map((e) => Date.parse(e.ts))
    .filter((t) => !isNaN(t));
  if (timestamps.length === 0) return Array(bucketCount).fill(0);
  const min = Math.min(...timestamps);
  const max = Math.max(...timestamps);
  const range = max - min || 1;
  const buckets = Array(bucketCount).fill(0);
  for (const ts of timestamps) {
    const idx = Math.min(Math.floor(((ts - min) / range) * bucketCount), bucketCount - 1);
    buckets[idx]++;
  }
  const maxCount = Math.max(...buckets, 1);
  return buckets.map((c) => (c / maxCount) * 100);
}

function formatTs(ts: string): string {
  const d = new Date(ts);
  if (isNaN(d.getTime())) return ts.slice(0, 16);
  return d.toLocaleString('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' });
}

export function Timeline() {
  const navigate = useNavigate();
  const [timeStart, setTimeStart] = useState('');
  const [timeEnd, setTimeEnd] = useState('');
  const [eventType, setEventType] = useState('');
  const normalizedTimeStart = useMemo(
    () => (timeStart ? new Date(timeStart).toISOString() : undefined),
    [timeStart],
  );
  const normalizedTimeEnd = useMemo(
    () => (timeEnd ? new Date(timeEnd).toISOString() : undefined),
    [timeEnd],
  );
  const { data: timelineData } = useTimelineEvents({
    offset: 0,
    limit: 100,
    timeStart: normalizedTimeStart,
    timeEnd: normalizedTimeEnd,
    eventType: eventType || undefined,
  });
  const events = timelineData?.items;
  const selectedTimelineId = useSelectionStore((state) => state.selectedTimelineId);
  const setSelectedTimelineId = useSelectionStore((state) => state.setSelectedTimelineId);
  const setSelectedFileId = useSelectionStore((state) => state.setSelectedFileId);
  const setSelectedArtifactId = useSelectionStore((state) => state.setSelectedArtifactId);
  const selectedEvent = events?.find((event) => event.id === selectedTimelineId) ?? events?.[0];
  const sourceCount = new Set((events ?? []).map((event) => String(event.attrs.source ?? '-'))).size;
  const eventTypes = useMemo(
    () => Array.from(new Set((events ?? []).map((event) => event.eventType))).sort(),
    [events],
  );

  const bars = useMemo(() => buildTimelineBars(events ?? [], 60), [events]);

  const timeRange = useMemo(() => {
    if (!events || events.length === 0) return { start: '-', end: '-' };
    const timestamps = events.map((e) => Date.parse(e.ts)).filter((t) => !isNaN(t));
    if (timestamps.length === 0) return { start: '-', end: '-' };
    const min = new Date(Math.min(...timestamps));
    const max = new Date(Math.max(...timestamps));
    return { start: formatTs(min.toISOString()), end: formatTs(max.toISOString()) };
  }, [events]);

  function jumpToSource() {
    if (!selectedEvent) return;
    const sourceId = selectedEvent.sourceObjectId;
    if (sourceId.startsWith('artifact:')) {
      setSelectedArtifactId(sourceId.replace(/^artifact:/, ''));
      navigate('/artifacts');
      return;
    }
    setSelectedFileId(sourceId);
    navigate('/files');
  }

  return (
    <div className="flex-1 flex flex-col w-full h-full bg-white min-w-0">
      <PageSubbar title="时间线控制带" meta={`事件 ${events?.length ?? 0} 条 / 数据源 ${sourceCount} 个`}>
        <div className="min-h-10 shrink-0 flex items-center px-4 py-1 justify-between gap-3 overflow-x-auto">
          <div className="flex items-center gap-4 whitespace-nowrap">
            <div className="flex items-center gap-2 text-[#666] text-[11px] font-mono">
              <Clock size={12} className="text-[#888]" />
              <span className="text-[#111]">{timeRange.start}</span>
              <span className="text-[#aaa]">至</span>
              <span className="text-[#111]">{timeRange.end}</span>
            </div>
            <div className="border-l border-[#e0e0e0] h-4"></div>
            <div className="flex items-center gap-2 text-[#888] text-[11px]">
              粒度:
              <span className="text-[#111] bg-white px-1.5 py-0.5 border border-[#ccc]">自适应</span>
            </div>
            <div className="border-l border-[#e0e0e0] h-4"></div>
            <label className="flex items-center gap-1.5 text-[#888] text-[11px]">
              起始
              <input
                type="datetime-local"
                value={timeStart}
                onChange={(event) => setTimeStart(event.target.value)}
                className="border border-[#ccc] bg-white px-1.5 py-0.5 text-[#111] font-mono"
              />
            </label>
            <label className="flex items-center gap-1.5 text-[#888] text-[11px]">
              结束
              <input
                type="datetime-local"
                value={timeEnd}
                onChange={(event) => setTimeEnd(event.target.value)}
                className="border border-[#ccc] bg-white px-1.5 py-0.5 text-[#111] font-mono"
              />
            </label>
            <label className="flex items-center gap-1.5 text-[#888] text-[11px]">
              类型
              <select
                value={eventType}
                onChange={(event) => setEventType(event.target.value)}
                className="border border-[#ccc] bg-white px-1.5 py-0.5 text-[#111]"
              >
                <option value="">全部</option>
                {eventTypes.map((type) => (
                  <option key={type} value={type}>
                    {type}
                  </option>
                ))}
              </select>
            </label>
            <button
              type="button"
              onClick={() => {
                setTimeStart('');
                setTimeEnd('');
                setEventType('');
              }}
              className="border border-[#ccc] bg-white px-2 py-0.5 text-[11px] text-[#666] hover:text-[#111] hover:bg-[#f0f0f0]"
            >
              清除
            </button>
          </div>
          <div className="flex items-center gap-2">
            <button className="p-1 hover:bg-[#f0f0f0] text-[#666] hover:text-[#111] rounded"><ZoomOut size={14} /></button>
            <button className="p-1 hover:bg-[#f0f0f0] text-[#666] hover:text-[#111] rounded"><ZoomIn size={14} /></button>
          </div>
        </div>
      </PageSubbar>

      <div className="h-24 border-b border-[#e0e0e0] bg-[#fcfcfc] shrink-0 flex flex-col p-2">
        <div className="flex-1 flex items-end gap-[1px] px-2">
          {bars.map((height, i) => (
            <div
              key={i}
              className={`flex-1 transition-colors ${height > 0 ? 'bg-[#111]' : 'bg-[#e8e8e8]'}`}
              style={{ height: `${Math.max(height, 4)}%` }}
              title={`${Math.round(height)}%`}
            />
          ))}
        </div>
        <div className="flex justify-between px-2 pt-1 text-[9px] font-mono text-[#888] mt-1">
          <span>{timeRange.start}</span>
          <span className="text-[#333] font-medium">
            {events && events.length > 0 ? formatTs(events[Math.floor(events.length / 2)].ts) : ''}
          </span>
          <span>{timeRange.end}</span>
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
                <div>window: {selectedEvent?.ts ?? '-'} ± 10m</div>
              </div>
            </InspectorSection>

            <InspectorSection title="关联动作">
              <button
                type="button"
                onClick={jumpToSource}
                disabled={!selectedEvent}
                className="w-full font-mono text-[#555] text-[11px] break-all border border-[#ccc] p-2 flex items-center justify-between hover:bg-[#f0f0f0] bg-white cursor-pointer transition-colors disabled:opacity-50"
              >
                <span className="font-medium">跳转到来源对象</span>
                <ChevronRight size={12} className="text-[#888]" />
              </button>
            </InspectorSection>
          </div>
        </InspectorPane>
      </div>
    </div>
  );
}
