import { ActivityBar } from '@/components/navigation/ActivityBar';
import { useNavigationModel } from '@/features/shell/use-navigation-model';

export function ActivityBarContainer() {
  const model = useNavigationModel();
  return <ActivityBar model={model} />;
}
