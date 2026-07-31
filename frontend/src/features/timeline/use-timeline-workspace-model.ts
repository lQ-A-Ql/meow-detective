import { useCallback, useMemo, useState } from 'react';
import {
  useInfiniteTimelineEvents,
  useTimelineEventById,
  useTimelineFacets,
} from '@/features/timeline/hooks';
import { useTimelineSelectionModel } from '@/features/timeline/use-timeline-page-model';
import type { TimelineEvent } from '@/types/models';

const MIN_BUCKET_COUNT = 20;
const MAX_BUCKET_COUNT = 180;
const BUCKET_STEP = 20;
const DEFAULT_BUCKET_COUNT = 60;

function buildTimelineBars(
  histogram: Array<{ count: number; startTs: string; endTs: string }>,
  bucketCount: number,
): Array<{ height: number; count: number; startTs?: string; endTs?: string }> {
  if (histogram.length === 0) {
    return Array.from({ length: bucketCount }, () => ({ height: 0, count: 0 }));
  }
  const counts = histogram.map((bucket) => bucket.count);
  const maxCount = Math.max(...counts, 1);
  return histogram.map((bucket) => ({
    height: (bucket.count / maxCount) * 100,
    count: bucket.count,
    startTs: bucket.startTs,
    endTs: bucket.endTs,
  }));
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

function toDateTimeLocal(timestamp: string): string | undefined {
  const date = new Date(timestamp);
  if (!Number.isFinite(date.getTime())) return undefined;
  const local = new Date(date.getTime() - date.getTimezoneOffset() * 60_000);
  return local.toISOString().slice(0, 19);
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
  const facetsQuery = useTimelineFacets({
    timeStart: normalizedTimeStart,
    timeEnd: normalizedTimeEnd,
    eventType: eventType || undefined,
    bucketCount,
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
  const totalEvents = facetsQuery.data?.totalEvents ?? 0;
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
    () => (facetsQuery.data?.eventTypes ?? []).map((entry) => entry.value),
    [facetsQuery.data?.eventTypes],
  );
  const sourceCount = facetsQuery.data?.dataSources.length ?? 0;
  const bars = useMemo(
    () => buildTimelineBars(facetsQuery.data?.histogram ?? [], bucketCount),
    [bucketCount, facetsQuery.data?.histogram],
  );
  const timeRange = useMemo(() => {
    if (!facetsQuery.data?.startTs || !facetsQuery.data.endTs) {
      return { start: '-', end: '-' };
    }
    return {
      start: formatTimestamp(facetsQuery.data.startTs),
      end: formatTimestamp(facetsQuery.data.endTs),
    };
  }, [facetsQuery.data?.endTs, facetsQuery.data?.startTs]);

  const applyDateRange = useCallback(() => {
    if (!draftDatesValid) {
      return;
    }
    setSelectedTimelineId(undefined);
    setTimeStart(draftTimeStart);
    setTimeEnd(draftTimeEnd);
  }, [draftDatesValid, draftTimeEnd, draftTimeStart, setSelectedTimelineId]);
  const clearFilters = useCallback(() => {
    setSelectedTimelineId(undefined);
    setDraftTimeStart('');
    setDraftTimeEnd('');
    setTimeStart('');
    setTimeEnd('');
    setEventType('');
  }, [setSelectedTimelineId]);
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
  const selectEventType = useCallback(
    (value: string) => {
      setSelectedTimelineId(undefined);
      setEventType(value === '__all__' ? '' : value);
    },
    [setSelectedTimelineId],
  );
  const selectTimeBucket = useCallback(
    (bar: { startTs?: string; endTs?: string }) => {
      if (!bar.startTs || !bar.endTs) return;
      const start = toDateTimeLocal(bar.startTs);
      const end = toDateTimeLocal(bar.endTs);
      if (!start || !end) return;
      setSelectedTimelineId(undefined);
      setDraftTimeStart(start);
      setDraftTimeEnd(end);
      setTimeStart(start);
      setTimeEnd(end);
    },
    [setSelectedTimelineId],
  );
  const loadNextPage = useCallback(() => {
    void timelineQuery.fetchNextPage();
  }, [timelineQuery]);
  const retry = useCallback(() => {
    void timelineQuery.refetch();
    void facetsQuery.refetch();
  }, [facetsQuery, timelineQuery]);

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
    initialLoadFailed: (timelineQuery.isError || facetsQuery.isError) && events.length === 0,
    onEventRowClick,
    applyDateRange,
    retry,
    selectedEvent,
    setDraftTimeEnd,
    setDraftTimeStart,
    selectEventType,
    selectTimeBucket,
    sourceCount,
    middleTimestamp: facetsQuery.data?.histogram.length
      ? formatTimestamp(facetsQuery.data.histogram[Math.floor(facetsQuery.data.histogram.length / 2)].startTs)
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
