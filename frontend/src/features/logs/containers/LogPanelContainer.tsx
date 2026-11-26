import { LogPanelView, type LogPanelViewProps } from '../components/LogPanelView';
import { useLogPanelModel } from '../use-log-panel-model';

export function LogPanelContainer(props: Omit<LogPanelViewProps, 'scroll'>) {
  const scroll = useLogPanelModel();
  return <LogPanelView {...props} scroll={scroll} />;
}
