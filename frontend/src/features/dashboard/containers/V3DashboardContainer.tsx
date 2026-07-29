import { V3DashboardWorkspace } from '@/features/dashboard/components/V3DashboardWorkspace';
import { useV3DashboardModel } from '@/features/dashboard/use-v3-dashboard-model';

export function V3DashboardContainer() {
  const model = useV3DashboardModel();
  return <V3DashboardWorkspace model={model} />;
}
