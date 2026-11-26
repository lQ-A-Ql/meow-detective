import { LogWorkspaceView } from '../components/LogWorkspaceView';
import { useLogWorkspaceModel } from '../use-log-workspace-model';

export function LogWorkspaceContainer() {
  const model = useLogWorkspaceModel();
  return <LogWorkspaceView model={model} />;
}
