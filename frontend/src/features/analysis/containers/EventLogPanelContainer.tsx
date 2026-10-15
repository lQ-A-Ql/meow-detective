import { useEventLogPanelModel } from '../use-event-log-panel-model';
import { EventLogPanelView } from '../components/panels/EventLogPanelView';
export function EventLogPanel(props: Parameters<typeof useEventLogPanelModel>[0]) { return <EventLogPanelView model={useEventLogPanelModel(props)} />; }
