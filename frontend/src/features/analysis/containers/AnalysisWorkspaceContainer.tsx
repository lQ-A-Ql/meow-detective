import { AnalysisWorkspace } from '@/features/analysis/components/AnalysisWorkspace';
import { useAnalysisWorkspaceModel } from '@/features/analysis/use-analysis-workspace-model';

export function AnalysisWorkspaceContainer() {
  const model = useAnalysisWorkspaceModel();
  return <AnalysisWorkspace model={model} />;
}
