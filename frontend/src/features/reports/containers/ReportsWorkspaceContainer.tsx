import { ReportsWorkspace } from '@/features/reports/components/ReportsWorkspace';
import { useReportsWorkspaceModel } from '@/features/reports/use-reports-workspace-model';

export function ReportsWorkspaceContainer() {
  const model = useReportsWorkspaceModel();
  return <ReportsWorkspace model={model} />;
}
