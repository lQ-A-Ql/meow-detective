import { useCallback, useMemo, useState } from 'react';
import { useInfiniteTimelineEvents, useTimelineEventById } from '@/features/timeline/hooks';
import { useTimelineSelectionModel } from '@/features/timeline/use-timeline-page-model';
import type { TimelineEvent } from '@/types/models';

const MIN_BUCKET_COUNT = 20;
const MAX_BUCKET_COUNT = 180;
const BUCKET_STEP = 20;
const DEFAULT_BUCKET_COUNT = 60;

function buildTimelineBars(
  events: TimelineEvent[],
  bucketCount: number,
): Array<{ height: number; count: number }> {
  if (events.length === 0) {
    return Array.from({ length: bucketCount }, () => ({ height: 0, count: 0 }));
  }

  const timestamps = events.map((event) => Date.parse(event.ts)).filter(Number.isFinite);
  if (timestamps.length === 0) {
    return Array.from({ length: bucketCount }, () => ({ height: 0, count: 0 }));
  }

  const min = Math.min(...timestamps);
  const max = Math.max(...timestamps);
  const range = max - min || 1;
  const counts = Array<number>(bucketCount).fill(0);
  for (const timestamp of timestamps) {
    const index = Math.min(
      Math.floor(((timestamp - min) / range) * bucketCount),
      bucketCount - 1,
    );
    counts[index] += 1;
  }

  const maxCount = Math.max(...counts, 1);
  return counts.map((count) => ({ height: (count / maxCount) * 100, count }));
}

function formatTimestamp(timestamp: string): string {
  const date = new Date(timestamp);
  if (!Number.isFinite(date.getTime())) {
    return timestamp.slice(0, 16);
  }
  return date.toLocaleString('zh-CN', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
}

function isValidDateInput(value: string): boolean {
  return value === '' || Number.isFinite(Date.parse(value));
}

function toIsoOrUndefined(value: string): string | undefined {
  return value && isValidDateInput(value) ? new Date(value).toISOString() : undefined;
}

/** Owns timeline query, selection, filter state, and navigation orchestration. */
export function useTimelineWorkspaceModel() {
  const [draftTimeStart, setDraftTimeStart] = useState('');
  const [draftTimeEnd, setDraftTimeEnd] = useState('');
  const [timeStart, setTimeStart] = useState('');
  const [timeEnd, setTimeEnd] = useState('');
  const [eventType, setEventType] = useState('');
  const [bucketCount, setBucketCount] = useState(DEFAULT_BUCKET_COUNT);
  const draftDatesValid = isValidDateInput(draftTimeStart) && isValidDateInput(draftTimeEnd);
  const normalizedTimeStart = useMemo(() => toIsoOrUndefined(timeStart), [timeStart]);
  const normalizedTimeEnd = useMemo(() => toIsoOrUndefined(timeEnd), [timeEnd]);
  const timelineQuery = useInfiniteTimelineEvents({
    timeStart: normalizedTimeStart,
    timeEnd: normalizedTimeEnd,
    eventType: eventType || undefined,
  });
  const loadContextKey = JSON.stringify([
    normalizedTimeStart ?? null,
    normalizedTimeEnd ?? null,
    eventType || null,
  ]);
  const events = useMemo(
    () => timelineQuery.data?.pages.flatMap((page) => page.items) ?? [],
    [timelineQuery.data],
  );
  const totalEvents = timelineQuery.data?.pages[0]?.total ?? 0;
  const {
    eventLookupId,
    jumpToSource,
    selectedTimelineId,
    setSelectedTimelineId,
  } = useTimelineSelectionModel();
  const selectedTimelineEvent = useTimelineEventById(eventLookupId);

  const selectedEvent =
    events.find((event) => event.id === selectedTimelineId) ??
    (selectedTimelineId?.startsWith('artifact:')
      ? events.find((event) => event.sourceObjectId === selectedTimelineId)
      : undefined) ??
    selectedTimelineEvent.data ??
    events[0];
  const tableEvents = useMemo(() => {
    if (!selectedTimelineEvent.data || events.some((event) => event.id === selectedTimelineEvent.data?.id)) {
      return events;
    }
    return [selectedTimelineEvent.data, ...events];
  }, [events, selectedTimelineEvent.data]);
  const eventTypes = useMemo(
    () => Array.from(new Set(events.map((event) => event.eventType))).sort(),
    [events],
  );
  const sourceCount = useMemo(
    () => new Set(events.map((event) => String(event.attrs.source ?? '-'))).size,
    [events],
  );
  const bars = useMemo(() => buildTimelineBars(events, bucketCount), [bucketCount, events]);
  const timeRange = useMemo(() => {
    const timestamps = events.map((event) => Date.parse(event.ts)).filter(Number.isFinite);
    if (timestamps.length === 0) {
      return { start: '-', end: '-' };
    }
    return {
      start: formatTimestamp(new Date(Math.min(...timestamps)).toISOString()),
      end: formatTimestamp(new Date(Math.max(...timestamps)).toISOString()),
    };
  }, [events]);

  const applyDateRange = useCallback(() => {
    if (!draftDatesValid) {
      return;
    }
    setTimeStart(draftTimeStart);
    setTimeEnd(draftTimeEnd);
  }, [draftDatesValid, draftTimeEnd, draftTimeStart]);
  const clearFilters = useCallback(() => {
    setDraftTimeStart('');
    setDraftTimeEnd('');
    setTimeStart('');
    setTimeEnd('');
    setEventType('');
  }, []);
  const zoomIn = useCallback(() => {
    setBucketCount((current) => Math.min(MAX_BUCKET_COUNT, current + BUCKET_STEP));
  }, []);
  const zoomOut = useCallback(() => {
    setBucketCount((current) => Math.max(MIN_BUCKET_COUNT, current - BUCKET_STEP));
  }, []);
  const onEventRowClick = useCallback(
    (event: TimelineEvent) => setSelectedTimelineId(event.id),
    [setSelectedTimelineId],
  );
  const selectEventType = useCallback((value: string) => {
    setEventType(value === '__all__' ? '' : value);
  }, []);
  const loadNextPage = useCallback(() => {
    void timelineQuery.fetchNextPage();
  }, [timelineQuery]);
  const retry = useCallback(() => {
    void timelineQuery.refetch();
  }, [timelineQuery]);

  return {
    bars,
    bucketCount,
    clearFilters,
    draftDatesValid,
    draftTimeEnd,
    draftTimeStart,
    eventType,
    eventTypes,
    events,
    loadContextKey,
    loadNextPage,
    loadStateKey: timelineQuery.dataUpdatedAt,
    loadingMore: timelineQuery.isFetchingNextPage,
    loadMoreFailed: timelineQuery.isFetchNextPageError,
    hasMore: timelineQuery.hasNextPage,
    initialLoadFailed: timelineQuery.isError && events.length === 0,
    onEventRowClick,
    applyDateRange,
    retry,
    selectedEvent,
    setDraftTimeEnd,
    setDraftTimeStart,
    selectEventType,
    sourceCount,
    middleTimestamp: events.length > 0
      ? formatTimestamp(events[Math.floor(events.length / 2)].ts)
      : '',
    tableEvents,
    timeRange,
    totalEvents,
    canZoomIn: bucketCount < MAX_BUCKET_COUNT,
    canZoomOut: bucketCount > MIN_BUCKET_COUNT,
    zoomIn,
    zoomOut,
    jumpToSource,
  };
}

export type TimelineWorkspaceModel = ReturnType<typeof useTimelineWorkspaceModel>;
