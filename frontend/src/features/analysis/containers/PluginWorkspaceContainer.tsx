import type { PluginModule } from '@/types/models';
import { errorMessage } from '@/lib/errors';
import { useRunPluginExtraction } from '@/features/analysis/hooks';
import { PluginWorkspaceView } from '../components/PluginWorkspaceView';
import { PluginModulePanel } from './PluginModulePanel';

interface PluginWorkspaceContainerProps {
  dataSourceId: string;
  module?: PluginModule;
  loadContextKey: string;
}

export function PluginWorkspaceContainer({ dataSourceId, module, loadContextKey }: PluginWorkspaceContainerProps) {
  const run = useRunPluginExtraction();
  return (
    <PluginWorkspaceView
      running={run.isPending}
      error={run.error ? errorMessage(run.error) : undefined}
      onRun={() => run.mutate({ dataSourceId })}
    >
      {module ? (
        <PluginModulePanel dataSourceId={dataSourceId} module={module} loadContextKey={loadContextKey} />
      ) : null}
    </PluginWorkspaceView>
  );
}
