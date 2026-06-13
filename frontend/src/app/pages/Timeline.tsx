import { Clock, ZoomIn, ZoomOut, ChevronRight } from 'lucide-react';
import { useMemo, useState } from 'react';
import { useNavigate } from 'react-router';
import { PageSubbar } from '@/components/layout/PageSubbar';
import { DenseDataTable } from '@/components/tables/DenseDataTable';
import {
  InspectorPane,
  InspectorSection,
  InspectorValue,
} from '@/components/layout/InspectorPane';
import { useTimelineEventById, useTimelineEvents } from '@/features/timeline/hooks';
import { useSelectionStore } from '@/stores/selection-store';
import { TimelineEventDto } from '@/types/models';

function buildTimelineBars(events: TimelineEventDto[], bucketCount: number): number[] {
  if (!events || events.length === 0) return Array(bucketCount).fill(0);
  const timestamps = events.map((e) => Date.parse(e.ts)).filter((t) => !isNaN(t));
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
  return d.toLocaleString('zh-CN', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
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
  const events = timelineData?.items ?? [];
  const selectedTimelineId = useSelectionStore((state) => state.selectedTimelineId);
  const setSelectedTimelineId = useSelectionStore((state) => state.setSelectedTimelineId);
  const setSelectedFileId = useSelectionStore((state) => state.setSelectedFileId);
  const setSelectedArtifactId = useSelectionStore((state) => state.setSelectedArtifactId);
  const eventLookupId =
    selectedTimelineId && !selectedTimelineId.startsWith('artifact:')
      ? selectedTimelineId
      : undefined;
  const selectedTimelineEvent = useTimelineEventById(eventLookupId);

  const selectedEvent =
    events.find((event) => event.id === selectedTimelineId) ??
    (selectedTimelineId?.startsWith('artifact:')
      ? events.find((event) => event.sourceObjectId === selectedTimelineId)
      : undefined) ??
    selectedTimelineEvent.data ??
    events[0];

  const tableEvents = useMemo(() => {
    if (!selectedTimelineEvent.data) {
      return events;
    }
    if (events.some((event) => event.id === selectedTimelineEvent.data?.id)) {
      return events;
    }
    return [selectedTimelineEvent.data, ...events];
  }, [events, selectedTimelineEvent.data]);

  const sourceCount = new Set(events.map((event) => String(event.attrs.source ?? '-'))).size;
  const eventTypes = useMemo(
    () => Array.from(new Set(events.map((event) => event.eventType))).sort(),
    [events],
  );

  const bars = useMemo(() => buildTimelineBars(events, 60), [events]);

  const timeRange = useMemo(() => {
    if (events.length === 0) return { start: '-', end: '-' };
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
    <div className="flex h-full min-w-0 flex-1 flex-col bg-white">
      <PageSubbar title="时间线控制带" meta={`事件 ${events.length} 条 / 数据源 ${sourceCount} 个`}>
        <div className="flex min-h-10 shrink-0 items-center justify-between gap-3 overflow-x-auto px-4 py-1">
          <div className="flex items-center gap-4 whitespace-nowrap">
            <div className="flex items-center gap-2 font-mono text-[11px] text-[#666]">
              <Clock size={12} className="text-[#888]" />
              <span className="text-[#111]">{timeRange.start}</span>
              <span className="text-[#aaa]">至</span>
              <span className="text-[#111]">{timeRange.end}</span>
            </div>
            <div className="h-4 border-l border-[#e0e0e0]" />
            <div className="flex items-center gap-2 text-[11px] text-[#888]">
              粒度:
              <span className="border border-[#ccc] bg-white px-1.5 py-0.5 text-[#111]">
                自适应
              </span>
            </div>
            <div className="h-4 border-l border-[#e0e0e0]" />
            <label className="flex items-center gap-1.5 text-[11px] text-[#888]">
              起始
              <input
                type="datetime-local"
                value={timeStart}
                onChange={(event) => setTimeStart(event.target.value)}
                className="border border-[#ccc] bg-white px-1.5 py-0.5 font-mono text-[#111]"
              />
            </label>
            <label className="flex items-center gap-1.5 text-[11px] text-[#888]">
              结束
              <input
                type="datetime-local"
                value={timeEnd}
                onChange={(event) => setTimeEnd(event.target.value)}
                className="border border-[#ccc] bg-white px-1.5 py-0.5 font-mono text-[#111]"
              />
            </label>
            <label className="flex items-center gap-1.5 text-[11px] text-[#888]">
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
              className="border border-[#ccc] bg-white px-2 py-0.5 text-[11px] text-[#666] hover:bg-[#f0f0f0] hover:text-[#111]"
            >
              清除
            </button>
          </div>
          <div className="flex items-center gap-2">
            <button className="rounded p-1 text-[#666] hover:bg-[#f0f0f0] hover:text-[#111]">
              <ZoomOut size={14} />
            </button>
            <button className="rounded p-1 text-[#666] hover:bg-[#f0f0f0] hover:text-[#111]">
              <ZoomIn size={14} />
            </button>
          </div>
        </div>
      </PageSubbar>

      <div className="flex h-24 shrink-0 flex-col border-b border-[#e0e0e0] bg-[#fcfcfc] p-2">
        <div className="flex flex-1 items-end gap-[1px] px-2">
          {bars.map((height, i) => (
            <div
              key={i}
              className={`flex-1 transition-colors ${
                height > 0 ? 'bg-[#111]' : 'bg-[#e8e8e8]'
              }`}
              style={{ height: `${Math.max(height, 4)}%` }}
              title={`${Math.round(height)}%`}
            />
          ))}
        </div>
        <div className="mt-1 flex justify-between px-2 pt-1 font-mono text-[9px] text-[#888]">
          <span>{timeRange.start}</span>
          <span className="font-medium text-[#333]">
            {events.length > 0 ? formatTs(events[Math.floor(events.length / 2)].ts) : ''}
          </span>
          <span>{timeRange.end}</span>
        </div>
      </div>

      <div className="flex min-h-0 flex-1 overflow-hidden">
        <div className="min-w-0 flex-1 border-r border-[#e0e0e0]">
          <DenseDataTable<TimelineEventDto>
            rows={tableEvents}
            getRowKey={(row) => row.id}
            selectedRowKey={selectedEvent?.id}
            onRowClick={(row) => setSelectedTimelineId(row.id)}
            emptyTitle="当前时间范围无事件"
            emptyDescription="请扩大时间范围或调整事件过滤条件。"
            columns={[
              {
                key: 'ts',
                title: '时间戳',
                className: 'w-36 text-[#666]',
                render: (row) => row.ts,
              },
              {
                key: 'source',
                title: '数据源',
                className: 'w-28 text-[#888]',
                render: (row) => String(row.attrs.source ?? '-'),
              },
              {
                key: 'eventType',
                title: '类型',
                className: 'w-28 text-[#666]',
                render: (row) => row.eventType,
              },
              {
                key: 'title',
                title: '描述',
                className: 'text-[#333]',
                render: (row) => row.title,
              },
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
              <div className="space-y-1 font-mono text-[10px] text-[#666]">
                <div>source: {String(selectedEvent?.attrs.source ?? '-')}</div>
                <div>window: {selectedEvent?.ts ?? '-'} ± 10m</div>
              </div>
            </InspectorSection>

            <InspectorSection title="关联动作">
              <button
                type="button"
                onClick={jumpToSource}
                disabled={!selectedEvent}
                className="flex w-full cursor-pointer items-center justify-between border border-[#ccc] bg-white p-2 font-mono text-[11px] text-[#555] transition-colors hover:bg-[#f0f0f0] disabled:opacity-50"
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
