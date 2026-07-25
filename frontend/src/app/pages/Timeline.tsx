import { Clock, ZoomIn, ZoomOut, ChevronRight } from 'lucide-react';
import { useCallback, useMemo, useState } from 'react';
import { Button } from '@/app/components/ui/button';
import { Input } from '@/app/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/app/components/ui/select';
import { PageSubbar } from '@/components/layout/PageSubbar';
import { DenseColumn, DenseDataTable } from '@/components/tables/DenseDataTable';
import {
  InspectorPane,
  InspectorSection,
  InspectorValue,
} from '@/components/layout/InspectorPane';
import { useTimelineEventById, useTimelineEvents } from '@/features/timeline/hooks';
import { useTimelineSelectionModel } from '@/features/timeline/use-timeline-page-model';
import { TimelineEvent } from '@/types/models';

// Module-level columns: stable reference keeps memoized table rows from
// re-rendering on unrelated state changes.
const TIMELINE_COLUMNS: DenseColumn<TimelineEvent>[] = [
  {
    key: 'ts',
    title: '时间戳',
    className: 'w-36 text-forensics-muted',
    render: (row) => row.ts,
  },
  {
    key: 'source',
    title: '数据源',
    className: 'w-28 text-forensics-muted-light',
    render: (row) => String(row.attrs.source ?? '-'),
  },
  {
    key: 'eventType',
    title: '类型',
    className: 'w-28 text-forensics-muted',
    render: (row) => row.eventType,
  },
  {
    key: 'title',
    title: '描述',
    className: 'text-forensics-text-secondary',
    render: (row) => row.title,
  },
];

