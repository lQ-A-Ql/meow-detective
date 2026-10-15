import { useCorrelationWorkspaceModel } from '../use-correlation-workspace-model';
import { CorrelationWorkspaceView } from '../components/CorrelationWorkspaceView';
export function CorrelationWorkspace(props: Parameters<typeof useCorrelationWorkspaceModel>[0]) { return <CorrelationWorkspaceView model={useCorrelationWorkspaceModel(props)} />; }
