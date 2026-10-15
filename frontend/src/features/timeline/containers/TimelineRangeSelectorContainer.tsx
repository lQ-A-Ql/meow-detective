import { useTimelineRangeSelectorModel } from '../use-timeline-range-selector-model';
import { TimelineRangeSelectorView } from '../components/TimelineRangeSelectorView';
export function TimelineRangeSelector(props: Parameters<typeof useTimelineRangeSelectorModel>[0]) { return <TimelineRangeSelectorView model={useTimelineRangeSelectorModel(props)} />; }