function buildTimelineBars(
  events: TimelineEvent[],
  bucketCount: number,
): Array<{ height: number; count: number }> {
  if (!events || events.length === 0) {
    return Array(bucketCount).fill({ height: 0, count: 0 });
  }
  const timestamps = events.map((e) => Date.parse(e.ts)).filter((t) => !isNaN(t));
  if (timestamps.length === 0) {
    return Array(bucketCount).fill({ height: 0, count: 0 });
  }
  const min = Math.min(...timestamps);
  const max = Math.max(...timestamps);
  const range = max - min || 1;
  const counts = Array(bucketCount).fill(0);
  for (const ts of timestamps) {
    const idx = Math.min(Math.floor(((ts - min) / range) * bucketCount), bucketCount - 1);
    counts[idx]++;
  }
  const maxCount = Math.max(...counts, 1);
  return counts.map((count) => ({ height: (count / maxCount) * 100, count }));
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

function isValidDateInput(value: string): boolean {
  return value === '' || !isNaN(Date.parse(value));
}

function toIsoOrUndefined(value: string): string | undefined {
  return value && isValidDateInput(value) ? new Date(value).toISOString() : undefined;
}

const MIN_BUCKET_COUNT = 20;
const MAX_BUCKET_COUNT = 180;
const BUCKET_STEP = 20;
const DEFAULT_BUCKET_COUNT = 60;

export function Timeline() {
  const [draftTimeStart, setDraftTimeStart] = useState('');
  const [draftTimeEnd, setDraftTimeEnd] = useState('');
  const [timeStart, setTimeStart] = useState('');
  const [timeEnd, setTimeEnd] = useState('');
  const [eventType, setEventType] = useState('');
  const [bucketCount, setBucketCount] = useState(DEFAULT_BUCKET_COUNT);
  const draftDatesValid = isValidDateInput(draftTimeStart) && isValidDateInput(draftTimeEnd);
  const normalizedTimeStart = useMemo(() => toIsoOrUndefined(timeStart), [timeStart]);
  const normalizedTimeEnd = useMemo(() => toIsoOrUndefined(timeEnd), [timeEnd]);
  const { data: timelineData } = useTimelineEvents({
    offset: 0,
    limit: 100,
    timeStart: normalizedTimeStart,
    timeEnd: normalizedTimeEnd,
    eventType: eventType || undefined,
  });
  const events = timelineData?.items ?? [];
  const {
    eventLookupId,
    jumpToSource,
    selectedTimelineId,
    setSelectedTimelineId,
  } = useTimelineSelectionModel();
  const selectedTimelineEvent = useTimelineEventById(eventLookupId);
  const handleEventRowClick = useCallback(
    (row: TimelineEvent) => setSelectedTimelineId(row.id),
    [setSelectedTimelineId],
  );

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

  const sourceCount = useMemo(
    () => new Set(events.map((event) => String(event.attrs.source ?? '-'))).size,
    [events],
  );
  const eventTypes = useMemo(
    () => Array.from(new Set(events.map((event) => event.eventType))).sort(),
    [events],
  );

  const bars = useMemo(() => buildTimelineBars(events, bucketCount), [events, bucketCount]);

  function zoomIn() {
    setBucketCount((current) => Math.min(MAX_BUCKET_COUNT, current + BUCKET_STEP));
  }

  function zoomOut() {
    setBucketCount((current) => Math.max(MIN_BUCKET_COUNT, current - BUCKET_STEP));
  }

  function applyDateRange() {
    if (!draftDatesValid) return;
    setTimeStart(draftTimeStart);
    setTimeEnd(draftTimeEnd);
  }

  const timeRange = useMemo(() => {
    if (events.length === 0) return { start: '-', end: '-' };
    const timestamps = events.map((e) => Date.parse(e.ts)).filter((t) => !isNaN(t));
    if (timestamps.length === 0) return { start: '-', end: '-' };
    const min = new Date(Math.min(...timestamps));
    const max = new Date(Math.max(...timestamps));
    return { start: formatTs(min.toISOString()), end: formatTs(max.toISOString()) };
  }, [events]);

  return (
    <div className="flex h-full min-w-0 flex-1 flex-col bg-forensics-surface">
      <PageSubbar title="时间线控制带" meta={`事件 ${events.length} 条 / 数据源 ${sourceCount} 个`}>
        <div className="flex min-h-10 shrink-0 items-center justify-between gap-3 overflow-x-auto px-4 py-1">
          <div className="flex items-center gap-4 whitespace-nowrap">
            <div className="flex items-center gap-2 font-mono text-[11px] text-forensics-muted">
              <Clock size={12} className="text-forensics-muted-light" />
              <span className="text-forensics-text">{timeRange.start}</span>
              <span className="text-forensics-500">至</span>
              <span className="text-forensics-text">{timeRange.end}</span>
            </div>
            <div className="h-4 border-l border-forensics-border" />
            <div className="flex items-center gap-2 text-[11px] text-forensics-muted-light">
              粒度:
              <span className="border border-forensics-border-strong bg-forensics-surface px-1.5 py-0.5 text-forensics-text">
                自适应
              </span>
            </div>
            <div className="h-4 border-l border-forensics-border" />
            <label className="flex items-center gap-1.5 text-[11px] text-forensics-muted-light">
              起始
              <Input
                type="datetime-local"
                value={draftTimeStart}
                onChange={(event) => setDraftTimeStart(event.target.value)}
                variant="mono"
                inputSize="inline"
                className={isValidDateInput(draftTimeStart) ? '' : 'border-forensics-error-border'}
              />
            </label>
            <label className="flex items-center gap-1.5 text-[11px] text-forensics-muted-light">
              结束
              <Input
                type="datetime-local"
                value={draftTimeEnd}
                onChange={(event) => setDraftTimeEnd(event.target.value)}
                variant="mono"
                inputSize="inline"
                className={isValidDateInput(draftTimeEnd) ? '' : 'border-forensics-error-border'}
              />
            </label>
            {!draftDatesValid ? (
              <span className="text-[11px] text-forensics-error-text">日期无效</span>
            ) : null}
            <Button
              type="button"
              variant="forensicsOutline"
              size="compact"
              onClick={applyDateRange}
              disabled={!draftDatesValid}
            >
              应用
            </Button>
            <label className="flex items-center gap-1.5 text-[11px] text-forensics-muted-light">
              类型
              <Select
                value={eventType}
                onValueChange={(value) => setEventType(value === '__all__' ? '' : value)}
              >
                <SelectTrigger variant="forensics" size="xs" className="w-28">
                  <SelectValue placeholder="全部" />
                </SelectTrigger>
                <SelectContent>
                <SelectItem value="__all__">全部</SelectItem>
                {eventTypes.map((type) => (
                  <SelectItem key={type} value={type}>
                    {type}
                  </SelectItem>
                ))}
                </SelectContent>
              </Select>
            </label>
            <Button
              type="button"
              variant="forensicsOutline"
              size="compact"
              onClick={() => {
                setDraftTimeStart('');
                setDraftTimeEnd('');
                setTimeStart('');
                setTimeEnd('');
                setEventType('');
              }}
            >
              清除
            </Button>
          </div>
          <div className="flex items-center gap-2">
            <Button
              type="button"
              variant="forensicsGhost"
              size="iconSm"
              onClick={zoomOut}
              disabled={bucketCount <= MIN_BUCKET_COUNT}
              aria-label="缩小"
            >
              <ZoomOut size={14} />
            </Button>
            <Button
              type="button"
              variant="forensicsGhost"
              size="iconSm"
              onClick={zoomIn}
              disabled={bucketCount >= MAX_BUCKET_COUNT}
              aria-label="放大"
            >
              <ZoomIn size={14} />
            </Button>
          </div>
        </div>
      </PageSubbar>

      <div className="flex min-h-24 shrink-0 flex-col border-b border-forensics-border bg-forensics-panel p-2">
        <div className="flex flex-1 items-end gap-[1px] px-2">
          {bars.map((bar, i) => (
            <div
              key={i}
              className={`min-h-[1px] flex-1 transition-colors ${
                bar.count > 0 ? 'bg-forensics-text' : 'bg-forensics-250'
              }`}
              style={{ height: `${bar.height}%` }}
              title={`${bar.count} 条事件`}
            />
          ))}
        </div>
        <div className="mt-1 flex justify-between px-2 pt-1 font-mono text-[9px] text-forensics-muted-light">
          <span>{timeRange.start}</span>
          <span className="font-light text-forensics-text-secondary">
            {events.length > 0 ? formatTs(events[Math.floor(events.length / 2)].ts) : ''}
          </span>
          <span>{timeRange.end}</span>
        </div>
      </div>

      <div className="flex min-h-0 flex-1 overflow-hidden">
        <div className="min-w-0 flex-1 border-r border-forensics-border">
          <DenseDataTable<TimelineEvent>
            rows={tableEvents}
            getRowKey={(row) => row.id}
            selectedRowKey={selectedEvent?.id}
            onRowClick={handleEventRowClick}
            emptyTitle="当前时间范围无事件"
            emptyDescription="请扩大时间范围或调整事件过滤条件。"
            columns={TIMELINE_COLUMNS}
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
              <div className="space-y-1 font-mono text-[10px] text-forensics-muted">
                <div className="truncate max-w-full">source: {String(selectedEvent?.attrs.source ?? '-')}</div>
                <div className="truncate max-w-full">window: {selectedEvent?.ts ?? '-'} ± 10m</div>
              </div>
            </InspectorSection>

            <InspectorSection title="关联动作">
              <Button
                type="button"
                variant="forensicsSurface"
                size="xs"
                onClick={() => jumpToSource(selectedEvent)}
                disabled={!selectedEvent}
                className="w-full justify-between font-mono text-forensics-text-tertiary"
              >
                <span className="font-light">跳转到来源对象</span>
                <ChevronRight size={12} className="text-forensics-muted-light" />
              </Button>
            </InspectorSection>
          </div>
        </InspectorPane>
      </div>
    </div>
  );
}
